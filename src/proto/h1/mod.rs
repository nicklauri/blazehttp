use std::{mem::MaybeUninit, net::SocketAddr, rc::Rc};

use anyhow::Result;
use http::{
    header::{HeaderName, CONTENT_LENGTH, EXPECT, TRANSFER_ENCODING},
    HeaderMap, HeaderValue, Method, Uri, Version,
};
use httparse::{Header, Request as HttparseRequest, Status};
use tokio::{io::AsyncWriteExt, net::TcpStream};
use tracing::{debug, warn};

use crate::{config::Config, err, error::BlazeResult, ok, util::buf::Buf};
use crate::{
    error::{BlazeError, BlazeErrorExt},
    util,
};

use super::{body::HttpBody, Request};
const DATA: &[u8] = b"\
HTTP/1.1 200 OK
content-type: text/plain; charset=utf-8
content-length: 12

Hello World!";

/// TODO: include notify for shutting down current (worker) connection.
#[derive(Debug)]
pub struct H1Connection {
    stream: TcpStream,
    addr: SocketAddr,
    info: H1ConnInfo,
    config: Rc<Config>,
}

impl H1Connection {
    pub(super) fn new(conn: super::Connection, config: Rc<Config>) -> Self {
        let super::Connection { stream, addr } = conn;

        Self {
            stream,
            addr,
            config,
            info: H1ConnInfo::default(),
        }
    }

    pub async fn handle_connection(mut self) {
        let mut buf = Buf::with_capacity(8 * 1024);

        let error = loop {
            let request = match self.read_request(&mut buf).await {
                Ok(request) => request,
                Err(err) => {
                    break Err(err);
                }
            };

            match self.info.fill_info(&request) {
                Ok(_) => {}
                Err(err) => {
                    warn!("fill_info error");
                    break Err(err);
                }
            };

            match self.stream.write_all(DATA).await {
                Ok(_) => {}
                Err(err) => {
                    break Err(BlazeError::Other(err.into()));
                }
            };

            // TODO: await for broadcast channel to signal shutdown then shutdown this gracefully or forcefully.
            if false {
                break Ok(());
            }
        };

        if let Err(err) = error {
            if !err.is_client_close_stream() {
                // Silently close the connection without sending any response.
                warn!(?err, "handle_connection");
            } else {
                self.try_send_error_response(err).await;
            }
        }

        debug!(client.addr = ?self.addr, "close stream");

        if let Err(err) = self.stream.shutdown().await {
            debug!("stream.shutdown: error: {:?}", err);
        }

        drop(self.stream);
    }

    pub async fn read_request(&mut self, buf: &mut Buf) -> BlazeResult<Request> {
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
                        err!(BlazeError::RequestHeaderTooLarge)
                    }

                    debug!("read_request: parse header partial (already read: {} bytes)", buf.filled_size());
                    if let Err(err) = buf.fill(&mut self.stream).await {
                        if err.is_eof() {
                            debug!("read_request: eof");
                        }

                        err!(err)
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

    /// Send appropriate reponse to client based on the error.
    /// This method should handle errors even when writing out the response.
    pub async fn try_send_error_response(&mut self, _error: BlazeError) {}
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

    #[inline]
    pub fn is_empty(&self) -> bool {
        matches!(self, BodyLen::Empty)
    }
}
