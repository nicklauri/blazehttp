use std::{
    ops::ControlFlow,
    sync::atomic::{AtomicU8, Ordering},
};

use anyhow::{Error, Result};
use tokio::{net::TcpListener, select, time};
use tracing::{debug, error, info, warn};

use crate::{
    config::Config,
    proto,
    runtime::{BlazeRuntime, Command, Spawner},
};

pub struct Server {
    id: u8,
    runtime: BlazeRuntime,
    config: Config,
}

impl Server {
    pub fn new(config: Config) -> Result<Server> {
        let server_id = Server::get_id();
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
        let result = self.runtime.run(async {
            let server = TcpListener::bind(&server_addr).await?;
            let mut total_conns = 0;
            info!("listen on {}://{}", scheme, server_addr);
            // TODO: handle tcp/http, tls/http, h2
            // Note: tls/http and h2 can be on the same TcpListener, but tcp/http can't.
            // Currently, blazehttp can only handle tcp/http.

            loop {
                select! {
                    _ = self.accept_http_connection(&server, &spawner, &mut total_conns) => { }
                    _ = tokio::signal::ctrl_c() => { break }
                }
            }

            if self.config.display_statistics_on_shutdown {
                info!("total connections accepted: {}", total_conns);
            }

            Ok::<_, Error>(())
        });

        if let Err(err) = result {
            error!("server.serve: runtime error: {err:#?}");
        }

        info!("server is shutting down");

        self.runtime.graceful_shutdown()?;

        Ok(())
    }

    #[inline]
    async fn accept_http_connection(&self, server: &TcpListener, spawner: &Spawner, total_conn: &mut usize) -> ControlFlow<()> {
        match server.accept().await {
            Ok((stream, addr)) => {
                debug!(?addr, "server.accept");

                let conn = proto::Connection::new(stream, addr);
                let result = spawner.send_command(Command::H1(conn)).await;

                if let Err(err) = result {
                    warn!(?err, "spawner.spawn_task");
                }
            }
            Err(err) => {
                if std::io::Error::from_raw_os_error(24).kind() == err.kind() {
                    warn!(
                        sleep = ?self.config.accept_error_sleep,
                        spawner.pending_task_count = spawner.pending_task_count(),
                        "too many open files",
                    );
                }
                time::sleep(self.config.accept_error_sleep).await;
            }
        }

        *total_conn += 1;

        ControlFlow::Continue(())
    }
}
