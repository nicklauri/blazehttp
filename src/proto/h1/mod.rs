use std::{mem::MaybeUninit, net::SocketAddr};

use anyhow::{bail, Result};
use http::{
    header::{HeaderName, CONTENT_LENGTH, EXPECT, TRANSFER_ENCODING},
    HeaderMap, HeaderValue, Method, Uri, Version,
};
use httparse::{Header, Request as HttparseRequest, Status};
use tokio::{io::AsyncWriteExt, net::TcpStream};
use tracing::{debug, warn};

use crate::{err, error::BlazeResult, ok, util::buf::Buf};
use crate::{
    error::{BlazeError, BlazeErrorExt},
    util,
};

use super::{body::HttpBody, Request};

pub async fn handle_connection(stream: TcpStream, addr: SocketAddr) {
    match H1Connection::new(stream, addr).handle_connection().await {
        Ok(()) => {}
        Err(_err) => {
            // error!("error: {:?}", err);
        }
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct H1Connection {
    stream: TcpStream,
    addr: SocketAddr,
    info: H1ConnInfo,
}

impl H1Connection {
    pub fn new(stream: TcpStream, addr: SocketAddr) -> Self {
        Self {
            stream,
            addr,
            info: H1ConnInfo::default(),
        }
    }

    pub async fn handle_connection(mut self) -> Result<()> {
        let mut buf = Buf::with_capacity(8 * 1024);
        const DATA: &[u8] = b"HTTP/1.1 200 OK\ncontent-length: 12\r\n\r\nHello, world";
        loop {
            // Note: should handle every errors then send appropriate error page.
            let request = match self.read_request(&mut buf).await {
                Ok(request) => request,
                Err(_err) => {
                    break;
                }
            };

            match self.info.fill_info(&request) {
                Ok(_) => {}
                Err(_error) => {
                    warn!("fill_info error");
                    break;
                }
            };

            match self.stream.write_all(DATA).await {
                Ok(_m) => {}
                Err(_error) => {
                    break;
                }
            };
        }

        // TODO: handle error.
        Ok(self.stream.shutdown().await?)
    }

    pub async fn read_request(&mut self, buf: &mut Buf) -> Result<Request> {
        if buf.is_empty() {
            buf.fill(&mut self.stream).await?;
        }

        loop {
            let reqbuf = buf.filled();
            let mut header: [MaybeUninit<Header>; 20] = unsafe { MaybeUninit::uninit().assume_init() };
            let mut hreq = HttparseRequest::new(&mut []);
            let parsed_size = match hreq.parse_with_uninit_headers(reqbuf, &mut header) {
                Ok(Status::Complete(size)) => size,
                Ok(Status::Partial) => {
                    if buf.is_full() {
                        // The buff is fulled but header is still incomplete.
                        debug!("read_request: header is too large");
                        bail!(BlazeError::RequestHeaderTooLarge)
                    }

                    debug!("read_request: parse header partial (already read: {} bytes)", buf.filled_size());
                    if let Err(err) = buf.fill(&mut self.stream).await {
                        if err.is_eof() {
                            debug!("read_request: eof");
                        }

                        bail!(err)
                    }

                    continue;
                }
                Err(err) => Err(err).blaze_error()?,
            };

            let request = map_to_http_request(&hreq)?;

            buf.advance(parsed_size);

            // *request.body_mut() = HttpBody::Empty;

            return Ok(request);
        }
    }
}

fn map_to_http_request(hreq: &HttparseRequest) -> Result<Request> {
    let mut req = Request::new(HttpBody::default());

    get_method(&hreq, &mut req)?;
    get_path(&hreq, &mut req)?;
    get_version(&hreq, &mut req)?;
    get_headers(&hreq, &mut req)?;

    Ok(req)
}

#[inline]
fn get_method(hreq: &HttparseRequest, req: &mut Request) -> Result<()> {
    *req.method_mut() = hreq
        .method
        .map(str::as_bytes)
        .ok_or(BlazeError::BadRequest)
        .and_then(util::transform_error(Method::from_bytes))?;
    Ok(())
}

#[inline]
fn get_path(hreq: &HttparseRequest, req: &mut Request) -> Result<()> {
    *req.uri_mut() = hreq
        .path
        .ok_or(BlazeError::BadRequest)
        .and_then(util::transform_error(str::parse::<Uri>))?;
    Ok(())
}

#[inline]
fn get_version(hreq: &HttparseRequest, req: &mut Request) -> Result<()> {
    *req.version_mut() = hreq.version.ok_or(BlazeError::InvalidVersion).and_then(|v| match v {
        0 => Ok(Version::HTTP_10),
        1 => Ok(Version::HTTP_11),
        _ => Err(BlazeError::InvalidVersion), // Note: httparse doesn't parse 0.9 or 2.0, 3.0
    })?;
    Ok(())
}

#[inline]
fn get_headers(hreq: &HttparseRequest, req: &mut Request) -> Result<()> {
    let header_iter = hreq.headers.iter().map(|h| (h.name, h.value));
    let header_count = hreq.headers.len();
    let mut header_maps = HeaderMap::with_capacity(header_count);

    for (name, value) in header_iter {
        let header_name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| BlazeError::InvalidHeaderName(name.to_string()))?;
        let header_value = HeaderValue::from_bytes(value).or(Err(BlazeError::InvalidHeaderValue))?;

        header_maps.append(header_name, header_value);
    }

    *req.headers_mut() = header_maps;

    Ok(())
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct H1ConnInfo {
    keep_alive: bool,
    expect_continue: bool,
    content_length: BodyLen,
}

impl H1ConnInfo {
    pub fn default() -> H1ConnInfo {
        H1ConnInfo {
            keep_alive: false,
            expect_continue: false,
            content_length: BodyLen::Empty,
        }
    }

    pub fn fill_info(&mut self, req: &Request) -> BlazeResult<()> {
        self.check_expect(&req)?;
        self.get_content_length(&req)?;

        Ok(())
    }

    fn check_expect(&mut self, req: &Request) -> BlazeResult<()> {
        let expect_value = match req.headers().get(EXPECT) {
            Some(val) => val,
            None => ok!(self.expect_continue = false), // This is just setting self.expect_continue to false and return Ok(())
        };

        self.expect_continue = expect_value.as_bytes().eq_ignore_ascii_case(b"100-continue");

        // At this point, self.expect_continue must be true.
        if !self.expect_continue {
            err!(BlazeError::BadRequest)
        }

        Ok(())
    }

    fn get_content_length(&mut self, req: &Request) -> BlazeResult<()> {
        #[allow(dead_code)]
        const NOT_ALLOWED_BODY_METHODS: &'static [Method] = &[Method::GET, Method::HEAD, Method::OPTIONS, Method::DELETE];

        self.content_length = BodyLen::Empty;

        let headers = req.headers();
        // Check transfer-encoding as well.
        if let Some(te) = headers.get(TRANSFER_ENCODING) {
            let is_chunked = te.as_bytes().eq_ignore_ascii_case(b"chunked");
            if is_chunked {
                self.content_length = BodyLen::Chunked;
            } else {
            }
        }

        if let Some(lenstr) = headers.get(CONTENT_LENGTH) {
            if self.content_length.is_chunked() {
                // Do something here, because of the conflict.
            }

            let len = lenstr
                .to_str()
                .blaze_error()
                .and_then(util::transform_error(str::parse::<u64>))?;

            self.content_length = BodyLen::Length(len);
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum BodyLen {
    Empty,
    Chunked,
    Length(u64),
}

impl BodyLen {
    #[inline]
    pub fn is_chunked(&self) -> bool {
        matches!(self, BodyLen::Chunked)
    }

    #[allow(dead_code)]
    #[inline]
    pub fn is_empty(&self) -> bool {
        matches!(self, BodyLen::Empty)
    }
}
