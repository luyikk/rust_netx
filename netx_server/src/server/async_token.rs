use crate::async_token_manager::IAsyncTokenManager;
use crate::{IController, NetPeer, RetResult};
use anyhow::{anyhow, bail, Result};
use aqueue::Actor;
use async_oneshot::{oneshot, Receiver, Sender};
use data_rw::{Data, DataOwnedReader};
use std::collections::{HashMap, VecDeque};
use std::ops::Deref;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Weak};
use tcpserver::IPeer;
use tokio::time::Instant;

pub struct AsyncToken {
    session_id: i64,
    controller: Option<Arc<dyn IController>>,
    peer: Option<Weak<NetPeer>>,
    manager: Weak<dyn IAsyncTokenManager>,
    result_dict: HashMap<i64, Sender<Result<DataOwnedReader>>>,
    serial_atomic: AtomicI64,
    request_queue: VecDeque<(i64, Instant)>,
}

unsafe impl Send for AsyncToken {}
unsafe impl Sync for AsyncToken {}

pub type NetxToken = Arc<Actor<AsyncToken>>;

impl AsyncToken {
    pub(crate) fn new(session_id: i64, manager: Weak<dyn IAsyncTokenManager>) -> AsyncToken {
        AsyncToken {
            session_id,
            controller: None,
            peer: None,
            manager,
            result_dict: Default::default(),
            serial_atomic: AtomicI64::new(1),
            request_queue: Default::default(),
        }
    }
}

impl Drop for AsyncToken {
    fn drop(&mut self) {
        log::debug!("token session_id:{} drop", self.session_id);
    }
}

impl AsyncToken {
    #[inline]
    pub(crate) async fn call_special_function(&self, cmd_tag: i32) -> Result<()> {
        if let Some(ref controller) = self.controller {
            controller
                .call(1, cmd_tag, DataOwnedReader::new(vec![0; 4]))
                .await?;
        }
        Ok(())
    }

    #[inline]
    pub(crate) async fn run_controller(
        &self,
        tt: u8,
        cmd: i32,
        dr: DataOwnedReader,
    ) -> Result<RetResult> {
        if let Some(ref controller) = self.controller {
            return controller.call(tt, cmd, dr).await;
        }
        bail!("controller is none")
    }

    #[inline]
    pub(crate) fn new_serial(&self) -> i64 {
        self.serial_atomic.fetch_add(1, Ordering::Acquire)
    }

    #[inline]
    pub fn set_error(&mut self, serial: i64, err: anyhow::Error) -> Result<()> {
        if let Some(mut tx) = self.result_dict.remove(&serial) {
            tx.send(Err(err)).map_err(|_| anyhow!("rx is close"))
        } else {
            Ok(())
        }
    }
    #[inline]
    pub fn check_request_timeout(&mut self, request_out_time: u32) {
        while let Some(item) = self.request_queue.pop_back() {
            if item.1.elapsed().as_millis() as u32 >= request_out_time {
                if let Err(er) = self.set_error(item.0, anyhow!("time out")) {
                    log::error!("check err:{}", er);
                }
            } else {
                self.request_queue.push_back(item);
                break;
            }
        }
    }
}

#[async_trait::async_trait]
pub(crate) trait IAsyncTokenInner {
    /// set controller
    async fn set_controller(&self, controller: Arc<dyn IController>) -> Result<()>;
    /// clean all controller fun map
    async fn clear_controller_fun_maps(&self) -> Result<()>;
    /// set peer
    async fn set_peer(&self, peer: Option<Weak<NetPeer>>) -> Result<()>;
    /// call special function disconnect or connect , close
    async fn call_special_function(&self, cmd_tag: i32) -> Result<()>;
    /// run netx controller
    async fn run_controller(&self, tt: u8, cmd: i32, data: DataOwnedReader) -> RetResult;
    /// set response result
    async fn set_result(&self, serial: i64, data: DataOwnedReader) -> Result<()>;
    /// set response error
    async fn set_error(&self, serial: i64, err: anyhow::Error) -> Result<()>;
    /// check request timeout
    async fn check_request_timeout(&self, request_out_time: u32) -> Result<()>;
}

#[async_trait::async_trait]
impl IAsyncTokenInner for Actor<AsyncToken> {
    #[inline]
    async fn set_controller(&self, controller: Arc<dyn IController>) -> Result<()> {
        self.inner_call(|inner| async move {
            inner.get_mut().controller = Some(controller);
            Ok(())
        })
        .await
    }

    #[inline]
    async fn clear_controller_fun_maps(&self) -> Result<()> {
        self.inner_call(|inner| async move {
            inner.get_mut().controller = None;
            Ok(())
        })
        .await
    }

    #[inline]
    async fn set_peer(&self, peer: Option<Weak<NetPeer>>) -> Result<()> {
        self.inner_call(|inner| async move {
            inner.get_mut().peer = peer;
            Ok(())
        })
        .await
    }

    #[inline]
    async fn call_special_function(&self, cmd_tag: i32) -> Result<()> {
        unsafe { self.deref_inner().call_special_function(cmd_tag).await }
    }

    #[inline]
    async fn run_controller(&self, tt: u8, cmd: i32, dr: DataOwnedReader) -> RetResult {
        unsafe {
            match self.deref_inner().run_controller(tt, cmd, dr).await {
                Ok(res) => res,
                Err(err) => {
                    log::error!(
                        "session id:{} call cmd:{} error:{}",
                        self.get_session_id(),
                        cmd,
                        err
                    );
                    RetResult::error(
                        -1,
                        format!(
                            "session id:{} call cmd:{} error:{}",
                            self.get_session_id(),
                            cmd,
                            err
                        ),
                    )
                }
            }
        }
    }

    #[inline]
    async fn set_result(&self, serial: i64, dr: DataOwnedReader) -> Result<()> {
        let have_tx: Option<Sender<Result<DataOwnedReader>>> = self
            .inner_call(|inner| async move { inner.get_mut().result_dict.remove(&serial) })
            .await;

        if let Some(mut tx) = have_tx {
            return tx.send(Ok(dr)).map_err(|_| anyhow!("close rx"));
        } else {
            match RetResult::from(dr) {
                Ok(res) => match res.check() {
                    Ok(_) => {
                        log::error!("not found 2 {}", serial)
                    }
                    Err(err) => {
                        log::error!("{}", err)
                    }
                },
                Err(er) => {
                    log::error!("not found {} :{}", serial, er)
                }
            }
        }
        Ok(())
    }

    #[inline]
    async fn set_error(&self, serial: i64, err: anyhow::Error) -> Result<()> {
        self.inner_call(|inner| async move { inner.get_mut().set_error(serial, err) })
            .await
    }

    #[inline]
    async fn check_request_timeout(&self, request_out_time: u32) -> Result<()> {
        self.inner_call(|inner| async move {
            inner.get_mut().check_request_timeout(request_out_time);
            Ok(())
        })
        .await
    }
}

#[async_trait::async_trait]
pub trait IAsyncToken {
    /// get netx session id
    fn get_session_id(&self) -> i64;
    /// new serial id
    fn new_serial(&self) -> i64;
    /// get tcp socket peer
    async fn get_peer(&self) -> Result<Option<Weak<NetPeer>>>;
    /// send buff
    async fn send<B: Deref<Target = [u8]> + Send + Sync + 'static>(&self, buff: B)
        -> Result<usize>;
    /// get netx token by session id
    async fn get_token(&self, session_id: i64) -> Result<Option<NetxToken>>;
    /// get all netx token
    async fn get_all_tokens(&self) -> Result<Vec<NetxToken>>;
    /// call
    async fn call(&self, serial: i64, buff: Data) -> Result<RetResult>;
    /// run
    async fn run(&self, buff: Data) -> Result<()>;
    /// is disconnect
    async fn is_disconnect(&self) -> Result<bool>;
}

#[async_trait::async_trait]
impl IAsyncToken for Actor<AsyncToken> {
    #[inline]
    fn get_session_id(&self) -> i64 {
        unsafe { self.deref_inner().session_id }
    }

    #[inline]
    fn new_serial(&self) -> i64 {
        unsafe { self.deref_inner().new_serial() }
    }

    #[inline]
    async fn get_peer(&self) -> Result<Option<Weak<NetPeer>>> {
        self.inner_call(|inner| async move { Ok(inner.get_mut().peer.clone()) })
            .await
    }

    #[inline]
    async fn send<B: Deref<Target = [u8]> + Send + Sync + 'static>(
        &self,
        buff: B,
    ) -> Result<usize> {
        unsafe {
            if let Some(ref peer) = self.deref_inner().peer {
                let peer = peer
                    .upgrade()
                    .ok_or_else(|| anyhow!("token:{} tcp disconnect", self.get_session_id()))?;
                return peer.send(buff).await;
            }
            bail!("token:{} tcp disconnect", self.get_session_id())
        }
    }

    #[inline]
    async fn get_token(&self, session_id: i64) -> Result<Option<NetxToken>> {
        self.inner_call(|inner| async move {
            let manager = inner
                .get()
                .manager
                .upgrade()
                .ok_or_else(|| anyhow!("manager upgrade fail"))?;
            manager.get_token(session_id).await
        })
        .await
    }

    #[inline]
    async fn get_all_tokens(&self) -> Result<Vec<NetxToken>> {
        self.inner_call(|inner| async move {
            let manager = inner
                .get()
                .manager
                .upgrade()
                .ok_or_else(|| anyhow!("manager upgrade fail"))?;
            manager.get_all_tokens().await
        })
        .await
    }

    #[inline]
    async fn call(&self, serial: i64, buff: Data) -> Result<RetResult> {
        let (peer, rx): (Arc<NetPeer>, Receiver<Result<DataOwnedReader>>) = self
            .inner_call(|inner| async move {
                if let Some(ref net) = inner.get().peer {
                    let peer = net.upgrade().ok_or_else(|| anyhow!("call peer is null"))?;
                    let (tx, rx): (
                        Sender<Result<DataOwnedReader>>,
                        Receiver<Result<DataOwnedReader>>,
                    ) = oneshot();
                    if inner.get_mut().result_dict.contains_key(&serial) {
                        bail!("serial is have")
                    }
                    if inner.get_mut().result_dict.insert(serial, tx).is_none() {
                        inner
                            .get_mut()
                            .request_queue
                            .push_front((serial, Instant::now()));
                    }
                    Ok((peer, rx))
                } else {
                    bail!("call not connect")
                }
            })
            .await?;
        peer.send(buff.into_inner()).await?;
        match rx.await {
            Err(_) => Err(anyhow!("tx is Close")),
            Ok(data) => Ok(RetResult::from(data?)?),
        }
    }

    #[inline]
    async fn run(&self, buff: Data) -> Result<()> {
        let peer: Arc<NetPeer> = self
            .inner_call(|inner| async move {
                if let Some(ref net) = inner.get().peer {
                    Ok(net.upgrade().ok_or_else(|| anyhow!("run not connect"))?)
                } else {
                    bail!("run not connect")
                }
            })
            .await?;
        peer.send(buff.into_inner()).await?;
        Ok(())
    }

    #[inline]
    async fn is_disconnect(&self) -> Result<bool> {
        self.inner_call(|inner| async move {
            if let Some(ref peer) = inner.get().peer {
                if let Some(peer) = peer.upgrade() {
                    return peer.is_disconnect().await;
                }
            }
            Ok(true)
        })
        .await
    }
}

#[macro_export]
macro_rules! call_peer {
    (@uint $($x:tt)*)=>(());
    (@count $($rest:expr),*)=>(<[()]>::len(&[$(call_peer!(@uint $rest)),*]));
    ($peer:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call_peer!(@count $($args),*) as i32;
            let serial=$peer.new_serial();
            data.write_fixed(0u32);
            data.write_fixed(2400u32);
            data.write_fixed(2u8);
            data.write_fixed($cmd);
            data.write_fixed(serial);
            data.write_fixed(args_count);
            $(data.pack_serialize($args)?;)*
            let len=data.len();
            (&mut data[0..4]).put_u32_le(len as u32);
            let mut ret= $peer.call(serial,data).await?.check()?;
            ret.deserialize()?
    });
    (@result $peer:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call_peer!(@count $($args),*) as i32;
            let serial=$peer.new_serial();
            data.write_fixed(0u32);
            data.write_fixed(2400u32);
            data.write_fixed(2u8);
            data.write_fixed($cmd);
            data.write_fixed(serial);
            data.write_fixed(args_count);
            $(data.pack_serialize($args)?;)*
            let len=data.len();
            (&mut data[0..4]).put_u32_le(len as u32);
            $peer.call(serial,data).await?
    });
    (@run $peer:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call_peer!(@count $($args),*) as i32;
            let serial=$peer.new_serial();
            data.write_fixed(0u32);
            data.write_fixed(2400u32);
            data.write_fixed(0u8);
            data.write_fixed($cmd);
            data.write_fixed(serial);
            data.write_fixed(args_count);
            $(data.pack_serialize($args)?;)*
            let len=data.len();
            (&mut data[0..4]).put_u32_le(len as u32);
            $peer.run(data).await?;
    });
     (@run_not_err $peer:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call_peer!(@count $($args),*) as i32;
            let serial=$peer.new_serial();
            data.write_fixed(0u32);
            data.write_fixed(2400u32);
            data.write_fixed(0u8);
            data.write_fixed($cmd);
            data.write_fixed(serial);
            data.write_fixed(args_count);
            $(
              if let Err(err)=  data.pack_serialize($args){
                 log::error!{"pack_serialize {} is error:{}",$cmd,err};
              }
            )*
            let len=data.len();
            (&mut data[0..4]).put_u32_le(len as u32);
            if let Err(err)= $peer.run(data).await{
                 log::warn!{"run {} is error:{}",$cmd,err}
            }
    });
    (@checkrun $peer:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call_peer!(@count $($args),*) as i32;
            let serial=$peer.new_serial();
            data.write_fixed(0u32);
            data.write_fixed(2400u32);
            data.write_fixed(1u8);
            data.write_fixed($cmd);
            data.write_fixed(serial);
            data.write_fixed(args_count);
            $(data.pack_serialize($args)?;)*
            let len=data.len();
            (&mut data[0..4]).put_u32_le(len as u32);
            $peer.call(serial,data).await?.check()?;
    });

}

///  make Box<dyn $interface> will clone $client
#[macro_export]
macro_rules! impl_interface {
    ($token:expr=>$interface:ty) => {
        paste::paste! {
              Box::new([<___impl_ $interface _call>]::new($token.clone()))  as  Box<dyn $interface>
        }
    };
}

/// make $interface struct  will ref $client
#[macro_export]
macro_rules! impl_ref {
    ($client:expr=>$interface:ty) => {
        paste::paste! {
            [<___impl_ $interface _call>]::new_ref(&$client)
        }
    };
}
