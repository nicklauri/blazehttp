use http::{header::HeaderName, HeaderMap, HeaderValue};
use regex::Regex;
use serde::Deserialize;

#[derive(Debug)]
pub struct Location {
    pattern: PathPattern,                    // required min=max=1
    headers: Vec<(HeaderName, HeaderValue)>, // optional
}

#[derive(Debug, Deserialize)]
pub enum PathPattern {
    // Regex(Regex), // write a deserializer for this.
    Exact(String),
    Prefix(String),
    Suffix(String),
}
