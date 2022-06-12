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
use tokio::{net::TcpListener, time};
use tracing::{debug, error, info, warn};

use crate::{
    config::{Config, Scheme, SharedConfig},
    proto::Connection,
    runtime::{BlazeRuntime, Command},
};

pub struct Server {
    id: u8,
    runtime: BlazeRuntime,
    config: Config,
}

impl Server {
    pub fn new(config: Config) -> Result<Server> {
        let server_id = Server::get_id();
        let config = config;

        let runtime = BlazeRuntime::new(&config)?;

        Ok(Self {
            id: server_id,
            runtime,
            config,
        })
    }

    fn get_id() -> u8 {
        static SERVER_ID: AtomicU8 = AtomicU8::new(0);
        SERVER_ID.fetch_add(1, Ordering::Relaxed)
    }

    pub fn serve(mut self) -> Result<()> {
        let (scheme, server_addr) = {
            let addr = format!("{}:{}", self.config.addr, self.config.port);
            let scheme = self.config.scheme;
            (scheme, addr)
        };

        let spawner = self.runtime.spawner();

        self.runtime.run(async move {
            let mut server = TcpListener::bind(&server_addr).await?;

            info!("listen on {}://{}", scheme, server_addr);
            loop {
                match server.accept().await {
                    Ok((stream, addr)) => {
                        debug!("{:?}: incomming: {addr}", std::thread::current().id());

                        // let conn = Connection::new(addr, stream);

                        // spawner.spawn(Command::Incomming(conn)).await;
                        spawner.spawn_task(move |config| Box::pin(Connection::new(addr, stream).handle(config)));
                    }
                    Err(err) => {
                        error!("accept error: {err:#?}");
                        warn!("current commands: {}", spawner.pending_task_count());
                    }
                }
            }

            Ok::<_, Error>(())
        });

        self.runtime.graceful_shutdown()?;

        Ok(())
    }
}
