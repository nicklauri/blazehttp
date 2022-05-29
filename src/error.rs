use std::error::Error as StdError;
use std::io;

use anyhow::{Context, Result};
use http::{method::InvalidMethod, uri::InvalidUri};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BlazeError {
    #[error("URI is too long")]
    UriTooLong,

    #[error("Invalid URI")]
    InvalidUri(String),

    #[error("Bad request")]
    BadRequest,

    #[error("Method \"{0}\" is not allowed")]
    MethodNotAllowed(String),

    #[error("Invalid method")]
    InvalidMethod,

    #[error("Internal error")]
    InternalError,

    #[error("Reached end of file")]
    Eof,

    #[error("Not enough space")]
    NotEnoughSpace,

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl BlazeError {
    pub fn transform_result<T, R>(this: Result<T, R>) -> BlazeResult<T>
    where
        Self: From<R>,
    {
        this.map_err(From::from)
    }

    pub fn transform_anyhow<T, R>(this: Result<T, R>) -> Result<T>
    where
        Self: From<R>,
        R: StdError + Send + Sync + 'static,
    {
        this.map_err(|err| From::from(err))
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
