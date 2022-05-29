use self::body::HttpBody;
use anyhow::Result;
use http::Request as HttpRequest;
use std::net::SocketAddr;
use tokio::net::TcpStream;

pub mod body;
pub mod h1;
pub mod h2;

pub type Request = HttpRequest<HttpBody>;

#[derive(Debug)]
pub enum ConnectionType {
    H1,
    H1Tls,
    H2,
}

#[derive(Debug)]
pub struct Connection {
    ty: ConnectionType,
    addr: SocketAddr,
    stream: TcpStream,
}

impl Connection {
    pub fn new_h1(addr: SocketAddr, stream: TcpStream) -> Self {
        Self {
            ty: ConnectionType::H1,
            addr,
            stream,
        }
    }

    pub async fn handle(mut self) -> Result<()> {
        match self.ty {
            ConnectionType::H1 => h1::handle_connection(self.stream, self.addr).await,
            ty => panic!("Unsupported connection type: {ty:?}"),
        }

        Ok(())
    }
}
