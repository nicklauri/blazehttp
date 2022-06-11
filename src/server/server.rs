use std::{
    cell::RefCell,
    io::ErrorKind,
    iter,
    net::ToSocketAddrs,
    rc::Rc,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use anyhow::{Context, Error, Result};
use async_channel::Sender;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use tokio::{
    net::TcpListener,
    runtime::{Builder, Runtime},
    time,
};
use tracing::{debug, error, info, warn};

use crate::{
    config::{Config, Scheme, SharedConfig},
    proto::Connection,
    worker::{Command, Worker},
};

#[derive(Debug)]
pub struct Server {
    id: u8,
    tx: Sender<Command>,
    runtime: Runtime,
    config: Config,
    workers: Vec<Worker>,
}

impl Server {
    pub fn new(config: Config) -> Result<Server> {
        let (tx, rx) = async_channel::bounded(config.max_connections_in_waiting);
        let server_id = Server::get_id();
        let config = config;
        let workers = (0..config.workers)
            .map(|_| Worker::new(rx.clone(), config.clone()))
            .collect::<Result<Vec<_>>>()?;

        let runtime = Server::create_server_runtime()?;

        Ok(Self {
            id: server_id,
            tx,
            runtime,
            config,
            workers,
        })
    }

    fn get_id() -> u8 {
        static SERVER_ID: AtomicU8 = AtomicU8::new(0);
        SERVER_ID.fetch_add(1, Ordering::Relaxed)
    }

    fn create_server_runtime() -> Result<Runtime> {
        let rt = Builder::new_current_thread()
            .enable_all()
            .thread_name("blaze-server")
            .build()
            .context("build Tokio runtime failed")?;

        Ok(rt)
    }

    #[inline]
    async fn send_task(&self, task: Command) -> Result<()> {
        Ok(self.tx.send(task).await?)
    }

    pub fn serve(mut self) -> Result<()> {
        let (scheme, server_addr) = {
            let addr = format!("{}:{}", self.config.addr, self.config.port);
            let scheme = self.config.scheme;
            (scheme, addr)
        };

        self.runtime.block_on(async {
            let mut server = TcpListener::bind(&server_addr).await?;

            info!("listen on {}://{}", scheme, server_addr);
            loop {
                match server.accept().await {
                    Ok((stream, addr)) => {
                        debug!("{:?}: incomming: {addr}", std::thread::current().id());
                        let conn = Connection::new_h1(addr, stream);
                        self.send_task(Command::Incomming(conn)).await;
                    }
                    Err(err) => {
                        error!("accept error: {err:#?}");
                        warn!("current commands: {}", self.tx.len());
                    }
                }
            }

            Ok::<_, Error>(())
        });

        for worker in self.workers.into_iter() {
            if let Err(err) = worker.join() {
                error!("worker error: {err:?}");
            }
        }

        Ok(())
    }
}
