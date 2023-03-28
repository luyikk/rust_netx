use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
#[cfg(all(feature = "use_openssl", not(feature = "use_rustls")))]
use tokio_openssl::SslStream;

#[cfg(all(feature = "use_rustls", not(feature = "use_openssl")))]
use tokio_rustls::client::TlsStream;

#[derive(Debug)]
pub enum MaybeStream {
    Plain(TcpStream),
    #[cfg(all(feature = "use_openssl", not(feature = "use_rustls")))]
    ServerSsl(SslStream<TcpStream>),
    #[cfg(all(feature = "use_rustls", not(feature = "use_openssl")))]
    ServerTls(TlsStream<TcpStream>),
}

impl AsyncRead for MaybeStream {
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
