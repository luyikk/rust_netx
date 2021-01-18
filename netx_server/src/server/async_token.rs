use aqueue::{Actor, AResult, AError};
use std::collections::{HashMap, VecDeque};
use crate::controller::FunctionInfo;
use std::sync::{Arc, Weak};
use tcpserver::{TCPPeer, IPeer};
use data_rw::Data;
use crate::RetResult;
use std::error::Error;
use log::*;
use std::ops::Deref;
use crate::async_token_manager::IAsyncTokenManager;
use async_oneshot::{oneshot, Sender, Receiver};
use std::sync::atomic::{AtomicI64, Ordering};
use tokio::time::Instant;


pub struct AsyncToken{
    sessionid:i64,
    controller_fun_register_dict:Option<HashMap<i32,Box<dyn FunctionInfo>>>,
    peer:Option<Weak<Actor<TCPPeer>>>,
    manager: Weak<dyn IAsyncTokenManager>,
    result_dict:HashMap<i64,Sender<AResult<Data>>>,
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
    pub (crate) async fn run_controller(&self, tt:u8,cmd:i32,data:Data)->Result<RetResult,Box<dyn Error>> {
        if let Some(ref dict) = self.controller_fun_register_dict {
            if let Some(func) = dict.get(&cmd) {
                if func.function_type() != tt {
                    return Err(format!(" cmd:{} function type error:{}", cmd, tt).into());
                }

                return func.call(data).await
            }
        }
        debug!("not found cmd:{}",cmd);
        return Err(format!("not found cmd:{}", cmd).into());
    }

    #[inline]
    pub (crate) fn new_serial(&self)->i64{
        self.serial_atomic.fetch_add(1,Ordering::Acquire)
    }

    #[inline]
    pub fn set_result(&mut self,serial:i64,data:Data)->Result<(),Box<dyn Error+ Send + Sync>>{
        if let Some(tx)= self.result_dict.remove(&serial){
            return match tx.send(Ok(data)) {
                Err(_) => {
                    Err("close rx".into())
                },
                Ok(_) => {
                    Ok(())
                }
            }
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
    pub fn set_error(&mut self,serial:i64,err:AError)->Result<(),Box<dyn Error+ Send + Sync>>{
        if let Some(tx)= self.result_dict.remove(&serial){
            match tx.send(Err(err)) {
                Err(_) => {
                    Err("close rx".into())
                },
                Ok(_) => {
                    Ok(())
                }
            }
        }
        else{
            Ok(())
        }
    }

    #[inline]
    pub fn check_request_timeout(&mut self,request_out_time:u32){
        while let Some(item) = self.request_queue.pop_back() {
            if item.1.elapsed().as_millis() as u32 >= request_out_time {
                if let Err(er) = self.set_error(item.0, AError::StrErr("time out".into())) {
                    error!("check err:{}", er);
                }
                drop(item)
            } else {
                self.request_queue.push_back(item);
                break;
            }
        }
    }
}

#[aqueue::aqueue_trait]
pub trait IAsyncToken{
    fn get_sessionid(&self)->i64;
    fn new_serial(&self)->i64;
    async fn set_controller_fun_maps(&self,map:HashMap<i32,Box<dyn FunctionInfo>>)->AResult<()>;
    async fn clear_controller_fun_maps(&self) ->AResult<()>;
    async fn set_peer(&self,peer:Option<Weak<Actor<TCPPeer>>>)->AResult<()>;
    async fn run_controller(&self, tt:u8,cmd:i32,data:Data)->RetResult;
    async fn send<T: Deref<Target=[u8]> + Send + Sync + 'static>(&self, buff: T) -> AResult<usize>;
    async fn get_token(&self,sessionid:i64)->AResult<Option<NetxToken>>;
    async fn get_all_tokens(&self)->AResult<Vec<NetxToken>>;
    async fn call(&self,serial:i64,buff: Data)->Result<RetResult,Box<dyn Error>>;
    async fn run(&self,buff: Data) -> Result<(),Box<dyn Error>>;
    async fn set_result(&self,serial:i64,data:Data)->AResult<()>;
    async fn set_error(&self,serial:i64,err:AError)->AResult<()>;
    async fn check_request_timeout(&self,request_out_time:u32)->AResult<()>;
    async fn disconnect(&self)->AResult<bool>;

}

#[aqueue::aqueue_trait]
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
    async fn set_controller_fun_maps(&self, map: HashMap<i32, Box<dyn FunctionInfo>>) -> AResult<()> {
        self.inner_call(async move|inner|{
            inner.get_mut().controller_fun_register_dict=Some(map);
            Ok(())
        }).await
    }

    #[inline]
    async fn clear_controller_fun_maps(&self) -> AResult<()> {
        self.inner_call(async move|inner|{
            inner.get_mut().controller_fun_register_dict=None;
            Ok(())
        }).await
    }

    #[inline]
    async fn set_peer(&self, peer: Option<Weak<Actor<TCPPeer>>>) -> AResult<()> {
        self.inner_call(async move|inner|{
            inner.get_mut().peer=peer;
            Ok(())
        }).await
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
    async fn send<T: Deref<Target=[u8]> + Send + Sync + 'static>(&self, buff: T) -> AResult<usize> {
        unsafe {
            if let Some(ref peer)= self.deref_inner().peer{
               if let Some(peer)= peer.upgrade(){
                   return peer.send(buff).await
               }
            }
            let err=format!("token:{} tcp disconnect",self.get_sessionid());
            Err(AError::StrErr(err))
        }
    }

    #[inline]
    async fn get_token(&self, sessionid: i64) -> AResult<Option<NetxToken>> {
        self.inner_call(async move|inner|{
            if let Some(manager)= inner.get().manager.upgrade(){
                manager.get_token(sessionid).await
            }
            else {
                Err(AError::StrErr("manager upgrade fail".into()))
            }
        }).await
    }

    #[inline]
    async fn get_all_tokens(&self) -> AResult<Vec<NetxToken>> {
        self.inner_call(async move|inner|{
            if let Some(manager)= inner.get().manager.upgrade(){
                manager.get_all_tokens().await
            }
            else {
                Err(AError::StrErr("manager upgrade fail".into()))
            }
        }).await
    }

    #[inline]
    async fn call(&self, serial: i64, buff: Data) -> Result<RetResult, Box<dyn Error>> {
        let (net,rx):(Arc<Actor<TCPPeer>>,Receiver<AResult<Data>>)=self.inner_call(async move|inner|{
            if let Some(ref net)=inner.get().peer{
                if let Some(peer)= net.upgrade() {
                    let (tx, rx): (Sender<AResult<Data>>, Receiver<AResult<Data>>) = oneshot();
                    if inner.get_mut().result_dict.contains_key(&serial) {
                        return Err(AError::StrErr("serial is have".into()))
                    }
                    if let None= inner.get_mut().result_dict.insert(serial, tx) {
                        inner.get_mut().request_queue.push_front((serial, Instant::now()));
                    }

                    Ok((peer, rx))
                }else{
                    Err(AError::StrErr("call peer is null".into()))
                }
            }else{
                Err(AError::StrErr("call not connect".into()))
            }
        }).await?;

        let len=buff.len()+4;
        let mut data=Data::with_capacity(len);
        data.write_to_le(&(len as u32));
        data.write(&buff);
        net.send(data).await?;
        match rx.await {
            Err(_)=>{
                Err("tx is Close".into())
            },
            Ok(data)=>{
                Ok(RetResult::from(data?)?)
            }
        }
    }

    #[inline]
    async fn run(&self, buff: Data) -> Result<(), Box<dyn Error>> {
        let net:Arc<Actor<TCPPeer>>= self.inner_call(async move|inner|{
            if let Some(ref net)=inner.get().peer{
                if let Some(peer)= net.upgrade() {
                    Ok(peer)
                }
                else{
                    Err(AError::StrErr("run peer is null".into()))
                }
            }else{
                Err(AError::StrErr("run not connect".into()))
            }
        }).await?;

        let len=buff.len()+4;
        let mut data=Data::with_capacity(len);
        data.write_to_le(&(len as u32));
        data.write(&buff);
        net.send(data).await?;
        Ok(())
    }
    #[inline]
    async fn set_result(&self, serial: i64, data: Data) -> AResult<()> {
        self.inner_call(async move|inner|{
            match inner.get_mut().set_result(serial,data){
                Err(er)=>{
                    Err(AError::Other(er))
                },
                Ok(_)=>{
                    Ok(())
                }
            }

        }).await
    }

    #[inline]
    async fn set_error(&self, serial: i64, err: AError) -> AResult<()> {
        self.inner_call(async move|inner|{
            match inner.get_mut().set_error(serial,err){
                Err(er)=>{
                    Err(AError::Other(er))
                },
                Ok(_)=>{
                    Ok(())
                }
            }
        }).await
    }

    #[inline]
    async fn check_request_timeout(&self, request_out_time: u32) -> AResult<()> {
        self.inner_call(async move|inner|{
            inner.get_mut().check_request_timeout(request_out_time);
            Ok(())
        }).await
    }
    #[inline]
    async fn disconnect(&self) -> AResult<bool> {
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
            data.write_to_le(&2400u32);
            data.write_to_le(&2u8);
            data.write_to_le(&$cmd);
            data.write_to_le(&serial);
            data.write_to_le(&args_count);
            $(data.serde_serialize(&$args)?;)*
            let ret= $peer.call(serial,data).await?;
            let mut ret= ret.check()?;
            ret.deserialize()?
    });
    (@result $peer:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call_peer!(@count $($args),*) as i32;
            let serial=$peer.new_serial();
            data.write_to_le(&2400u32);
            data.write_to_le(&2u8);
            data.write_to_le(&$cmd);
            data.write_to_le(&serial);
            data.write_to_le(&args_count);
            $(data.serde_serialize(&$args)?;)*
            $peer.call(serial,data).await?
    });
    (@run $peer:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call_peer!(@count $($args),*) as i32;
            let serial=$peer.new_serial();
            data.write_to_le(&2400u32);
            data.write_to_le(&0u8);
            data.write_to_le(&$cmd);
            data.write_to_le(&serial);
            data.write_to_le(&args_count);
            $(data.serde_serialize(&$args)?;)*
            $peer.run(data).await?;
    });
     (@run_not_err $peer:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call_peer!(@count $($args),*) as i32;
            let serial=$peer.new_serial();
            data.write_to_le(&2400u32);
            data.write_to_le(&0u8);
            data.write_to_le(&$cmd);
            data.write_to_le(&serial);
            data.write_to_le(&args_count);
            $(data.serde_serialize(&$args)?;)*
            if let Err(err)= $peer.run(data).await{
                 log::error!{"run {} is error:{}",$cmd,err}
            }
    });
    (@checkrun $peer:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call_peer!(@count $($args),*) as i32;
            let serial=$peer.new_serial();
            data.write_to_le(&2400u32);
            data.write_to_le(&1u8);
            data.write_to_le(&$cmd);
            data.write_to_le(&serial);
            data.write_to_le(&args_count);
            $(data.serde_serialize(&$args)?;)*
            let ret=$peer.call(serial,data).await?;
            ret.check()?;
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