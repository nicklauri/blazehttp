use std::io;
use std::{error::Error as StdError, num::ParseIntError};

use anyhow::{Context, Result};
use http::{
    header::{InvalidHeaderName, InvalidHeaderValue, ToStrError as HeaderValueToStrError},
    method::InvalidMethod,
    uri::InvalidUri,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BlazeError {
    /// Return: 414 URI Too Long
    #[error("URI is too long")]
    UriTooLong,

    /// Return: 400 Bad Request
    #[error("Invalid URI")]
    InvalidUri(String),

    /// Return: 400 Bad Request
    #[error("Bad request")]
    BadRequest,

    /// Return: 405 Method Not Allowed
    #[error("Method \"{0}\" is not allowed")]
    MethodNotAllowed(String),

    #[error("Invalid method")]
    InvalidMethod,

    /// Return: 400 Bad Request
    #[error("Invalid HTTP version")]
    InvalidVersion,

    /// Return: 400 Bad Request
    #[error("Invalid header name: {0}")]
    InvalidHeaderName(String),

    /// Return: 400 Bad Request
    #[error("Invalid header name")]
    InvalidHeaderNameEmpty,

    /// Return: 400 Bad Request
    #[error("Invalid header value")]
    InvalidHeaderValue,

    /// Return: 400 Bad Request
    #[error("Invalid expect value")]
    InvalidExpectValue,

    /// Return: 500 Internal Server Error
    #[error("Internal error")]
    InternalError,

    /// Silent and close connection
    #[error("Reached end of file")]
    Eof,

    /// Return: 413 Payload Too Large
    #[error("Request header too large")]
    RequestHeaderTooLarge,

    /// Return: 413 Payload Too Large
    #[error("Payload too large")]
    PayloadTooLarge,

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl BlazeError {
    #[inline]
    pub fn transform_result<T, R>(this: Result<T, R>) -> BlazeResult<T>
    where
        Self: From<R>,
    {
        this.map_err(From::from)
    }

    #[inline]
    pub fn transform_anyhow<T, R>(this: Result<T, R>) -> Result<T>
    where
        Self: From<R>,
        R: StdError + Send + Sync + 'static,
    {
        this.map_err(From::from)
    }
}

pub type BlazeResult<T> = Result<T, BlazeError>;

impl From<io::Error> for BlazeError {
    #[inline]
    fn from(err: io::Error) -> Self {
        // TODO: check and conver error from IO
        BlazeError::Other(err.into())
    }
}

impl From<httparse::Error> for BlazeError {
    #[inline]
    fn from(err: httparse::Error) -> Self {
        BlazeError::Other(err.into())
    }
}

impl From<InvalidMethod> for BlazeError {
    #[inline]
    fn from(_: InvalidMethod) -> Self {
        BlazeError::InvalidMethod
    }
}

impl From<InvalidUri> for BlazeError {
    #[inline]
    fn from(err: InvalidUri) -> Self {
        BlazeError::InvalidUri(err.to_string())
    }
}

impl From<InvalidHeaderName> for BlazeError {
    #[inline]
    fn from(_: InvalidHeaderName) -> Self {
        BlazeError::InvalidHeaderNameEmpty
    }
}

impl From<InvalidHeaderValue> for BlazeError {
    #[inline]
    fn from(_: InvalidHeaderValue) -> Self {
        BlazeError::InvalidHeaderValue
    }
}

impl From<HeaderValueToStrError> for BlazeError {
    #[inline]
    fn from(_: HeaderValueToStrError) -> Self {
        BlazeError::InvalidHeaderValue
    }
}

impl From<ParseIntError> for BlazeError {
    #[inline]
    fn from(_: ParseIntError) -> Self {
        BlazeError::BadRequest
    }
}

pub trait BlazeErrorExt<T> {
    fn blaze_error(self) -> BlazeResult<T>;
}

impl<T, E> BlazeErrorExt<T> for Result<T, E>
where
    E: Into<BlazeError>,
{
    fn blaze_error(mut self) -> BlazeResult<T> {
        self.map_err(Into::into)
    }
}
