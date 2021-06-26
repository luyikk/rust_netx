use crate::async_token::{IAsyncToken, NetxToken};
use crate::async_token_manager::{IAsyncTokenManager, TokenManager};
use crate::controller::ICreateController;
use crate::server::async_token_manager::AsyncTokenManager;
use crate::{ReadHalfExt, RetResult, ServerOption};
use anyhow::*;
use aqueue::Actor;
use bytes::BufMut;
use data_rw::Data;
use log::*;
use std::sync::{Arc, Weak};
use tcpserver::{Builder, IPeer, ITCPServer, TCPPeer};
use tokio::io::{AsyncReadExt, ReadHalf};
use tokio::task::JoinHandle;

cfg_if::cfg_if! {
if #[cfg(feature = "tls")]{
   use openssl::ssl::{Ssl,SslAcceptor};
   use tokio::net::TcpStream;
   use tokio_openssl::SslStream;
   use std::time::Duration;
   use tokio::time::sleep;
   use std::pin::Pin;
   pub type NetPeer=Actor<TCPPeer<SslStream<TcpStream>>>;
   pub type NetReadHalf=ReadHalf<SslStream<TcpStream>>;

}else if #[cfg(feature = "tcp")]{
   use tokio::net::TcpStream;
   pub type NetPeer=Actor<TCPPeer<TcpStream>>;
   pub type NetReadHalf=ReadHalf<TcpStream>;
}
}

pub(crate) enum SpecialFunctionTag {
    Connect = 2147483647,
    Disconnect = 2147483646,
    Closed = 2147483645,
}
pub struct NetXServer<T> {
    option: ServerOption,
    serv: Arc<dyn ITCPServer<Arc<NetXServer<T>>>>,
    async_tokens: TokenManager<T>,
}
unsafe impl<T> Send for NetXServer<T> {}
unsafe impl<T> Sync for NetXServer<T> {}

impl<T> NetXServer<T>
where
    T: ICreateController + 'static,
{
    cfg_if::cfg_if! {
    if #[cfg(feature = "tls")]{
       #[inline]
       pub async fn new(ssl_acceptor:&'static SslAcceptor,option:ServerOption,impl_controller:T)->Arc<NetXServer<T>> {
          let serv = Builder::new(&option.addr).set_connect_event(|addr| {
             info!("{} connect", addr);
             true
          })
          .set_stream_init(async move |tcp_stream|{
             let ssl = Ssl::new(ssl_acceptor.context())?;
             let mut stream = SslStream::new(ssl, tcp_stream)?;
             sleep(Duration::from_millis(200)).await;
             Pin::new(&mut stream).accept().await?;
             Ok(stream)
          })
          .set_input_event(async move |mut reader, peer, serv| {
              let addr = peer.addr();
              let token = match Self::get_peer_token(&mut reader, &peer, &serv).await {
                    Ok(token) => token,
                    Err(er) => {
                       info!("user:{}:{},disconnect it", addr, er);
                       return Ok(())
                    }
              };

              Self::read_buff_byline(&mut reader, &peer, &token).await?;
              token.call_special_function(SpecialFunctionTag::Disconnect as i32).await?;
              serv.async_tokens.peer_disconnect(token.get_sessionid()).await?;
              Ok(())
          }).build().await;
          let request_out_time = option.request_out_time;
          let session_save_time = option.session_save_time;
          Arc::new(NetXServer {
             option,
             serv,
             async_tokens: AsyncTokenManager::new(impl_controller, request_out_time, session_save_time)
          })
       }
    }else if #[cfg(feature = "tcp")]{
       #[inline]
       pub async fn new(option:ServerOption,impl_controller:T)->Arc<NetXServer<T>> {
          let serv = Builder::new(&option.addr).set_connect_event(|addr| {
             info!("{} connect", addr);
             true
          })
          .set_stream_init(async move |tcp_stream|{
             Ok(tcp_stream)
           })
          .set_input_event(async move |mut reader, peer, serv| {
             let addr = peer.addr();
             let token = match Self::get_peer_token(&mut reader, &peer, &serv).await {
                Ok(token) => token,
                Err(er) => {
                   info!("user:{}:{},disconnect it", addr, er);
                   return Ok(())
                }
             };

             Self::read_buff_byline(&mut reader, &peer, &token).await?;
             token.call_special_function(SpecialFunctionTag::Disconnect as i32).await?;
             serv.async_tokens.peer_disconnect(token.get_sessionid()).await?;
             Ok(())
          }).build().await;
          let request_out_time = option.request_out_time;
          let session_save_time = option.session_save_time;
          Arc::new(NetXServer {
             option,
             serv,
             async_tokens: AsyncTokenManager::new(impl_controller, request_out_time, session_save_time)
          })
       }
    }}
    #[inline]
    async fn get_peer_token(
        mut reader: &mut NetReadHalf,
        peer: &Arc<NetPeer>,
        serv: &Arc<NetXServer<T>>,
    ) -> Result<NetxToken> {
        let cmd = reader.read_i32_le().await?;
        if cmd != 1000 {
            Self::send_to_key_verify_msg(peer, true, "not verify key").await?;
            bail!("not verify key")
        }
        let name = reader.read_string().await?;
        if !serv.option.service_name.is_empty() && name != serv.option.service_name {
            Self::send_to_key_verify_msg(peer, true, "service name error").await?;
            bail!("IP:{} service name:{} error", peer.addr(), name)
        }
        let password = reader.read_string().await?;
        if !serv.option.verify_key.is_empty() && password != serv.option.verify_key {
            Self::send_to_key_verify_msg(peer, true, "service verify key error").await?;
            bail!("IP:{} verify key:{} error", peer.addr(), name)
        }
        Self::send_to_key_verify_msg(peer, false, "verify success").await?;
        let session = reader.read_i64_le().await?;
        let token = if session == 0 {
            serv.async_tokens
                .create_token(Arc::downgrade(&serv.async_tokens) as Weak<dyn IAsyncTokenManager>)
                .await?
        } else {
            match serv.async_tokens.get_token(session).await? {
                Some(token) => token,
                None => {
                    serv.async_tokens
                        .create_token(
                            Arc::downgrade(&serv.async_tokens) as Weak<dyn IAsyncTokenManager>
                        )
                        .await?
                }
            }
        };

        Ok(token)
    }

    #[inline]
    async fn read_buff_byline(
        mut reader: &mut NetReadHalf,
        peer: &Arc<NetPeer>,
        token: &NetxToken,
    ) -> Result<()> {
        token.set_peer(Some(Arc::downgrade(peer))).await?;
        token
            .call_special_function(SpecialFunctionTag::Connect as i32)
            .await?;
        Self::data_reading(&mut reader, peer, token).await?;
        Ok(())
    }

    #[inline]
    async fn data_reading(
        mut reader: &mut NetReadHalf,
        peer: &Arc<NetPeer>,
        token: &NetxToken,
    ) -> Result<()> {
        while let Ok(mut dr) = reader.read_buff().await {
            let cmd = dr.read_fixed::<i32>()?;
            match cmd {
                2000 => {
                    Self::send_to_sessionid(peer, token.get_sessionid()).await?;
                }
                2400 => {
                    let tt = dr.read_fixed::<u8>()?;
                    let cmd = dr.read_fixed::<i32>()?;
                    let serial = dr.read_fixed::<i64>()?;
                    match tt {
                        0 => {
                            let run_token = token.clone();
                            tokio::spawn(async move {
                                let _ = run_token.run_controller(tt, cmd, dr).await;
                            });
                        }
                        1 => {
                            let run_token = token.clone();
                            tokio::spawn(async move {
                                let res = run_token.run_controller(tt, cmd, dr).await;
                                if let Err(er) =
                                    run_token.send(&Self::get_result_buff(serial, res)).await
                                {
                                    error!("send buff 1 error:{}", er);
                                }
                            });
                        }
                        2 => {
                            let run_token = token.clone();
                            tokio::spawn(async move {
                                let res = run_token.run_controller(tt, cmd, dr).await;
                                if let Err(er) =
                                    run_token.send(&Self::get_result_buff(serial, res)).await
                                {
                                    error!("send buff {} error:{}", serial, er);
                                }
                            });
                        }
                        _ => {
                            error!("not found call type:{}", tt)
                        }
                    }
                }
                2500 => {
                    let serial = dr.read_fixed::<i64>()?;
                    token.set_result(serial, dr).await?;
                }
                _ => {
                    error!("not found cmd:{}", cmd)
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
    async fn send_to_sessionid(peer: &Arc<NetPeer>, sessionid: i64) -> Result<usize> {
        let mut data = Data::new();
        data.write_fixed(0u32);
        data.write_fixed(2000i32);
        data.write_fixed(sessionid);
        let len = data.len();
        (&mut data[0..4]).put_u32_le(len as u32);
        peer.send(&data).await
    }
    #[inline]
    async fn send_to_key_verify_msg(peer: &Arc<NetPeer>, is_err: bool, msg: &str) -> Result<usize> {
        let mut data = Data::new();
        data.write_fixed(0u32);
        data.write_fixed(1000i32);
        data.write_fixed(is_err);
        data.write_fixed(msg);
        data.write_fixed(1u8);
        let len = data.len();
        (&mut data[0..4]).put_u32_le(len as u32);
        peer.send(&data).await
    }

    #[inline]
    pub async fn start(self: &Arc<Self>) -> Result<JoinHandle<Result<()>>> {
        Ok(self.serv.start(self.clone()).await?)
    }
    #[inline]
    pub async fn start_block(self: &Arc<Self>) -> Result<()> {
        self.serv.start_block(self.clone()).await
    }
}
