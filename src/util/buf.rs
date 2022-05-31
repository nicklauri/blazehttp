use anyhow::{anyhow, bail, Result};
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::error::{BlazeError, BlazeErrorExt, BlazeResult};

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
    #[inline]
    pub fn from_vec(mut vec: Vec<u8>) -> Self {
        if vec.len() < vec.capacity() {
            unsafe {
                vec.set_len(vec.capacity());
            }
        }

        let data = vec.into_boxed_slice();

        Self {
            pos: data.len(),
            data,
        }
    }

    #[inline]
    pub fn reset(&mut self) {
        self.pos = 0;
    }

    /// Fill buffer with `reader: R`. If the buffer is full, error `BlazeError::RequestHeaderTooLarge` will be returned.
    #[inline]
    pub async fn fill<R>(&mut self, reader: &mut R) -> Result<()>
    where
        R: AsyncReadExt + Unpin,
    {
        if self.is_full() {
            bail!(BlazeError::RequestHeaderTooLarge);
        }

        let writable_buf = &mut self.data[self.pos..];

        let amount = reader.read(writable_buf).await.blaze_error()?;

        if amount == 0 && self.pos == 0 {
            bail!(BlazeError::Eof);
        }

        self.pos += amount;

        Ok(())
    }

    #[inline]
    pub fn get_buf(&self) -> &[u8] {
        &self.data[..self.pos]
    }

    #[inline]
    pub fn clear(&mut self) {
        self.pos = 0;
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.pos == 0
    }

    #[inline]
    pub fn pos(&self) -> usize {
        self.pos
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.pos >= self.data.len()
    }

    #[inline]
    pub fn buf_capacity(&self) -> usize {
        self.data.len()
    }

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
    #[inline]
    pub fn to_vec(mut self) -> Vec<u8> {
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
    // case: empty buf
    // case: full buf error
    // case: advance
    // case: to vec
}
