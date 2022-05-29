use std::sync::Arc;

use parking_lot::RwLock;

pub type GlobalConfig = Arc<RwLock<Config>>;

#[derive(Debug)]
pub struct Config {
    pub addr: Option<String>,
    pub port: u16,
    pub scheme: Scheme,
    pub workers: u16,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            addr: None,
            port: 80,
            scheme: Scheme::Http,
            workers: 8,
        }
    }
}

#[derive(Debug)]
pub enum Scheme {
    Http,
    Https,
}
