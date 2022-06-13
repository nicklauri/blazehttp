use anyhow::{anyhow, Result};
use async_channel::Receiver;
use std::{
    cell::RefCell,
    future::Future,
    ops::ControlFlow,
    pin::Pin,
    rc::Rc,
    thread::{self, JoinHandle},
};
use tokio::{
    runtime::Builder,
    select,
    task::{self, JoinHandle as TokioJoinHandle, LocalSet},
};
use tracing::{debug, info};

use crate::{
    config::Config,
    proto,
    util::{self, Notify},
};

use super::ShutdownNotity;

pub type TaskFn = dyn FnOnce(Rc<Config>) -> Pin<Box<dyn Future<Output = ()>>> + Send + Sync + 'static;
pub type Task = Box<TaskFn>;

thread_local! {
    static TASK_COUNTER: RefCell<u32> = RefCell::new(0);
}

pub enum Command {
    H1(proto::Connection),
    Task(Task),
    Stop,
}

#[derive(Debug)]
pub struct Worker {
    id: usize,
    handle: JoinHandle<Result<()>>,
}

impl Worker {
    /// Create a new worker.
    pub fn new(rx: Receiver<Command>, config: Config, shutdown: ShutdownNotity) -> Result<Worker> {
        let id = util::next_id();
        let handle = Worker::spawn_thread(id, rx, config, shutdown);

        Ok(Self { id, handle })
    }

    /// Join with worker thread.
    pub fn join(self) -> Result<()> {
        Ok(self
            .handle
            .join()
            .map_err(|err| anyhow!("worker[{:>2}]: join error: {:?}", self.id, err))??)
    }

    fn spawn_thread(id: usize, rx: Receiver<Command>, config: Config, shutdown: ShutdownNotity) -> JoinHandle<Result<()>> {
        let handle = thread::spawn(move || {
            let rt = Builder::new_current_thread()
                .enable_all()
                .thread_name(format!("blaze-worker:{}", id))
                .build()?;

            let localset = LocalSet::new();
            let config = Rc::new(config);
            let fut = localset.run_until(WorkerInner::new(id, rx, config.clone(), shutdown).run());

            WorkerInner::on_thread_start(id, config.clone());

            rt.block_on(fut);
            rt.block_on(localset); // Drain all tasks in the set.

            WorkerInner::on_thread_stop(id, config);

            Ok(())
        });

        handle
    }
}

struct WorkerInner {
    id: usize,
    rx: Receiver<Command>,
    config: Rc<Config>,
    notify: Notify,
    shutdown: ShutdownNotity,
}

impl WorkerInner {
    fn new(id: usize, rx: Receiver<Command>, config: Rc<Config>, shutdown: ShutdownNotity) -> Self {
        Self {
            id,
            rx,
            config,
            notify: Notify::new(),
            shutdown,
        }
    }

    #[inline]
    async fn ready(&self) {
        let current_tasks = self.count_tasks();

        if current_tasks >= self.config.max_tasks_per_worker {
            debug!(config.max_tasks_per_worker = ?self.config.max_tasks_per_worker,
                "worker[{:>2}] reached limit:", self.id);

            self.notify.notified().await;
        }
    }

    #[inline]
    async fn handle_task(&self) -> ControlFlow<Result<()>> {
        // Handle backpressure.
        self.ready().await;

        match self.rx.recv().await {
            Ok(task) => match task {
                Command::H1(conn) => {
                    self.spawn_managed_task(proto::handle_h1_connection(conn, self.config.clone()));
                }
                Command::Task(func) => {
                    self.spawn_managed_task(func(self.config.clone()));
                }
                Command::Stop => return ControlFlow::Break(Ok(())),
            },
            Err(err) => return ControlFlow::Break(Err(err.into())),
        }

        Self::connection(1);

        // Yield now help distribute tasks from async_channel.
        // TODO(maybe): Implement a fair distribute channel.
        task::yield_now().await;
        ControlFlow::Continue(())
    }

    async fn run(self) {
        let err = loop {
            select! {
                _ = self.shutdown.notified() => {
                    debug!("worker[{:>2}]: shutting down", self.id);
                    return;
                }

                res = self.handle_task() => {
                    match res {
                        ControlFlow::Continue(_) => continue,
                        ControlFlow::Break(result) => match result {
                            Ok(()) => {
                                // debug!("worker[{:>2}]: peacefully shutdown", self.id);
                                return
                            },
                            Err(err) => break err,
                        }
                    }
                }
            }
        };

        debug!("worker[{:>2}]: worker stopped with error: {err:#?}", self.id);
    }

    #[inline]
    fn spawn_managed_task<F>(&self, fut: F) -> TokioJoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let notifier = self.notify.notifier();

        util::spawn(async move {
            let res = fut.await;

            notifier.notify();

            res
        })
    }

    #[inline]
    fn count_tasks(&self) -> u32 {
        self.notify.notifiers()
    }

    #[inline]
    fn connection(amount: i32) -> i32 {
        thread_local! {
            static CONNS: RefCell<i32> = RefCell::new(0);
        }

        CONNS.with(|c| {
            let mut c = c.borrow_mut();

            *c += amount;
            *c
        })
    }

    fn on_thread_start(id: usize, _config: Rc<Config>) {
        debug!("worker[{:>2}]: thread start", id);
    }

    fn on_thread_stop(id: usize, config: Rc<Config>) {
        if config.display_statistics_on_shutdown {
            info!("worker[{:>2}]: total connections: {}", id, Self::connection(0));
        }
    }
}
