#[allow(dead_code)]
#[derive(Debug)]
pub enum HttpBody {
    Stream,
    File,
    Bytes(Vec<u8>),
    Chunked,
    Empty,
}

impl Default for HttpBody {
    fn default() -> Self {
        HttpBody::Empty
    }
}
