use crate::config::Config;

use self::body::HttpBody;
use http::Request as HttpRequest;
use std::{net::SocketAddr, rc::Rc};
use tokio::net::TcpStream;

pub mod body;
pub mod h1;
pub mod h2;

pub type Request = HttpRequest<HttpBody>;

#[allow(dead_code)]
#[derive(Debug)]
pub enum ConnectionType {
    H1,
    H1Tls,
    H2,
}

#[derive(Debug)]
pub struct Connection {
    addr: SocketAddr,
    stream: TcpStream,
}

impl Connection {
    pub fn new(addr: SocketAddr, stream: TcpStream) -> Self {
        Self { addr, stream }
    }

    pub async fn handle(self, _config: Rc<Config>) {
        h1::handle_connection(self.stream, self.addr).await;
    }
}
