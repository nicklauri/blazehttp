#![allow(warnings)]

use std::{sync::Arc, time::Duration};

use anyhow::Result;
use parking_lot::RwLock;

use crate::{config::Config, server::server::Server};

mod config;
mod error;
mod proto;
mod server;
mod util;
mod worker;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    println!("server run at localhost:80");

    let config = Arc::new(RwLock::new(Config::default()));

    config.write().workers = 8;

    Server::new(config).serve().await?;

    Ok(())
}
