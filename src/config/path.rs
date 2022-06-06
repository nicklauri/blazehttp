use std::{path::PathBuf, rc::Rc, time::Duration};

use anyhow::{anyhow, Context};
use http::{header::HeaderName, HeaderMap, HeaderValue};
use regex::Regex;
use serde::{de::Error, Deserialize, Deserializer};

use crate::{error::BlazeError, util};

pub type UriPath<'a> = &'a str;

#[derive(Debug, Deserialize)]
pub struct Location {
    pattern: PathPattern,

    #[serde(deserialize_with = "Location::deserialize_headers")]
    #[serde(default)]
    headers: Vec<(HeaderName, HeaderValue)>,

    /// Serve mode.
    #[serde(default = "ServeMode::default")]
    serve: ServeMode,

    /// Keep alive mode.
    #[serde(default = "KeepAlive::default")]
    keep_alive: KeepAlive,
}

impl Location {
    pub fn deserialize_headers<'de, D>(de: D) -> Result<Vec<(HeaderName, HeaderValue)>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let headers: Vec<(&str, &str)> = Deserialize::deserialize(de)?;

        headers
            .into_iter()
            .map(util::result::and_then_tuple(
                str::parse::<HeaderName>,
                str::parse::<HeaderValue>,
            ))
            .collect::<Result<_, _>>()
            .map_err(D::Error::custom)
    }
}

#[derive(Debug, Deserialize)]
pub enum PathPattern {
    /// Match all path, this will always return true for any path.
    All,

    /// Match path if it starts with a string.
    Prefix(String),

    /// Match path if it ends with a string.
    Suffix(String),

    /// Match exact path.<br />
    /// TODO: is trailing slash ok?
    Exact(String),

    /// Consider if regex should always start at 0 or depend on user to define.<br />
    /// TODO: is it more error prone/easy to forget to put caret in the beginning of regex?
    #[serde(deserialize_with = "PathPattern::deserialize_regex")]
    Regex(Regex),
}

impl PathPattern {
    pub fn deserialize_regex<'de, D>(de: D) -> Result<Regex, D::Error>
    where
        D: Deserializer<'de>,
    {
        let regex_str: &str = Deserialize::deserialize(de)?;

        Regex::new(regex_str).map_err(D::Error::custom)
    }

    pub fn match_against(&self, path: UriPath<'_>) -> bool {
        match self {
            PathPattern::All => true,
            PathPattern::Prefix(p) => path.starts_with(p),
            PathPattern::Suffix(p) => path.ends_with(p),
            PathPattern::Exact(p) => path == p,
            PathPattern::Regex(reg) => reg.is_match(path),
            pattern => panic!("unsupported pattern: {:?}", pattern),
        }
    }
}

#[derive(Debug, Deserialize)]
pub enum ServeMode {
    /// Serve files from system path.<br />
    /// TODO: should check for forbidden path.
    Files(PathBuf),
}

impl ServeMode {
    pub fn default() -> Self {
        Self::Files("./www".into())
    }

    pub fn get_file_path(&self, path: UriPath<'_>) -> Option<PathBuf> {
        match self {
            ServeMode::Files(ref root) => {
                let mut system_path = root.clone();
                system_path.push(path);
                Some(system_path)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub enum KeepAlive {
    None,
    Duration(Duration),
    Indefinitely,
}

impl KeepAlive {
    /// Default settings, but it will be different between HTTP/0.9, HTTP/1 and HTTP/1.1
    pub const fn default() -> KeepAlive {
        const DEFAULT_KEEP_ALIVE: Duration = Duration::from_secs(20);
        KeepAlive::Duration(DEFAULT_KEEP_ALIVE)
    }
}
