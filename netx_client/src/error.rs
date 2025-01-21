use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Error(#[from] anyhow::Error),
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[cfg(all(feature = "tcpclient", not(feature = "tcp-channel-client")))]
    #[error(transparent)]
    NetError(#[from] tcpclient::error::Error),
    #[cfg(all(feature = "tcp-channel-client", not(feature = "tcpclient")))]
    #[error(transparent)]
    NetError(#[from] tcp_channel_client::error::Error),
    #[error(transparent)]
    WatchRecvError(#[from] tokio::sync::watch::error::RecvError),
    #[error("ConnectError:{0}")]
    ConnectError(String),
    #[error("Serial:{0} is close")]
    SerialClose(i64),
    #[error("Serial:{0} timeout")]
    SerialTimeOut(i64),
    #[error("Call Error:{{ id:{0},msg:\"{1}\"}}")]
    CallError(i32, String),
    #[cfg(feature = "use_openssl")]
    #[error(transparent)]
    OpenSslError(#[from] openssl::error::ErrorStack),
}

pub type Result<T, E = Error> = core::result::Result<T, E>;
