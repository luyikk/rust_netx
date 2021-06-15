use aqueue::Actor;
use std::collections::{HashMap, VecDeque};
use crate::controller::FunctionInfo;
use std::sync::{Arc, Weak};
use tcpserver::{TCPPeer, IPeer};
use data_rw::Data;
use crate::RetResult;
use log::*;
use crate::async_token_manager::IAsyncTokenManager;
use async_oneshot::{oneshot, Sender, Receiver};
use std::sync::atomic::{AtomicI64, Ordering};
use tokio::time::Instant;
use anyhow::*;

pub struct AsyncToken{
    sessionid:i64,
    controller_fun_register_dict:Option<HashMap<i32,Box<dyn FunctionInfo>>>,
    peer:Option<Weak<Actor<TCPPeer>>>,
    manager: Weak<dyn IAsyncTokenManager>,
    result_dict:HashMap<i64,Sender<Result<Data>>>,
    serial_atomic:AtomicI64,
    request_queue:VecDeque<(i64,Instant)>,
}

unsafe impl Send for AsyncToken{}
unsafe impl Sync for AsyncToken{}

pub type NetxToken=Arc<Actor<AsyncToken>>;

impl AsyncToken{
    pub(crate) fn new(sessionid:i64,manager: Weak<dyn IAsyncTokenManager>)->AsyncToken{
        AsyncToken{
            sessionid,
            controller_fun_register_dict:None,
            peer:None,
            manager,
            result_dict: Default::default(),
            serial_atomic:AtomicI64::new(1),
            request_queue:Default::default()
        }
    }
}

impl Drop for AsyncToken{
    fn drop(&mut self) {
        debug!("token sessionid:{} drop",self.sessionid);
    }
}

impl AsyncToken{

    #[inline]
    pub(crate) async fn call_special_function(&self,cmd:i32)->Result<()>{
        if let Some(ref dict) = self.controller_fun_register_dict {
            if let Some(func) = dict.get(&cmd) {
                func.call(Data::with_len(4,0)).await?;
            }
        }
        Ok(())
    }

    #[inline]
    pub (crate) async fn run_controller(&self, tt:u8,cmd:i32,data:Data)->Result<RetResult> {
        if let Some(ref dict) = self.controller_fun_register_dict {
            if let Some(func) = dict.get(&cmd) {
                return if func.function_type() != tt {
                     bail!(" cmd:{} function type error:{}", cmd, tt)
                }else {
                     func.call(data).await
                }
            }
        }
        bail!("not found cmd:{}", cmd)
    }

    #[inline]
    pub (crate) fn new_serial(&self)->i64{
        self.serial_atomic.fetch_add(1,Ordering::Acquire)
    }

    #[inline]
    pub fn set_error(&mut self,serial:i64,err:anyhow::Error)->Result<()>{
        if let Some(tx)= self.result_dict.remove(&serial){
            tx.send(Err(err)).map_err(|_|anyhow!("rx is close"))
        }
        else{
            Ok(())
        }
    }
    #[inline]
    pub fn check_request_timeout(&mut self,request_out_time:u32){
        while let Some(item) = self.request_queue.pop_back() {
            if item.1.elapsed().as_millis() as u32 >= request_out_time {
                if let Err(er) = self.set_error(item.0, anyhow!("time out")) {
                    error!("check err:{}", er);
                }
            } else {
                self.request_queue.push_back(item);
                break;
            }
        }
    }
}

#[async_trait::async_trait]
pub trait IAsyncToken{
    fn get_sessionid(&self)->i64;
    fn new_serial(&self)->i64;
    async fn set_controller_fun_maps(&self,map:HashMap<i32,Box<dyn FunctionInfo>>)->Result<()>;
    async fn clear_controller_fun_maps(&self) ->Result<()>;
    async fn set_peer(&self,peer:Option<Weak<Actor<TCPPeer>>>)->Result<()>;
    async fn get_peer(&self)->Result<Option<Weak<Actor<TCPPeer>>>>;
    async fn call_special_function(&self,cmd:i32)->Result<()>;
    async fn run_controller(&self, tt:u8,cmd:i32,data:Data)->RetResult;
    async fn send<'a>(&'a self, buff: &'a [u8]) -> Result<usize>;
    async fn get_token(&self,sessionid:i64)->Result<Option<NetxToken>>;
    async fn get_all_tokens(&self)->Result<Vec<NetxToken>>;
    async fn call(&self,serial:i64,buff: Data)->Result<RetResult>;
    async fn run(&self,buff: Data) -> Result<()>;
    async fn set_result(&self,serial:i64,data:Data)->Result<()>;
    async fn set_error(&self,serial:i64,err:anyhow::Error)->Result<()>;
    async fn check_request_timeout(&self,request_out_time:u32)->Result<()>;
    async fn is_disconnect(&self) ->Result<bool>;

}

#[async_trait::async_trait]
impl IAsyncToken for Actor<AsyncToken>{
    #[inline]
    fn get_sessionid(&self) -> i64 {
        unsafe {
            self.deref_inner().sessionid
        }
    }

    #[inline]
    fn new_serial(&self) -> i64 {
        unsafe {
            self.deref_inner().new_serial()
        }
    }

    #[inline]
    async fn set_controller_fun_maps(&self, map: HashMap<i32, Box<dyn FunctionInfo>>) -> Result<()> {
        self.inner_call(async move|inner|{
            inner.get_mut().controller_fun_register_dict=Some(map);
            Ok(())
        }).await
    }

    #[inline]
    async fn clear_controller_fun_maps(&self) -> Result<()> {
        self.inner_call(async move|inner|{
            inner.get_mut().controller_fun_register_dict=None;
            Ok(())
        }).await
    }

    #[inline]
    async fn set_peer(&self, peer: Option<Weak<Actor<TCPPeer>>>) -> Result<()> {
        self.inner_call(async move|inner|{
            inner.get_mut().peer=peer;
            Ok(())
        }).await
    }

    #[inline]
    async fn get_peer(&self) -> Result<Option<Weak<Actor<TCPPeer>>>> {
        self.inner_call(async move|inner|{
            Ok(inner.get_mut().peer.clone())
        }).await
    }

    #[inline]
    async fn call_special_function(&self, cmd: i32) -> Result<()> {
       unsafe{
           self.deref_inner().call_special_function(cmd).await
       }
    }

    #[inline]
    async fn run_controller(&self, tt: u8, cmd: i32, data: Data) -> RetResult{
        unsafe {
            match self.deref_inner().run_controller(tt,cmd,data).await{
                Ok(res)=>res,
                Err(err)=>{
                    error!("sessionid:{} call cmd:{} error:{}",self.get_sessionid(),cmd, err);
                    RetResult::error(-1,format!("sessionid:{} call cmd:{} error:{}",self.get_sessionid(),cmd, err))
                }
            }
        }
    }

    #[inline]
    async fn send<'a>(&'a self, buff: &'a [u8]) -> Result<usize> {
        unsafe {
            if let Some(ref peer)= self.deref_inner().peer {
                let peer = peer.upgrade()
                    .ok_or_else(|| anyhow!("token:{} tcp disconnect",self.get_sessionid()))?;
                return peer.send(buff).await
            }
            bail!("token:{} tcp disconnect",self.get_sessionid())
        }
    }

    #[inline]
    async fn get_token(&self, sessionid: i64) -> Result<Option<NetxToken>> {
        self.inner_call(async move|inner|{
            let manager= inner.get().manager.upgrade()
                .ok_or_else(||anyhow!("manager upgrade fail"))?;
            manager.get_token(sessionid).await
        }).await
    }

    #[inline]
    async fn get_all_tokens(&self) -> Result<Vec<NetxToken>> {
        self.inner_call(async move|inner|{
            let manager= inner.get().manager.upgrade()
                .ok_or_else(||anyhow!("manager upgrade fail"))?;
            manager.get_all_tokens().await
        }).await
    }

    #[inline]
    async fn call(&self, serial: i64, buff: Data) -> Result<RetResult> {
        let (peer,rx):(Arc<Actor<TCPPeer>>,Receiver<Result<Data>>)=self.inner_call(async move|inner|{
            if let Some(ref net)=inner.get().peer {
                let peer = net.upgrade()
                    .ok_or_else(|| anyhow!("call peer is null"))?;
                let (tx, rx): (Sender<Result<Data>>, Receiver<Result<Data>>) = oneshot();
                if inner.get_mut().result_dict.contains_key(&serial) {
                    bail!("serial is have")
                }
                if inner.get_mut().result_dict.insert(serial, tx).is_none() {
                    inner.get_mut().request_queue.push_front((serial, Instant::now()));
                }
                Ok((peer, rx))
            }else{
                bail!("call not connect")
            }
        }).await?;
        peer.send(&buff).await?;
        match rx.await {
            Err(_)=>{
                Err(anyhow!("tx is Close"))
            },
            Ok(data)=> {
                Ok(RetResult::from(data?)?)
            }
        }
    }

    #[inline]
    async fn run(&self, buff: Data) -> Result<()> {
        let peer:Arc<Actor<TCPPeer>>= self.inner_call(async move|inner|{
            if let Some(ref net)=inner.get().peer{
                Ok(net.upgrade().ok_or_else(||anyhow!("run not connect"))?)
            }else{
                bail!("run not connect")
            }
        }).await?;
        peer.send(&buff).await?;
        Ok(())
    }

    #[inline]
    async fn set_result(&self, serial: i64, data: Data) -> Result<()> {
        let have_tx:Option<Sender<Result<Data>>>= self.inner_call(async move|inner|{
            Ok(inner.get_mut().result_dict.remove(&serial))
        }).await?;

        if let Some(tx)=have_tx{
            return tx.send(Ok(data)).map_err(|_|anyhow!("close rx"));
        }
        else{
            match RetResult::from(data){
                Ok(res)=>{
                    match res.check(){
                        Ok(_)=>{
                            error!("not found 2 {}",serial)
                        },
                        Err(err)=> {
                            error!("{}",err)
                        }
                    }
                },
                Err(er)=>{
                    error!("not found {} :{}",serial,er)
                }
            }
        }
        Ok(())
    }

    #[inline]
    async fn set_error(&self, serial: i64, err: anyhow::Error) -> Result<()> {
        self.inner_call(async move|inner|{
             inner.get_mut().set_error(serial,err)
        }).await
    }

    #[inline]
    async fn check_request_timeout(&self, request_out_time: u32) -> Result<()> {
        self.inner_call(async move|inner|{
            inner.get_mut().check_request_timeout(request_out_time);
            Ok(())
        }).await
    }
    #[inline]
    async fn is_disconnect(&self) -> Result<bool> {
        self.inner_call(async move|inner|{
            if let Some(ref peer)= inner.get().peer{
                if let Some(peer)= peer.upgrade(){
                    return peer.is_disconnect().await
                }
            }
            Ok(true)
        }).await
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
            data.write_to_le(&0u32);
            data.write_to_le(&2400u32);
            data.write_to_le(&2u8);
            data.write_to_le(&$cmd);
            data.write_to_le(&serial);
            data.write_to_le(&args_count);
            $(data.msgpack_serialize($args)?;)*
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
            data.write_to_le(&0u32);
            data.write_to_le(&2400u32);
            data.write_to_le(&2u8);
            data.write_to_le(&$cmd);
            data.write_to_le(&serial);
            data.write_to_le(&args_count);
            $(data.msgpack_serialize($args)?;)*
            let len=data.len();
            (&mut data[0..4]).put_u32_le(len as u32);
            $peer.call(serial,data).await?
    });
    (@run $peer:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call_peer!(@count $($args),*) as i32;
            let serial=$peer.new_serial();
            data.write_to_le(&0u32);
            data.write_to_le(&2400u32);
            data.write_to_le(&0u8);
            data.write_to_le(&$cmd);
            data.write_to_le(&serial);
            data.write_to_le(&args_count);
            $(data.msgpack_serialize($args)?;)*
            let len=data.len();
            (&mut data[0..4]).put_u32_le(len as u32);
            $peer.run(data).await?;
    });
     (@run_not_err $peer:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call_peer!(@count $($args),*) as i32;
            let serial=$peer.new_serial();
            data.write_to_le(&0u32);
            data.write_to_le(&2400u32);
            data.write_to_le(&0u8);
            data.write_to_le(&$cmd);
            data.write_to_le(&serial);
            data.write_to_le(&args_count);
            $(
              if let Err(err)=  data.msgpack_serialize($args){
                 log::error!{"msgpack_serialize {} is error:{}",$cmd,err};
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
            data.write_to_le(&0u32);
            data.write_to_le(&2400u32);
            data.write_to_le(&1u8);
            data.write_to_le(&$cmd);
            data.write_to_le(&serial);
            data.write_to_le(&args_count);
            $(data.msgpack_serialize($args)?;)*
            let len=data.len();
            (&mut data[0..4]).put_u32_le(len as u32);
            $peer.call(serial,data).await?.check()?;
    });

}

#[macro_export]
macro_rules! impl_interface {
    ($token:expr=>$interface:ty) => (
      paste::paste!{
            Box::new([<___impl_ $interface _call>]::new($token.clone()))  as  Box<dyn $interface>
      }
    )
}