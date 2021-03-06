use std::{path::PathBuf, time::Duration};

use http::{header::HeaderName, HeaderValue};
use regex::Regex;
use serde::{de::Error, Deserialize, Deserializer};

use crate::util;

pub type UriPath<'a> = &'a str;

#[derive(Debug, Deserialize, Clone)]
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
    // TODO: ban list.

    /*
    #[serde(default)]
    r#return: Option<ReturnCode>,

    ban_client_addrs: Vec<SocketAddr>,  // This override the return code ^
    ban_header_names: Vec<HeaderName>,
    ban_header_values: Vec<HeaderValue>,

     */
}

impl Location {
    pub fn deserialize_headers<'de, D>(de: D) -> Result<Vec<(HeaderName, HeaderValue)>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let headers: Vec<(&str, &str)> = Deserialize::deserialize(de)?;

        headers
            .into_iter()
            .map(util::result::and_then_tuple(str::parse::<HeaderName>, str::parse::<HeaderValue>))
            .collect::<Result<_, _>>()
            .map_err(D::Error::custom)
    }
}

#[derive(Debug, Deserialize, Clone)]
pub enum PathPattern {
    /// Match all paths, this will always return true for any path.
    All,

    /// Match path if it starts with a string.
    Prefix(String),

    /// Like `Prefix` but match path starts with any of those strings.
    PrefixAny(Vec<String>),

    /// Match path if it ends with a string.
    Suffix(String),

    /// Like `Suffix` but match path ends with any of those strings.
    SuffixAny(Vec<String>),

    /// Match exact path.<br />
    /// TODO: is trailing slash ok?
    Exact(String),

    /// Math path using regular expression.
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
            PathPattern::PrefixAny(vp) => vp.iter().any(|p| path.starts_with(p)),
            PathPattern::Suffix(p) => path.ends_with(p),
            PathPattern::SuffixAny(vp) => vp.iter().any(|p| path.ends_with(p)),
            PathPattern::Exact(p) => path == p,
            PathPattern::Regex(reg) => reg.is_match(path),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
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
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
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
