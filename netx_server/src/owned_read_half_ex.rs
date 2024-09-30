use data_rw::DataOwnedReader;
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, ReadHalf};

/// A trait that extends the functionality of `ReadHalf`.
pub(crate) trait ReadHalfExt {
    /// Reads a string from the `ReadHalf`.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if reading from the `ReadHalf` fails.
    async fn read_string(&mut self) -> io::Result<String>;

    /// Reads a `DataOwnedReader` from the `ReadHalf`.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if reading from the `ReadHalf` fails.
    async fn read_buff(&mut self) -> io::Result<DataOwnedReader>;
}

impl<C> ReadHalfExt for &mut ReadHalf<C>
where
    C: AsyncRead + AsyncWrite + Send + 'static,
{
    /// Reads a string from the `ReadHalf`.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if reading from the `ReadHalf` fails.
    #[inline]
    async fn read_string(&mut self) -> io::Result<String> {
        let len = self.read_u32_le().await? as usize;
        let mut data = vec![0; len];
        let r = self.read_exact(&mut data).await?;
        debug_assert_eq!(len, r);
        Ok(String::from_utf8_lossy(&data).to_string())
    }

    /// Reads a `DataOwnedReader` from the `ReadHalf`.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if reading from the `ReadHalf` fails.
    #[inline]
    async fn read_buff(&mut self) -> io::Result<DataOwnedReader> {
        let len = (self.read_u32_le().await? - 4) as usize;
        let mut data = vec![0; len];
        let r = self.read_exact(&mut data).await?;
        debug_assert_eq!(len, r);
        Ok(DataOwnedReader::new(data))
    }
}
