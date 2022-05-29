#[derive(Debug)]
pub enum HttpBody {
    Stream,
    File,
    Bytes,
    Empty,
}

impl Default for HttpBody {
    fn default() -> Self {
        HttpBody::Empty
    }
}
