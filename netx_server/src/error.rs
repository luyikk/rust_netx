use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Error(#[from] anyhow::Error),
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[cfg(all(feature = "tcpserver", not(feature = "tcp-channel-server")))]
    #[error(transparent)]
    NetError(#[from] tcpserver::error::Error),
    #[cfg(feature = "tcp-channel-server")]
    #[error(transparent)]
    NetChannelError(#[from] tcp_channel_server::error::Error),
    #[error("Serial:{0} is close")]
    SerialClose(i64),
    #[error("Serial:{0} timeout")]
    SerialTimeOut(i64),
    #[error("manager upgrade fail")]
    ManagerUpgradeFail,
    #[error("serial id is have")]
    SerialHave,
    #[error("token:{0} disconnect")]
    TokenDisconnect(i64),
    #[error("Call Error:{{ id:{0},msg:\"{1}\"}}")]
    CallError(i32, String),
}

pub type Result<T, E = Error> = core::result::Result<T, E>;
