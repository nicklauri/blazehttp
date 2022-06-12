use std::sync::atomic::{AtomicU8, Ordering};

use anyhow::{Error, Result};
use tokio::{net::TcpListener, select};
use tracing::{debug, error, info, warn};

use crate::{
    config::Config,
    proto::Connection,
    runtime::{BlazeRuntime, Spawner},
};

#[allow(dead_code)]
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

    fn register_ctrl_c(&self) {
        let shutdown_handle = self.runtime.shutdown_handle();

        self.runtime.spawn(async {
            if let Err(err) = tokio::signal::ctrl_c().await {
                warn!("register_ctrl_c: await error: {err}");
            }

            info!("server is shutting down");
            shutdown_handle.shutdown().await;
        });
    }

    pub fn serve(mut self) -> Result<()> {
        let (scheme, server_addr) = {
            let addr = format!("{}:{}", self.config.addr, self.config.port);
            let scheme = self.config.scheme;
            (scheme, addr)
        };

        self.register_ctrl_c();

        let spawner = self.runtime.spawner();

        let result = self.runtime.run(async move {
            let server = TcpListener::bind(&server_addr).await?;

            info!("listen on {}://{}", scheme, server_addr);
            loop {
                select! {
                    _ = Server::accept_connection(&server, &spawner) => {}
                    res = tokio::signal::ctrl_c() => {
                        res.unwrap();
                        break;
                    }
                }
            }

            Ok::<_, Error>(())
        });

        if let Err(err) = result {
            error!("runtime error: {err:#?}");
        }

        info!("server is shutting down");

        self.runtime.graceful_shutdown()?;

        Ok(())
    }

    #[inline]
    async fn accept_connection(server: &TcpListener, spawner: &Spawner) {
        match server.accept().await {
            Ok((stream, addr)) => {
                debug!(addr = ?addr, "server.accept()");

                let result = spawner
                    .spawn_task(move |config| Connection::new(addr, stream).handle(config))
                    .await;

                if let Err(err) = result {
                    warn!("spawner.spawn_task: {err:?}");
                }
            }
            Err(err) => {
                warn!("server.accept: {err:#?}");
                info!("spawner.pending_task_count(): {}", spawner.pending_task_count());
            }
        }
    }
}
