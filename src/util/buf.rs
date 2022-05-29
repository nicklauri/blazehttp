use anyhow::{anyhow, bail, Result};
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::error::{BlazeError, BlazeErrorExt, BlazeResult};

/// Convenient wrapper type for buffer slice.
#[derive(Debug)]
pub struct Buf<'a> {
    data: &'a mut [u8],
    data_len: usize,
}

impl<'a> Buf<'a> {
    #[inline]
    pub fn from_slice(data: &'a mut [u8]) -> Self {
        Self { data, data_len: 0 }
    }

    #[inline]
    pub async fn fill<R>(&mut self, reader: &mut R) -> Result<()>
    where
        R: AsyncReadExt + Unpin,
    {
        if self.data_len >= self.data.len() {
            bail!(BlazeError::NotEnoughSpace);
        }

        let writable_buf = &mut self.data[self.data_len..];

        let amount = reader.read(writable_buf).await.blaze_error()?;

        if amount == 0 && self.data_len == 0 {
            bail!(BlazeError::Eof);
        }

        self.data_len += amount;

        Ok(())
    }

    #[inline]
    pub fn get_buf(&self) -> &[u8] {
        &self.data[..self.data_len]
    }

    #[inline]
    pub fn clear(&mut self) {
        self.data_len = 0;
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.data_len == 0
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.data_len == self.data.len()
    }

    #[inline]
    pub fn buf_size(&self) -> usize {
        self.data.len()
    }

    #[inline]
    pub fn filled_size(&self) -> usize {
        self.data_len
    }

    #[inline]
    pub fn advance(&mut self, offset: usize) {
        let new_len = self.data_len.saturating_sub(offset);

        if offset < self.data_len {
            self.data.copy_within(offset..self.data_len, 0);
        }

        self.data_len = new_len;
    }
}
