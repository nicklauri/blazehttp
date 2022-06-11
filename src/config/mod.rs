pub mod path;

use std::{fmt::Display, sync::Arc};

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use self::path::Location;

pub type SharedConfig = Arc<Config>;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default = "default_addr")]
    pub addr: String,

    #[serde(default = "default_port")]
    pub port: u16,

    #[serde(default = "default_workers")]
    pub workers: usize,

    #[serde(default = "Scheme::default")]
    pub scheme: Scheme,

    #[serde(default)]
    pub pattern: Vec<Location>,

    #[serde(default = "default_tasks_per_worker")]
    pub max_tasks_per_worker: u32,

    #[serde(default = "default_max_connections_in_waiting")]
    pub max_connections_in_waiting: usize,
}

impl Config {
    pub fn default() -> Config {
        Config {
            addr: default_addr(),
            port: default_port(),
            workers: default_workers(),
            scheme: Scheme::default(),
            pattern: Vec::new(),
            max_tasks_per_worker: default_tasks_per_worker(),
            max_connections_in_waiting: default_max_connections_in_waiting(),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    Http,
    Https,
}

impl Display for Scheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Scheme::Http => "http",
            Scheme::Https => "https",
        })
    }
}

impl Scheme {
    pub const fn default() -> Self {
        Scheme::Http
    }
}

pub const DEFAULT_ADDR: &'static str = "0.0.0.0";
pub const DEFAULT_PORT: u16 = 8000;
pub const DEFAULT_TASKS_PER_WORKER: u32 = 300_000;
pub const DEFAULT_MAX_CONNECTION_IN_WAITING: usize = 5000;

pub const fn default_port() -> u16 {
    DEFAULT_PORT
}

pub fn default_workers() -> usize {
    num_cpus::get()
}

pub fn default_addr() -> String {
    DEFAULT_ADDR.to_string()
}

pub fn default_tasks_per_worker() -> u32 {
    DEFAULT_TASKS_PER_WORKER
}

pub fn default_max_connections_in_waiting() -> usize {
    DEFAULT_MAX_CONNECTION_IN_WAITING
}
