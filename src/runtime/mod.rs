pub mod worker;

use std::{future::Future, mem, pin::Pin, rc::Rc, sync::Arc, time::Duration};

use anyhow::{bail, Context, Result};
use async_channel::{Receiver, Sender};
use tokio::{
    runtime::{Builder, Runtime},
    sync::Notify as TokioNotify,
};

use crate::{config::Config, err, error::BlazeRuntimeError};

use self::worker::{Task, Worker};

pub use self::worker::Command;

pub type ShutdownNotity = Arc<TokioNotify>;

pub struct BlazeRuntime {
    rt: Runtime,
    tx: Sender<Command>,
    workers: Vec<Worker>,
    shutdown_notify: ShutdownNotity,
}

impl BlazeRuntime {
    /// Create new Blaze runtime.
    pub fn new(config: &Config) -> Result<Self> {
        let rt = Builder::new_current_thread()
            .enable_all()
            .thread_name("blaze-server")
            .build()
            .context("build Tokio runtime failed")?;

        let (tx, rx) = async_channel::bounded(config.max_connections_in_waiting);
        let shutdown_notify = Arc::new(TokioNotify::const_new());
        let workers = BlazeRuntime::spawn_workers(rx, config, shutdown_notify.clone())?;

        Ok(Self {
            rt,
            tx,
            workers,
            shutdown_notify,
        })
    }

    fn spawn_workers(rx: Receiver<Command>, config: &Config, shutdown: ShutdownNotity) -> Result<Vec<Worker>> {
        (0..config.workers)
            .map(|_| Worker::new(rx.clone(), config.clone(), shutdown.clone()))
            .collect::<Result<Vec<_>>>()
    }

    /// Create a spawner to send tasks to workers.
    pub fn spawner(&self) -> Spawner {
        Spawner::new(self.tx.clone())
    }

    /// Spawn a task in workers.
    pub async fn spawn(&self, task: Command) -> Result<()> {
        Ok(self.tx.send(task).await?)
    }

    /// Block current thread and run the task.
    pub fn run<F>(&self, task: F) -> Result<F::Output, BlazeRuntimeError>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        err!(self.workers.is_empty(), BlazeRuntimeError::RuntimeStopped);

        Ok(self.rt.block_on(task))
    }

    /// Gracefully shutdown the runtime.<br />
    /// This function will wait for all the remaining tasks (in queue or already spawned) to complete.
    pub fn graceful_shutdown(&mut self) -> Result<(), BlazeRuntimeError> {
        err!(self.workers.is_empty(), BlazeRuntimeError::WorkerStopped);

        let number_of_workers = self.workers.len();

        let spawner = self.spawner();

        self.run(async move {
            for i in 0..number_of_workers {
                spawner.spawn(Command::Stop).await;
            }
        });

        let workers = mem::take(&mut self.workers);
        for worker in workers {
            let _ = worker.join();
        }

        Ok(())
    }

    pub fn shutdown(&mut self) -> Result<(), BlazeRuntimeError> {
        err!(self.workers.is_empty(), BlazeRuntimeError::WorkerStopped);

        self.shutdown_notify.notify_waiters();

        let workers = mem::take(&mut self.workers);
        for worker in workers {
            let _ = worker.join();
        }

        Ok(())
    }
}

#[derive(Clone)]
pub struct Spawner {
    tx: Sender<Command>,
}

impl Spawner {
    fn new(tx: Sender<Command>) -> Self {
        Self { tx }
    }

    /// Push a task into the queue and wait for workers to execute.
    pub async fn spawn(&self, task: Command) -> Result<()> {
        Ok(self.tx.send(task).await?)
    }

    /// Spawn task by executing closure. This function capable to spawn a future holding `!Send + !Sync` accross yield points.<br />
    /// **Note**:
    ///     - This does not apply for capturing `!Send + !Sync` data, only datas created in the future are applied.<br />
    ///     - The function to create future must return immediately to not block worker thread.<br />
    /// Example:
    /// ```ignore
    /// async fn some_async_work() {}
    ///
    /// fn success(spawner: &Spawner) -> anyhow::Result<()> {
    ///     let data = String::from("Hello world!");
    ///
    ///     spawner.spawn_task(|| async move {
    ///         let mut data = Rc::new(RefCell::new(data));
    ///         some_async_work().await;
    ///         data.borrow_mut().push_str("It works!");
    ///     })
    /// }
    /// ```
    pub async fn spawn_task<F>(&self, task: F) -> Result<()>
    where
        F: FnOnce(Rc<Config>) -> Pin<Box<dyn Future<Output = ()>>> + Send + Sync + 'static,
    {
        Ok(self.tx.send(Command::Task(Box::new(task))).await?)
    }

    /// Return number of receivers, it should match with the number of workers.
    pub fn receiver_count(&self) -> usize {
        self.tx.receiver_count()
    }

    /// Count pending tasks.
    pub fn pending_task_count(&self) -> usize {
        self.tx.len()
    }
}
