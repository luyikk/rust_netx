use std::sync::atomic::{AtomicI64, Ordering};
use aqueue::{Actor, AResult};
use std::collections::HashMap;
use crate::controller::FunctionInfo;
use std::collections::hash_map::RandomState;
use std::sync::{Arc, Weak};
use tcpserver::{TCPPeer, IPeer};
use data_rw::Data;
use crate::RetResult;
use std::error::Error;
use log::*;
use std::ops::Deref;


pub struct AsyncToken{
    sessionid:i64,
    controller_fun_register_dict:Option<HashMap<i32,Box<dyn FunctionInfo>>>,
    peer:Option<Weak<Actor<TCPPeer>>>
}

unsafe impl Send for AsyncToken{}
unsafe impl Sync for AsyncToken{}

pub type NetxToken=Arc<Actor<AsyncToken>>;

impl AsyncToken{
    pub(crate) fn new(sessionid:i64)->AsyncToken{
        AsyncToken{
            sessionid,
            controller_fun_register_dict:None,
            peer:None
        }
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
}

#[aqueue::aqueue_trait]
pub trait IAsyncToken{
    fn get_sessionid(&self)->i64;
    async fn set_controller_fun_maps(&self,map:HashMap<i32,Box<dyn FunctionInfo>>)->AResult<()>;
    async fn set_peer(&self,peer:Option<Weak<Actor<TCPPeer>>>)->AResult<()>;
    async fn run_controller(&self, tt:u8,cmd:i32,data:Data)->RetResult;
    async fn send<T: Deref<Target=[u8]> + Send + Sync + 'static>(&self, buff: T) -> AResult<usize>;
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
    async fn set_controller_fun_maps(&self, map: HashMap<i32, Box<dyn FunctionInfo>>) -> AResult<()> {
        self.inner_call(async move|inner|{
            inner.get_mut().controller_fun_register_dict=Some(map);
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
                    error!("call controller error:{}",err);
                    RetResult::error(1,format!("call controller err:{}",err))
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
            debug!(" token:{} tcp disconnect",self.get_sessionid());
            Ok(0)
        }
    }
}

