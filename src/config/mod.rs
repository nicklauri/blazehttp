pub mod pattern;

use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

pub type GlobalConfig = Arc<RwLock<Config>>;

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default = "default_addr")]
    pub addr: String,

    #[serde(default = "default_port")]
    pub port: u16,

    #[serde(default = "default_workers")]
    pub workers: usize,

    #[serde(default = "Scheme::default")]
    pub scheme: Scheme,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            ..Default::default()
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    Http,
    Https,
}

impl Scheme {
    pub fn default() -> Self {
        Scheme::Http
    }
}

pub const DEFAULT_ADDR: &'static str = "0.0.0.0";
pub const DEFAULT_PORT: u16 = 80;

pub fn default_port() -> u16 {
    DEFAULT_PORT
}

pub fn default_workers() -> usize {
    num_cpus::get()
}

pub fn default_addr() -> String {
    DEFAULT_ADDR.to_string()
}
