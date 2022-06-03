use std::{iter, net::ToSocketAddrs};

use anyhow::Result;
use async_channel::{unbounded, Sender};
use tokio::net::TcpListener;

use crate::{
    config::{Config, GlobalConfig},
    proto::Connection,
    worker::{Command, Worker},
};

#[derive(Debug)]
pub struct Server {
    workers: Vec<Worker>,
    sender: Sender<Command>,
    config: GlobalConfig,
}

impl Server {
    pub fn new(config: GlobalConfig) -> Server {
        let (tx, rx) = unbounded();

        let num_workers = config.read().workers;

        let workers = iter::repeat_with(|| Worker::new(rx.clone(), config.clone()))
            .take(num_workers as _)
            .collect();

        Self {
            workers,
            sender: tx,
            config,
        }
    }

    pub async fn stop(&self) -> Result<()> {
        self.sender.send(Command::Stop).await?;
        Ok(())
    }

    pub fn detect_config_changes(&self) -> bool {
        false
    }
    pub fn apply_config_changes(&self) {}

    pub async fn serve(mut self) -> Result<()> {
        let server_addr = {
            let config = self.config.read();
            format!("{}:{}", config.addr, config.port)
        };

        let mut server = TcpListener::bind(server_addr).await?;

        loop {
            // Detect Ctrl-C.
            match server.accept().await {
                Ok((stream, addr)) => {
                    let conn = Connection::new_h1(addr, stream);
                    self.sender.send(Command::ProcessRequest(conn)).await;
                }
                Err(_) => {}
            }

            // Separate to serve without detect config changes
            if self.detect_config_changes() {
                self.apply_config_changes();
            }
        }

        Ok(())
    }
}
