#![allow(warnings)]

use std::{env, sync::Arc, time::Duration};

use anyhow::Result;
use parking_lot::RwLock;
use tokio::fs;

use crate::{config::Config, server::server::Server};

mod config;
mod error;
mod proto;
mod server;
mod util;
mod worker;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let config = match env::args().nth(1) {
        Some(path) => {
            let content = fs::read_to_string(&path).await?;
            let c: Config = ron::from_str(&content)?;
            c
        }
        None => Config::default(),
    };

    let config = Arc::new(config);

    Server::new(config)?.serve().await?;

    Ok(())
}
