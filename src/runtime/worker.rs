use anyhow::{anyhow, bail, Context, Result};
use async_channel::{unbounded, Receiver};
use std::{
    cell::RefCell,
    future::{self, Future},
    net::SocketAddr,
    ops::ControlFlow,
    pin::Pin,
    rc::Rc,
    thread::{self, JoinHandle},
};
use tokio::{
    net::TcpStream,
    runtime::{Builder, Runtime},
    select,
    task::{self, JoinHandle as TokioJoinHandle, LocalSet},
};
use tracing::{debug, info};

use crate::{
    config::Config,
    proto::{self, Connection},
    server,
    util::{self, Notify},
};

use super::ShutdownNotity;

pub type WorkerHandle = JoinHandle<Result<()>>;
pub type TaskFn = dyn FnOnce(Rc<Config>) -> Pin<Box<dyn Future<Output = ()>>> + Send + Sync + 'static;
pub type Task = Box<TaskFn>;

thread_local! {
    static TASK_COUNTER: RefCell<u32> = RefCell::new(0);
}

pub enum Command {
    Incomming(Connection),
    Task(Task),
    Stop,
}

#[derive(Debug)]
pub struct Worker {
    id: usize,
    handle: JoinHandle<Result<()>>,
}

impl Worker {
    pub fn new(rx: Receiver<Command>, config: Config, shutdown: ShutdownNotity) -> Result<Worker> {
        let id = util::next_id();
        let handle = Worker::spawn_thread(id, rx, config, shutdown);

        Ok(Self { id, handle })
    }

    pub fn join(self) -> Result<()> {
        Ok(self.handle.join().map_err(|err| anyhow!("join error: {err:?}"))??)
    }

    fn spawn_thread(id: usize, rx: Receiver<Command>, config: Config, shutdown: ShutdownNotity) -> JoinHandle<Result<()>> {
        let handle = thread::spawn(move || {
            let rt = Builder::new_current_thread()
                .enable_all()
                .thread_name(format!("blaze-worker:{}", id))
                .on_thread_park(WorkerInner::on_thread_park)
                .build()?;

            let localset = LocalSet::new();
            let fut = localset.run_until(WorkerInner::new(id, rx, config, shutdown).run());

            rt.block_on(fut);
            rt.block_on(localset); // Drain all tasks in the set.

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
    fn new(id: usize, rx: Receiver<Command>, config: Config, shutdown: ShutdownNotity) -> Self {
        Self {
            id,
            rx,
            config: Rc::new(config),
            notify: Notify::new(),
            shutdown,
        }
    }

    async fn ready(&self) {
        let current_tasks = self.count_tasks();

        if current_tasks >= self.config.max_tasks_per_worker {
            debug!("reached max_tasks_per_worker: {}", self.config.max_tasks_per_worker);
            self.notify.notified().await;
        }
    }

    async fn handle_task(&self) -> ControlFlow<Result<()>> {
        // Handle backpressure.
        self.ready().await;

        match self.rx.recv().await {
            Ok(task) => match task {
                Command::Incomming(conn) => {
                    // WorkerInner::connection(1);
                    self.spawn_managed_task(conn.handle(self.config.clone()));
                }
                Command::Task(fut) => {
                    self.spawn_managed_task(fut(self.config.clone()));
                }
                Command::Stop => return ControlFlow::Break(Ok(())),
            },
            Err(err) => return ControlFlow::Break(Err(err.into())),
        }

        task::yield_now().await;
        ControlFlow::Continue(())
    }

    async fn worker_command(&self) -> Result<()> {
        // TODO: use oneshot to receive stop signal.
        info!("worker_command: pending");
        future::pending().await
    }

    async fn run(mut self) -> Result<()> {
        loop {
            select! {
                res = self.shutdown.notified() => {
                    // stop the worker.
                }

                res = self.handle_task() => {
                    // handle worker spawned task.
                }
            }
        }

        Ok(())
    }

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

    fn count_tasks(&self) -> u32 {
        self.notify.notifiers()
    }

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

    fn on_thread_park() {
        // info!(
        //     "[{:?}] on_thread_park: {} connections",
        //     thread::current().id(),
        //     Self::connection(0)
        // );
    }
}
