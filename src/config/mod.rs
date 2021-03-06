pub mod path;

use std::{fmt::Display, time::Duration};

use serde::{Deserialize, Deserializer};

use self::path::Location;

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

    #[serde(default)]
    pub display_statistics_on_shutdown: bool,

    #[serde(default = "default_accept_error_sleep", deserialize_with = "Config::deserialize_duration")]
    pub accept_error_sleep: Duration,
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
            display_statistics_on_shutdown: Default::default(),
            accept_error_sleep: default_accept_error_sleep(),
        }
    }

    pub fn deserialize_duration<'de, D>(de: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let ms: u64 = Deserialize::deserialize(de)?;

        Ok(Duration::from_millis(ms))
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
pub const DEFAULT_TASKS_PER_WORKER: u32 = 100;
pub const DEFAULT_MAX_CONNECTION_IN_WAITING: usize = 1;
pub const DEFAULT_ACCEPT_ERROR_SLEEP: Duration = Duration::from_millis(500);

pub const fn default_port() -> u16 {
    DEFAULT_PORT
}

pub fn default_workers() -> usize {
    num_cpus::get()
}

pub fn default_addr() -> String {
    DEFAULT_ADDR.to_string()
}

pub const fn default_tasks_per_worker() -> u32 {
    // TODO: calculate based on ulimit.
    DEFAULT_TASKS_PER_WORKER
}

pub const fn default_max_connections_in_waiting() -> usize {
    // TODO: calculate based on ulimit.
    DEFAULT_MAX_CONNECTION_IN_WAITING
}

pub const fn default_accept_error_sleep() -> Duration {
    DEFAULT_ACCEPT_ERROR_SLEEP
}
