use std::{mem::MaybeUninit, net::SocketAddr, ops::ControlFlow};

use anyhow::{anyhow, bail, Context, Result};
use http::{
    header::HeaderName, method::InvalidMethod, HeaderMap, HeaderValue, Method,
    Request as HttpRequest, Uri, Version,
};
use httparse::{Header, Request as HttparseRequest, Status};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

use crate::util::buf::Buf;
use crate::{
    error::{BlazeError, BlazeErrorExt},
    util,
};

use super::{body::HttpBody, Request};

pub async fn handle_connection(stream: TcpStream, addr: SocketAddr) {
    match H1Connection::new(stream, addr).handle_connection().await {
        Ok(()) => {}
        Err(err) => {
            println!("error: {:?}", err);
        }
    }
}

#[derive(Debug)]
pub struct H1Connection {
    stream: TcpStream,
    addr: SocketAddr,
}

impl H1Connection {
    pub fn new(stream: TcpStream, addr: SocketAddr) -> Self {
        Self { stream, addr }
    }

    pub async fn handle_connection(mut self) -> Result<()> {
        let content = "Hello world!";
        let len = content.len();
        let response_text = format!(
            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n{}",
            len, content
        );

        // If buffer is configurable in the future, these will be changed into Vec<u8>
        let buf = &mut [0u8; 8 * 1024];
        let mut buf = Buf::from_slice(buf);

        println!("Hello");
        loop {
            let res = buf.fill(&mut self.stream).await;

            if buf.is_empty() {
                break;
            }

            // TODO: move these to util.
            let reqbuf = buf.get_buf();
            let mut header: [MaybeUninit<Header>; 20] =
                unsafe { MaybeUninit::uninit().assume_init() };
            let mut hreq = HttparseRequest::new(&mut []);
            let parsed_size = match hreq.parse_with_uninit_headers(reqbuf, &mut header) {
                Ok(Status::Complete(size)) => size,
                Ok(Status::Partial) => {
                    if buf.is_full() {
                        // The buff is fulled but header is still incomplete.
                        // Not supported!
                        bail!(BlazeError::NotEnoughSpace)
                    }
                    continue
                },
                Err(err) => Err(err).blaze_error()?,
            };

            // Debug write, eventually write HttpBody to stream.
            self.stream.write_all(response_text.as_bytes()).await?;

            let mut request = map_to_http_request(&hreq)?;

            let body = buf.advance(parsed_size).get_buf().to_vec();

            *request.body_mut() = HttpBody::Bytes(body);
        }

        Ok(())
    }
}

fn map_to_http_request(hreq: &HttparseRequest) -> Result<Request> {
    let mut req = Request::new(HttpBody::default());

    get_method(&hreq, &mut req);
    get_path(&hreq, &mut req);
    get_version(&hreq, &mut req);
    get_headers(&hreq, &mut req);

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
    *req.version_mut() = hreq
        .version
        .ok_or(BlazeError::InvalidVersion)
        .and_then(|v| match v {
            0 => Ok(Version::HTTP_10),
            1 => Ok(Version::HTTP_11),
            _ => Err(BlazeError::InvalidVersion)    // Note: httparse doesn't parse 0.9 or 2.0, 3.0
        })?;
    Ok(())
}

#[inline]
fn get_headers(hreq: &HttparseRequest, req: &mut Request) -> Result<()> {
    let header_iter = hreq.headers.iter().map(|h| (h.name, h.value));
    let header_map_cap = hreq.headers.len();
    let mut header_maps = HeaderMap::with_capacity(header_map_cap);

    for (name, value) in header_iter {
        let header_name = HeaderName::from_bytes(name.as_bytes())?;
        let header_value = HeaderValue::from_bytes(value)?;

        header_maps.append(header_name, header_value);
    }

    *req.headers_mut() = header_maps;

    Ok(())
}
