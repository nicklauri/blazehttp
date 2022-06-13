use std::{
    ops::ControlFlow,
    sync::atomic::{AtomicU8, Ordering},
};

use anyhow::{Error, Result};
use tokio::{net::TcpListener, select};
use tracing::{debug, error, info, warn};

use crate::{
    config::Config,
    proto,
    runtime::{BlazeRuntime, Spawner},
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
        let result = self.runtime.run(async move {
            let server = TcpListener::bind(&server_addr).await?;
            let mut c = 0;
            info!("listen on {}://{}", scheme, server_addr);
            // TODO: handle tcp/http, tls/http, h2
            // Note: tls/http and h2 can be on the same TcpListener, but tcp/http can't.
            // Currently, blazehttp can only handle tcp/http.
            loop {
                select! {
                    res = Server::accept_connection_h1(&server, &spawner, &mut c) => {
                        if res.is_break() {
                            break;
                        }
                    }
                    res = tokio::signal::ctrl_c() => {
                        if let Err(err) = res {
                            warn!("await CTRL-C signal error: {err:#?}");
                        }
                        break
                    }
                }
            }

            info!("total connections accepted: {}", c);

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
    async fn accept_connection_h1(server: &TcpListener, spawner: &Spawner, c: &mut usize) -> ControlFlow<()> {
        match server.accept().await {
            Ok((stream, addr)) => {
                debug!(?addr, "server.accept");

                let conn = proto::Connection::new(stream, addr);
                let result = spawner
                    .spawn_task(move |config| proto::handle_h1_connection(conn, config))
                    .await;

                if let Err(err) = result {
                    warn!(?err, "spawner.spawn_task");
                }
            }
            Err(err) => {
                warn!("server.accept: {err:#?}");
                info!("spawner.pending_task_count: {}", spawner.pending_task_count());

                if std::io::Error::from_raw_os_error(24).kind() == err.kind() {
                    error!("too many files opened");
                }

                return ControlFlow::Break(());
            }
        }

        // TODO: REMOVE DEBUG
        *c += 1;

        ControlFlow::Continue(())
    }
}
