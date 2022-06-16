use anyhow::{anyhow, Result};
use async_channel::Receiver;
use std::{
    cell::RefCell,
    future::Future,
    ops::ControlFlow,
    pin::Pin,
    process,
    rc::Rc,
    thread::{self, JoinHandle},
};
use tokio::{
    runtime::Builder,
    select,
    task::{self, JoinHandle as TokioJoinHandle, LocalSet},
};
use tracing::{debug, error, info};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerShutdownType {
    Graceful,
    Forced,
}

pub enum Command {
    /// Spawn an HTTP/1.1 connection.<br />
    /// This is like hard-coding, but it's okay since the sole purpose of
    /// this project is to make a static HTTP server, not a web framework.
    H1(proto::Connection),

    /// Run the lambda to get the future then execute it in the local executor.
    Task(Task),

    /// Signal the worker to stop
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
        let result = thread::Builder::new().name(format!("blaze-worker:{}", id)).spawn(move || {
            let rt = Builder::new_current_thread().enable_all().build()?;

            let localset = LocalSet::new();
            let config = Rc::new(config);
            let fut = localset.run_until(WorkerInner::new(id, rx, config.clone(), shutdown).run());

            WorkerInner::on_thread_start(id, config.clone());

            let shutdown_type = rt.block_on(fut);

            if matches!(shutdown_type, WorkerShutdownType::Graceful) {
                debug!("worker[{:>2}]: shutdown gracefully", id);
                rt.block_on(localset); // Drain all tasks in the set.
            } else {
                debug!("worker[{:>2}]: forcing shutdown", id);
            }

            WorkerInner::on_thread_stop(id, config);

            // debug!("worker[{:>2}]: worker is exitting", id);

            Ok(())
        });

        match result {
            Ok(handle) => handle,
            Err(err) => {
                error!("spawn thread for worker-{id} error: {err:#?}");
                process::exit(1);
            }
        }
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

    async fn run(self) -> WorkerShutdownType {
        let err = loop {
            select! {
                _ = self.shutdown.notified() => {
                    debug!("worker[{:>2}]: shutting down", self.id);
                    return WorkerShutdownType::Forced;
                }

                res = self.handle_task() => {
                    match res {
                        ControlFlow::Continue(_) => continue,
                        ControlFlow::Break(result) => match result {
                            Ok(()) => {
                                debug!("worker[{:>2}]: peacefully shutdown", self.id);
                                return WorkerShutdownType::Graceful;
                            },
                            Err(err) => break err,
                        }
                    }
                }
            }
        };

        debug!("worker[{:>2}]: worker stopped with error: {err:#?}", self.id);
        WorkerShutdownType::Forced
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
