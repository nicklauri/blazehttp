use tokio::io::AsyncReadExt;

use crate::{
    err,
    error::{BlazeError, BlazeErrorExt, BlazeResult},
};

/// Convenient wrapper type for buffer slice.
#[derive(Debug)]
pub struct Buf {
    data: Box<[u8]>,
    pos: usize,
}

impl Buf {
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        let mut data = Vec::with_capacity(capacity);

        // SAFETY:
        //  this is safe because `data` is always been filled before read (`fill` method)
        //  and tracked by `pos`. `get_buf` method will also get range `..self.pos` which is
        //  also a valid range.
        unsafe {
            data.set_len(capacity);
        }

        let data = data.into_boxed_slice();

        Self { data, pos: 0 }
    }

    /// Construct from a vector.
    #[allow(dead_code)]
    #[inline]
    pub fn from_vec(mut vec: Vec<u8>) -> Self {
        if vec.len() < vec.capacity() {
            unsafe {
                vec.set_len(vec.capacity());
            }
        }

        let data = vec.into_boxed_slice();

        Self { pos: data.len(), data }
    }

    #[inline]
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.pos = 0;
    }

    /// Fill buffer with `reader: R`. If the buffer is full, error `BlazeError::RequestHeaderTooLarge` will be returned.
    #[inline]
    pub async fn fill<R>(&mut self, reader: &mut R) -> BlazeResult<()>
    where
        R: AsyncReadExt + Unpin,
    {
        if self.is_full() {
            err!(BlazeError::RequestHeaderTooLarge);
        }

        let writable_buf = &mut self.data[self.pos..];

        let amount = reader.read(writable_buf).await.blaze_error()?;

        err!(amount == 0, BlazeError::Eof);

        self.pos += amount;

        Ok(())
    }

    #[inline]
    pub fn filled(&self) -> &[u8] {
        &self.data[..self.pos]
    }

    #[allow(dead_code)]
    #[inline]
    pub fn clear(&mut self) {
        self.pos = 0;
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.pos == 0
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.pos >= self.data.len()
    }

    #[allow(dead_code)]
    #[inline]
    pub fn buf_capacity(&self) -> usize {
        self.data.len()
    }

    #[allow(dead_code)]
    #[inline]
    pub fn filled_size(&self) -> usize {
        self.pos
    }

    /// Truncate buffer from the left to the offset.<br/>
    /// If `offset` is greater than the amount of data, clear all data.
    #[inline]
    pub fn advance(&mut self, offset: usize) -> &Self {
        let new_len = self.pos.saturating_sub(offset);

        if offset < self.pos {
            self.data.copy_within(offset..self.pos, 0);
        }

        self.pos = new_len;

        self
    }

    /// Consume self and return unlying vector.
    #[allow(dead_code)]
    #[inline]
    pub fn to_vec(self) -> Vec<u8> {
        let vec_len = if self.pos <= self.data.len() { self.pos } else { 0 };
        let mut vec = self.data.to_vec();

        unsafe {
            vec.set_len(vec_len);
        }

        vec
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use tokio::time::{self};
    use tokio_test::io::Builder;

    use crate::error::BlazeError;

    use super::Buf;

    #[tokio::test]
    async fn fill_buf_twice() {
        let mut mock = Builder::new().read(b"hello").read(b"world").build();

        const MAX_CAP: usize = 10;
        let mut buf = Buf::with_capacity(MAX_CAP);

        assert!(buf.is_empty());
        assert_eq!(buf.buf_capacity(), MAX_CAP);

        let res = buf.fill(&mut mock).await;

        assert!(res.is_ok());

        assert_eq!(buf.filled(), &b"hello"[..]);

        let res = buf.fill(&mut mock).await;

        assert!(res.is_ok());

        assert_eq!(buf.filled(), &b"helloworld"[..]);
    }

    #[tokio::test]
    async fn fill_to_eof() {
        let mut mock = Builder::new()
            .read(b"hello")
            .read(b"world")
            .read(b"another")
            .read(b"test")
            .build();

        const MAX_CAP: usize = 1024;
        let mut buf = Buf::with_capacity(MAX_CAP);

        assert!(buf.is_empty());
        assert_eq!(buf.buf_capacity(), MAX_CAP);

        loop {
            let fut = buf.fill(&mut mock);
            let timeout_res = time::timeout(Duration::from_secs(1), fut).await;

            assert!(timeout_res.is_ok(), "read operation took longer time than expected");

            let res = timeout_res.unwrap();

            match res {
                Ok(()) => {}
                Err(BlazeError::Eof) => break,
                Err(err) => assert!(false, "read operations expect only success, error: {err:?}"),
            }
        }

        assert_eq!(buf.filled(), &b"helloworldanothertest"[..]);
    }

    #[tokio::test]
    async fn advance_buf() {
        // let mock = Builder::new().read(b"hello world").read(b"blaze http").build();

        // const MAX_CAP: usize = 8096;
        // let buf = Buf::with_capacity(MAX_CAP);
    }

    #[tokio::test]
    async fn fill_full_buf() {}
}
