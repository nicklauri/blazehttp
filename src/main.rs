#![allow(warnings)]

use std::{env, sync::Arc, time::Duration};

use anyhow::Result;
use parking_lot::RwLock;

use crate::{config::Config, server::server::Server};

mod config;
mod error;
mod proto;
mod server;
mod util;
mod worker;

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let config = match env::args().nth(1) {
        Some(path) => {
            let content = std::fs::read_to_string(&path)?;
            let c: Config = ron::from_str(&content)?;
            c
        }
        None => Config::default(),
    };

    Server::new(config)?.serve()?;

    Ok(())
}
