use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
#[cfg(all(feature = "use_openssl", not(feature = "use_rustls")))]
use tokio_openssl::SslStream;

#[cfg(all(feature = "use_rustls", not(feature = "use_openssl")))]
use tokio_rustls::client::TlsStream;

/// `MaybeStream` is an enum that represents a stream which can be either a plain `TcpStream`
/// or a TLS/SSL encrypted stream using either OpenSSL or Rustls.
#[derive(Debug)]
pub enum MaybeStream {
    /// A plain TCP stream.
    Plain(TcpStream),
    /// An SSL encrypted stream using OpenSSL.
    #[cfg(all(feature = "use_openssl", not(feature = "use_rustls")))]
    ServerSsl(SslStream<TcpStream>),
    /// A TLS encrypted stream using Rustls.
    #[cfg(all(feature = "use_rustls", not(feature = "use_openssl")))]
    ServerTls(TlsStream<TcpStream>),
}

impl AsyncRead for MaybeStream {
    /// Polls for reading from the stream.
    ///
    /// # Parameters
    /// - `self`: A pinned mutable reference to the `MaybeStream`.
    /// - `cx`: The context of the current task.
    /// - `buf`: The buffer to read data into.
    ///
    /// # Returns
    /// A `Poll` that resolves to a `Result` indicating the success or failure of the read operation.
    #[inline]
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MaybeStream::Plain(ref mut s) => Pin::new(s).poll_read(cx, buf),
            #[cfg(all(feature = "use_openssl", not(feature = "use_rustls")))]
            MaybeStream::ServerSsl(ref mut s) => Pin::new(s).poll_read(cx, buf),
            #[cfg(all(feature = "use_rustls", not(feature = "use_openssl")))]
            MaybeStream::ServerTls(ref mut s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for MaybeStream {
    /// Polls for writing to the stream.
    ///
    /// # Parameters
    /// - `self`: A pinned mutable reference to the `MaybeStream`.
    /// - `cx`: The context of the current task.
    /// - `buf`: The buffer containing data to write.
    ///
    /// # Returns
    /// A `Poll` that resolves to a `Result` indicating the number of bytes written or an error.
    #[inline]
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        match self.get_mut() {
            MaybeStream::Plain(ref mut s) => Pin::new(s).poll_write(cx, buf),
            #[cfg(all(feature = "use_openssl", not(feature = "use_rustls")))]
            MaybeStream::ServerSsl(ref mut s) => Pin::new(s).poll_write(cx, buf),
            #[cfg(all(feature = "use_rustls", not(feature = "use_openssl")))]
            MaybeStream::ServerTls(ref mut s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    /// Polls for flushing the stream.
    ///
    /// # Parameters
    /// - `self`: A pinned mutable reference to the `MaybeStream`.
    /// - `cx`: The context of the current task.
    ///
    /// # Returns
    /// A `Poll` that resolves to a `Result` indicating the success or failure of the flush operation.
    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        match self.get_mut() {
            MaybeStream::Plain(ref mut s) => Pin::new(s).poll_flush(cx),
            #[cfg(all(feature = "use_openssl", not(feature = "use_rustls")))]
            MaybeStream::ServerSsl(ref mut s) => Pin::new(s).poll_flush(cx),
            #[cfg(all(feature = "use_rustls", not(feature = "use_openssl")))]
            MaybeStream::ServerTls(ref mut s) => Pin::new(s).poll_flush(cx),
        }
    }

    /// Polls for shutting down the stream.
    ///
    /// # Parameters
    /// - `self`: A pinned mutable reference to the `MaybeStream`.
    /// - `cx`: The context of the current task.
    ///
    /// # Returns
    /// A `Poll` that resolves to a `Result` indicating the success or failure of the shutdown operation.
    #[inline]
    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        match self.get_mut() {
            MaybeStream::Plain(ref mut s) => Pin::new(s).poll_shutdown(cx),
            #[cfg(all(feature = "use_openssl", not(feature = "use_rustls")))]
            MaybeStream::ServerSsl(ref mut s) => Pin::new(s).poll_shutdown(cx),
            #[cfg(all(feature = "use_rustls", not(feature = "use_openssl")))]
            MaybeStream::ServerTls(ref mut s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}
