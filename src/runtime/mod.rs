pub mod worker;

use std::{future::Future, mem, rc::Rc, sync::Arc};

use anyhow::{Context, Error, Result};
use async_channel::{Receiver, Sender};
use tokio::{
    runtime::{Builder, Runtime},
    sync::Notify as TokioNotify,
    task::JoinHandle as TokioJoinHandle,
};
use tracing::{debug, error, info};

use crate::{config::Config, err, error::BlazeRuntimeError};

use self::worker::Worker;

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

        info!("started {} worker{}", workers.len(), if workers.len() > 1 { "s" } else { "" });

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
    #[inline]
    pub fn spawner(&self) -> Spawner {
        Spawner::new(self.tx.clone())
    }

    /// Spawn a task in Blaze runtime (not in workers).
    #[inline]
    pub fn spawn<F>(&self, fut: F) -> TokioJoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.rt.spawn(fut)
    }

    /// Block current thread and run the task.
    #[inline]
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

        let res = self.run(async move {
            debug!("sending stop command to workers");
            for _ in 0..number_of_workers {
                spawner.send_command(Command::Stop).await?;
            }
            Ok::<_, Error>(())
        });

        if let Err(err) = res {
            error!("send shutdown signal to workers error: {err:?}");
        }

        debug!("join with worker threads: {} worker(s)", self.workers.len());
        let workers = mem::take(&mut self.workers);
        for worker in workers {
            let _ = worker.join();
        }

        info!("Server is now exitting");

        Ok(())
    }

    #[allow(dead_code)]
    pub fn shutdown(&mut self) -> Result<(), BlazeRuntimeError> {
        err!(self.workers.is_empty(), BlazeRuntimeError::WorkerStopped);

        self.shutdown_notify.notify_waiters();

        let workers = mem::take(&mut self.workers);
        for worker in workers {
            let _ = worker.join();
        }

        Ok(())
    }

    pub fn shutdown_handle(&self) -> ShutdownHandle {
        ShutdownHandle {
            number_of_workers: self.workers.len(),
            shutdown: self.shutdown_notify.clone(),
            spawner: self.spawner(),
        }
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

    /// Push a task (command) into the queue and wait for workers to execute.
    #[inline]
    pub async fn send_command(&self, task: Command) -> Result<()> {
        Ok(self.tx.send(task).await?)
    }

    /// Spawn task by executing closure. This function capable to spawn a future holding `!Send + !Sync` accross yield points.<br />
    /// **Note**:
    ///     - This does not apply for capturing `!Send + !Sync` data, only datas created in the future are applied.<br />
    ///     - The function to create future should not do anything but initiating and return future
    ///  to not block the runtime's thread.<br />
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
    #[inline]
    pub async fn spawn_task<F, Fut>(&self, task: F) -> Result<()>
    where
        F: FnOnce(Rc<Config>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        // The line is too long but we can't just separate it tho.
        // Very sad time :(
        Ok(self
            .tx
            .send(Command::Task(Box::new(move |config| Box::pin(task(config)))))
            .await?)
    }

    /// Return number of receivers, it should match with the number of workers.
    #[allow(dead_code)]
    #[inline]
    pub fn receiver_count(&self) -> usize {
        self.tx.receiver_count()
    }

    /// Count pending tasks.
    #[inline]
    pub fn pending_task_count(&self) -> usize {
        self.tx.len()
    }
}

pub struct ShutdownHandle {
    spawner: Spawner,
    shutdown: ShutdownNotity,
    number_of_workers: usize,
}

impl ShutdownHandle {
    #[allow(dead_code)]
    pub async fn shutdown(self) {
        for _ in 0..self.number_of_workers {
            // TODO: unwrap!
            self.spawner.send_command(Command::Stop).await.unwrap();
        }

        self.shutdown.notify_waiters();
    }

    #[allow(dead_code)]
    pub async fn shutdown_gracefully(self) {}
}
