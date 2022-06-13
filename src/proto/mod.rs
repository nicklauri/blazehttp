mod body;
mod h1;
mod h2;

use crate::config::Config;

use self::body::HttpBody;
use http::Request as HttpRequest;
use std::{future::Future, net::SocketAddr, rc::Rc};
use tokio::net::TcpStream;

pub type Request = HttpRequest<HttpBody>;

pub struct Connection {
    stream: TcpStream,
    addr: SocketAddr,
}

impl Connection {
    pub fn new(stream: TcpStream, addr: SocketAddr) -> Self {
        Self { stream, addr }
    }
}

pub fn handle_h1_connection(conn: Connection, config: Rc<Config>) -> impl Future<Output = ()> {
    h1::H1Connection::new(conn, config).handle_connection()
}
