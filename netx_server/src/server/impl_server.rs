use anyhow::{bail, Result};
use aqueue::Actor;
use bytes::BufMut;
use data_rw::Data;
use std::sync::{Arc, Weak};
use tcpserver::{Builder, IPeer, ITCPServer, TCPPeer};
use tokio::io::{AsyncReadExt, ReadHalf};
use tokio::task::JoinHandle;

use crate::async_token::{IAsyncToken, IAsyncTokenInner, NetxToken};
use crate::async_token_manager::{IAsyncTokenManager, TokenManager};
use crate::controller::ICreateController;
use crate::owned_read_half_ex::ReadHalfExt;
use crate::server::async_token_manager::{
    AsyncTokenManager, IAsyncTokenManagerCreateToken, ITokenManager,
};
use crate::server::maybe_stream::MaybeStream;
use crate::{RetResult, ServerOption};

cfg_if::cfg_if! {
if #[cfg(feature = "use_openssl")]{
   use openssl::ssl::{Ssl,SslAcceptor};
   use tokio_openssl::SslStream;
   use std::time::Duration;
   use tokio::time::sleep;
   use std::pin::Pin;
}else if #[cfg(feature = "use_rustls")]{
   use tokio_rustls::TlsAcceptor;
}}

pub type NetPeer = Actor<TCPPeer<MaybeStream>>;
pub type NetReadHalf = ReadHalf<MaybeStream>;

pub(crate) enum SpecialFunctionTag {
    Connect = 2147483647,
    Disconnect = 2147483646,
    Closed = 2147483645,
}

struct NetXServerInner<T: ICreateController + 'static> {
    option: ServerOption,
    async_tokens: TokenManager<T>,
}

/// Netx Service
pub struct NetXServer<T: ICreateController + 'static> {
    inner: Arc<NetXServerInner<T>>,
    serv: Arc<dyn ITCPServer<Arc<NetXServerInner<T>>>>,
}
unsafe impl<T: ICreateController + 'static> Send for NetXServer<T> {}
unsafe impl<T: ICreateController + 'static> Sync for NetXServer<T> {}

impl<T> NetXServer<T>
where
    T: ICreateController + 'static,
{
    cfg_if::cfg_if! {

        if #[cfg(feature = "use_openssl")]{
        /// new openssl tls encryption service
        #[inline]
        pub async fn new_ssl(
            ssl_acceptor: &'static SslAcceptor,
            option: ServerOption,
            impl_controller: T,
        ) -> NetXServer<T> {
            let request_out_time = option.request_out_time;
            let session_save_time = option.session_save_time;
            let async_tokens =
                AsyncTokenManager::new(impl_controller, request_out_time, session_save_time);
            let inner = Arc::new(NetXServerInner {
                option,
                async_tokens,
            });
            let serv = Builder::new(&inner.option.addr)
                .set_connect_event(|addr| {
                    log::debug!("{} connect", addr);
                    true
                })
                .set_stream_init(move |tcp_stream| async move {
                    let ssl = Ssl::new(ssl_acceptor.context())?;
                    let mut stream = SslStream::new(ssl, tcp_stream)?;
                    sleep(Duration::from_millis(200)).await;
                    Pin::new(&mut stream).accept().await?;
                    Ok(MaybeStream::ServerSsl(stream))
                })
                .set_input_event(|mut reader, peer, inner| async move {
                    let addr = peer.addr();
                    let token = match Self::get_peer_token(&mut reader, &peer, &inner).await {
                        Ok(token) => token,
                        Err(er) => {
                            log::debug!("user:{}:{},disconnect it", addr, er);
                            return Ok(());
                        }
                    };

                    Self::read_buff_byline(&mut reader, &peer, &token).await?;
                    token
                        .call_special_function(SpecialFunctionTag::Disconnect as i32)
                        .await?;
                    inner
                        .async_tokens
                        .peer_disconnect(token.get_session_id())
                        .await?;
                    Ok(())
                })
                .build()
                .await;
            NetXServer { inner, serv }
        }
        }else if #[cfg(feature = "use_rustls")]{
        /// new rustls tls encryption service
        #[inline]
        pub async fn new_tls(
            acceptor:&'static TlsAcceptor,
            option: ServerOption,
            impl_controller: T,
        ) -> NetXServer<T> {
            let request_out_time = option.request_out_time;
            let session_save_time = option.session_save_time;
            let async_tokens =
                AsyncTokenManager::new(impl_controller, request_out_time, session_save_time);
            let inner = Arc::new(NetXServerInner {
                option,
                async_tokens,
            });
            let serv = Builder::new(&inner.option.addr)
                .set_connect_event(|addr| {
                    log::debug!("{} connect", addr);
                    true
                })
                .set_stream_init(move |tcp_stream| async move {
                   Ok(MaybeStream::ServerTls(acceptor.accept(tcp_stream).await?))
                })
                .set_input_event(|mut reader, peer, inner| async move {
                    let addr = peer.addr();
                    let token = match Self::get_peer_token(&mut reader, &peer, &inner).await {
                        Ok(token) => token,
                        Err(er) => {
                            log::debug!("user:{}:{},disconnect it", addr, er);
                            return Ok(());
                        }
                    };

                    Self::read_buff_byline(&mut reader, &peer, &token).await?;
                    token
                        .call_special_function(SpecialFunctionTag::Disconnect as i32)
                        .await?;
                    inner
                        .async_tokens
                        .peer_disconnect(token.get_session_id())
                        .await;
                    Ok(())
                })
                .build()
                .await;
            NetXServer { inner, serv }
        }
        }
    }

    /// new service
    #[inline]
    pub async fn new(option: ServerOption, impl_controller: T) -> NetXServer<T> {
        let request_out_time = option.request_out_time;
        let session_save_time = option.session_save_time;
        let async_tokens =
            AsyncTokenManager::new(impl_controller, request_out_time, session_save_time);
        let inner = Arc::new(NetXServerInner {
            option,
            async_tokens,
        });
        let serv = Builder::new(&inner.option.addr)
            .set_connect_event(|addr| {
                log::debug!("{} connect", addr);
                true
            })
            .set_stream_init(|tcp_stream| async move { Ok(MaybeStream::Plain(tcp_stream)) })
            .set_input_event(|mut reader, peer, inner| async move {
                let addr = peer.addr();
                let token = match Self::get_peer_token(&mut reader, &peer, &inner).await {
                    Ok(token) => token,
                    Err(er) => {
                        log::debug!("user:{}:{},disconnect it", addr, er);
                        return Ok(());
                    }
                };

                Self::read_buff_byline(&mut reader, &peer, &token).await?;
                token
                    .call_special_function(SpecialFunctionTag::Disconnect as i32)
                    .await?;
                inner
                    .async_tokens
                    .peer_disconnect(token.get_session_id())
                    .await;
                Ok(())
            })
            .build()
            .await;
        NetXServer { inner, serv }
    }

    #[inline]
    async fn get_peer_token(
        mut reader: &mut NetReadHalf,
        peer: &Arc<NetPeer>,
        inner: &Arc<NetXServerInner<T>>,
    ) -> Result<NetxToken<T::Controller>> {
        let cmd = reader.read_i32_le().await?;
        if cmd != 1000 {
            Self::send_to_key_verify_msg(peer, true, "not verify key").await?;
            bail!("not verify key")
        }
        let name = reader.read_string().await?;
        if !inner.option.service_name.is_empty() && name != inner.option.service_name {
            Self::send_to_key_verify_msg(peer, true, "service name error").await?;
            bail!("IP:{} service name:{} error", peer.addr(), name)
        }
        let password = reader.read_string().await?;
        if !inner.option.verify_key.is_empty() && password != inner.option.verify_key {
            Self::send_to_key_verify_msg(peer, true, "service verify key error").await?;
            bail!("IP:{} verify key:{} error", peer.addr(), name)
        }
        Self::send_to_key_verify_msg(peer, false, "verify success").await?;
        let session = reader.read_i64_le().await?;
        let token = if session == 0 {
            inner
                .async_tokens
                .create_token(Arc::downgrade(&inner.async_tokens))
                .await?
        } else {
            match inner.async_tokens.get_token(session).await {
                Some(token) => token,
                None => {
                    inner
                        .async_tokens
                        .create_token(Arc::downgrade(&inner.async_tokens))
                        .await?
                }
            }
        };

        Ok(token)
    }

    #[inline]
    async fn read_buff_byline(
        reader: &mut NetReadHalf,
        peer: &Arc<NetPeer>,
        token: &NetxToken<T::Controller>,
    ) -> Result<()> {
        token.set_peer(Some(Arc::downgrade(peer))).await;
        token
            .call_special_function(SpecialFunctionTag::Connect as i32)
            .await?;
        Self::data_reading(reader, peer, token).await?;
        Ok(())
    }

    #[inline]
    async fn data_reading(
        mut reader: &mut NetReadHalf,
        peer: &Arc<NetPeer>,
        token: &NetxToken<T::Controller>,
    ) -> Result<()> {
        while let Ok(mut dr) = reader.read_buff().await {
            let cmd = dr.read_fixed::<i32>()?;
            match cmd {
                2000 => {
                    Self::send_to_session_id(peer, token.get_session_id()).await?;
                }
                2400 => {
                    let tt = dr.read_fixed::<u8>()?;
                    let cmd = dr.read_fixed::<i32>()?;
                    let serial = dr.read_fixed::<i64>()?;
                    match tt {
                        0 => {
                            let run_token = token.clone();
                            tokio::spawn(async move {
                                let _ = run_token.execute_controller(tt, cmd, dr).await;
                            });
                        }
                        1 => {
                            let run_token = token.clone();
                            tokio::spawn(async move {
                                let res = run_token.execute_controller(tt, cmd, dr).await;
                                if let Err(er) = run_token
                                    .send(Self::get_result_buff(serial, res).into_inner())
                                    .await
                                {
                                    log::error!("send buff 1 error:{}", er);
                                }
                            });
                        }
                        2 => {
                            let run_token = token.clone();
                            tokio::spawn(async move {
                                let res = run_token.execute_controller(tt, cmd, dr).await;
                                if let Err(er) = run_token
                                    .send(Self::get_result_buff(serial, res).into_inner())
                                    .await
                                {
                                    log::error!("send buff {} error:{}", serial, er);
                                }
                            });
                        }
                        _ => {
                            log::error!("not found call type:{}", tt)
                        }
                    }
                }
                2500 => {
                    let serial = dr.read_fixed::<i64>()?;
                    token.set_result(serial, dr).await?;
                }
                _ => {
                    log::error!("not found cmd:{}", cmd)
                }
            }
        }
        Ok(())
    }

    #[inline]
    fn get_result_buff(serial: i64, result: RetResult) -> Data {
        let mut data = Data::with_capacity(1024);

        data.write_fixed(0u32);
        data.write_fixed(2500u32);
        data.write_fixed(serial);

        if result.is_error {
            data.write_fixed(true);
            data.write_fixed(result.error_id);
            data.write_fixed(result.msg);
        } else {
            data.write_fixed(false);
            data.write_fixed(result.arguments.len() as u32);
            for argument in result.arguments {
                data.write_fixed(argument.into_inner());
            }
        }

        let len = data.len();
        (&mut data[0..4]).put_u32_le(len as u32);
        data
    }
    #[inline]
    async fn send_to_session_id(peer: &Arc<NetPeer>, session_id: i64) -> Result<()> {
        let mut data = Data::new();
        data.write_fixed(0u32);
        data.write_fixed(2000i32);
        data.write_fixed(session_id);
        let len = data.len();
        (&mut data[0..4]).put_u32_le(len as u32);
        peer.send_all(data.into_inner()).await
    }
    #[inline]
    async fn send_to_key_verify_msg(peer: &Arc<NetPeer>, is_err: bool, msg: &str) -> Result<()> {
        let mut data = Data::new();
        data.write_fixed(0u32);
        data.write_fixed(1000i32);
        data.write_fixed(is_err);
        data.write_fixed(msg);
        data.write_fixed(1u8);
        let len = data.len();
        (&mut data[0..4]).put_u32_le(len as u32);
        peer.send_all(data.into_inner()).await
    }

    #[inline]
    pub fn get_token_manager(&self) -> Weak<dyn ITokenManager<T::Controller>> {
        Arc::downgrade(&self.inner.async_tokens) as Weak<dyn ITokenManager<T::Controller>>
    }

    #[inline]
    pub async fn start(&self) -> Result<JoinHandle<Result<()>>> {
        self.serv.start(self.inner.clone()).await
    }
    #[inline]
    pub async fn start_block(&self) -> Result<()> {
        self.serv.start_block(self.inner.clone()).await
    }
}
