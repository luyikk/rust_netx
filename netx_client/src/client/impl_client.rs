use std::sync::atomic::{AtomicI64, Ordering};
use std::collections::HashMap;
use std::sync::Arc;
use aqueue::Actor;
use tcpclient::{TcpClient, SocketClientTrait};
use tokio::net::tcp::OwnedReadHalf;
use tokio::io::AsyncReadExt;
use tokio::time::{Duration, sleep};
use tokio::sync::watch::{Sender as WSender,Receiver as WReceiver,channel};
use async_oneshot::{oneshot, Receiver, Sender};
use serde::{Serialize,Deserialize};
use data_rw::Data;
use bytes::Buf;
use log::*;
use anyhow::*;

use crate::client::result::RetResult;
use crate::client::request_manager::{RequestManager,IRequestManager};
use crate::client::controller::{FunctionInfo, IController};


pub trait SessionSave{
    fn get_sessionid(&self)->i64;
    fn store_sessionid(&mut self,sessionid:i64);
}

enum SpecialFunctionTag{
    Connect =2147483647,
    Disconnect =2147483646
}

pub struct NetXClient<T>{
    session:T,
    serverinfo: ServerOption,
    net:Option<Arc<Actor<TcpClient>>>,
    connect_stats:Option<WReceiver<(bool,String)>>,
    result_dict:HashMap<i64,Sender<Result<Data>>>,
    serial_atomic:AtomicI64,
    request_manager:Option<Arc<Actor<RequestManager<T>>>>,
    controller_fun_register_dict:HashMap<i32,Box<dyn FunctionInfo>>,
    mode:u8
}

unsafe impl <T> Send for NetXClient<T>{}
unsafe impl <T> Sync for NetXClient<T>{}

impl<T> Drop for NetXClient<T>{
    fn drop(&mut self) {
        debug!("{} is drop",self.serverinfo)
    }
}

#[derive(Clone,Deserialize,Serialize)]
pub struct ServerOption {
    addr:String,
    service_name:String,
    verify_key:String,
    request_out_time_ms:u32
}

impl std::fmt::Display for ServerOption {
    fn fmt(&self, f: &mut  std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f,"{}[{}]",self.service_name,self.addr)
    }
}

impl ServerOption {
    pub fn new(addr:String,service_name:String,verify_key:String,request_out_time_ms:u32)-> ServerOption {
        ServerOption {
            addr,
            service_name,
            verify_key,
            request_out_time_ms
        }
    }
}

impl<T:SessionSave+'static> NetXClient<T>{
    pub async fn new(serverinfo: ServerOption, session:T) ->Result<Arc<Actor<NetXClient<T>>>>{
        let request_out_time_ms=serverinfo.request_out_time_ms;
        let netx_client=Arc::new(Actor::new(NetXClient{
            session,
            serverinfo,
            net:None,
            result_dict:HashMap::new(),
            connect_stats:None,
            serial_atomic:AtomicI64::new(1),
            request_manager:None,
            controller_fun_register_dict:HashMap::new(),
            mode:0
        }));

        let request_manager=RequestManager::new(request_out_time_ms,Arc::downgrade(&netx_client));
        netx_client.set_request_manager(request_manager).await?;
        Ok(netx_client)
    }

    #[inline]
    pub fn init<C:IController+Sync+Send+'static>(&mut self,controller:C){
        self.controller_fun_register_dict=
            IController::register(Arc::new(controller)).expect("init error");
    }

    #[allow(clippy::type_complexity)]
    async fn input_buffer((mut netx_client,set_connect):(Arc<Actor<NetXClient<T>>>, WSender<(bool, String)>), client:Arc<Actor<TcpClient>>, mut reader:OwnedReadHalf) ->Result<bool>{
        if let Err(er)=Self::read_buffer(&mut netx_client, set_connect, client, &mut reader).await{
            error!("read buffer err:{}",er);
        }
        netx_client.call_special_function(SpecialFunctionTag::Disconnect as i32).await?;
        netx_client.close().await?;
        info! {"disconnect to {}", netx_client.get_serviceinfo()};
        Ok(true)
    }

    async fn read_buffer(netx_client: &mut Arc<Actor<NetXClient<T>>>, set_connect: WSender<(bool, String)>, client: Arc<Actor<TcpClient>>, reader: &mut OwnedReadHalf)->Result<()> {
        let serverinfo = netx_client.get_serviceinfo();
        let mut sessionid = netx_client.get_sessionid();
        client.send(Self::get_verify_buff(&serverinfo.service_name, &serverinfo.verify_key, &sessionid)).await?;
        let mut option_connect = Some(set_connect);
        while let Ok(len) = reader.read_u32_le().await {
            let len = (len - 4) as usize;
            let mut data = Data::with_len(len, 0);
            reader.read_exact(&mut data).await?;
            let cmd = data.get_le::<i32>()?;
            match cmd {
                1000 => {
                    match data.get_le::<bool>()? {
                        false => {
                            info!("{} {}", serverinfo, data.get_le::<String>()?);
                            if data.have_len() == 1 && data.get_u8() == 1 {
                                netx_client.set_mode(1).await?;
                            }
                            client.send(Self::get_sessionid_buff(netx_client.get_mode())).await?;
                            netx_client.call_special_function(SpecialFunctionTag::Connect as i32).await?;
                            if let Some(set_connect) = option_connect.take() {
                                if set_connect.send((true, "success".into())).is_err() {
                                    error!("talk connect rx is close");
                                }
                                drop(set_connect);
                            }
                        },
                        true => {
                            let err = data.get_le::<String>()?;
                            error!("connect {} error:{}", serverinfo, err);
                            if let Some(set_connect) = option_connect.take() {
                                if set_connect.send((false, err)).is_err() {
                                    error!("talk connect rx is close");
                                }
                                drop(set_connect);
                            }
                            break;
                        }
                    }
                },
                2000 => {
                    sessionid = data.get_le::<i64>()?;
                    info!("{} save sessionid:{}", serverinfo, sessionid);
                    netx_client.store_sessionid(sessionid).await?;
                },
                2400 => {
                    let tt = data.get_le::<u8>()?;
                    let cmd = data.get_le::<i32>()?;
                    let sessionid = data.get_le::<i64>()?;
                    match tt {
                        0 => {
                            let run_netx_client = netx_client.clone();
                            tokio::spawn(async move {
                                let _ = run_netx_client.call_controller(tt, cmd, data).await;
                            });
                        },
                        1 => {
                            let run_netx_client = netx_client.clone();
                            let send_client = client.clone();
                            tokio::spawn(async move {
                                let res = run_netx_client.call_controller(tt, cmd, data).await;
                                if let Err(er) = send_client.send(Self::get_result_buff(sessionid, res, run_netx_client.get_mode())).await {
                                    error!("send buff 1 error:{}", er);
                                }
                            });
                        },
                        2 => {
                            let run_netx_client = netx_client.clone();
                            let send_client = client.clone();
                            tokio::spawn(async move {
                                let res = run_netx_client.call_controller(tt, cmd, data).await;
                                if let Err(er) = send_client.send(Self::get_result_buff(sessionid, res, run_netx_client.get_mode())).await {
                                    error!("send buff 2 error:{}", er);
                                }
                            });
                        },
                        _ => {
                            panic!("not found call type:{}", tt);
                        }
                    }
                },
                2500 => {
                    let serial = data.get_le::<i64>()?;
                    netx_client.set_result(serial, data).await?;
                },
                _ => {
                    error!("{} Unknown command:{}->{:?}", serverinfo, cmd, data);
                    break;
                }
            }
        }

        Ok(())
    }

    #[inline]
    pub(crate) async fn call_special_function(&self,cmd:i32)->Result<()> {
        if let Some(func)= self.controller_fun_register_dict.get(&cmd) {
            func.call(Data::with_len(4, 0)).await?;
        }
        Ok(())
    }

    #[inline]
    pub (crate)  async fn run_controller(&self, tt:u8,cmd:i32,data:Data)->Result<RetResult>{
        return if let Some(func) = self.controller_fun_register_dict.get(&cmd) {
            if func.function_type() != tt {
                bail!(" cmd:{} function type error:{}", cmd, tt)
            } else {
                func.call(data).await
            }
        } else {
            bail!("not found cmd:{}", cmd)
        }
    }


    #[inline]
    fn get_verify_buff(service_name:&str,verify_key:&str,sessionid:&i64)->Data{
        let mut data=Data::with_capacity(128);
        data.write_to_le(&1000);
        data.write_to_le(&service_name);
        data.write_to_le(&verify_key);
        data.write_to_le(sessionid);
        data
    }

    fn get_sessionid_buff(mode:u8)->Data{
        let mut buff=Data::with_capacity(32);
        buff.write_to_le(&2000);
        if mode==0{
            buff
        }else {
            let len = buff.len() + 4;
            let mut data = Data::with_capacity(len);
            data.write_to_le(&(len as u32));
            data.write(&buff);
            data
        }
    }


    #[inline]
    fn get_result_buff(sessionid:i64,result:RetResult,mode:u8)->Data{
        let mut data=Data::with_capacity(1024);
        data.write_to_le(&2500u32);
        data.write_to_le(&sessionid);
        if result.is_error{
            data.write_to_le(&true);
            data.write_to_le(&result.error_id);
            data.write_to_le(&result.msg);
        }
        else{
            data.write_to_le(&false);
            data.write_to_le(&(result.arguments.len() as u32));
            for argument in result.arguments {
                data.write_to_le(&argument.bytes());
            }
        }

        if mode==0{
            data
        }
        else{
            let len=data.len()+4usize;
            let mut buff=Data::with_capacity(len);
            buff.write_to_le(&(len as u32));
            buff.write(&data);
            buff
        }

    }

    #[inline]
    pub fn set_mode(&mut self,mode:u8){
        self.mode=mode
    }

    #[inline]
    pub fn get_mode(&self)->u8{
        self.mode
    }

    #[inline]
    pub fn get_addr_string(&self)->String{
        self.serverinfo.addr.clone()
    }

    #[inline]
    pub fn get_service_info(&self)-> ServerOption {
        self.serverinfo.clone()
    }

    #[inline]
    pub fn get_sessionid(&self)->i64{
        self.session.get_sessionid()
    }

    #[inline]
    pub fn store_sessionid(&mut self,sessionid:i64){
        self.session.store_sessionid(sessionid)
    }

    #[inline]
    pub fn set_network_client(&mut self, client:Arc<Actor<TcpClient>>){
        self.net=Some(client);
    }

    #[inline]
    pub fn set_connect_stats(&mut self,stats:Option<WReceiver<(bool,String)>>){
        self.connect_stats=stats;
    }

    #[inline]
    pub fn is_connect(&self)->bool{
        self.net.is_some()
    }

    #[inline]
    pub fn new_serial(&self)->i64{
        self.serial_atomic.fetch_add(1,Ordering::Acquire)
    }


    #[inline]
    pub fn get_callback_len(&mut self,) ->usize{
        self.result_dict.len()
    }

    #[inline]
    pub fn set_request_manager(&mut self, request:Arc<Actor<RequestManager<T>>>){
        self.request_manager=Some(request);
    }

    #[inline]
    pub (crate) async fn set_request_sessionid(&self,sessionid:i64)->Result<()>{
        if let Some(ref request)=self.request_manager{
            return request.set(sessionid).await
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
#[async_trait::async_trait]
pub trait INetXClient<T>{
    async fn init<C:IController+Sync+Send+'static>(&self,controller:C)->Result<()>;
    async fn connect_network(self:&Arc<Self>)->Result<()>;
    fn get_address(&self)->String;
    fn get_serviceinfo(&self)-> ServerOption;
    fn get_sessionid(&self)->i64;
    fn get_mode(&self)->u8;
    fn new_serial(&self)->i64;
    fn is_connect(&self)->bool;
    async fn get_peer(&self)->Result<Option<Arc<Actor<TcpClient>>>>;
    async fn store_sessionid(&self,sessionid:i64)->Result<()>;
    async fn set_mode(&self,mode:u8)->Result<()>;
    async fn set_network_client(&self,client:Arc<Actor<TcpClient>>)->Result<()>;
    async fn reset_connect_stats(&self)->Result<()>;
    async fn get_callback_len(&self) -> Result<usize> ;
    async fn set_result(&self,serial:i64,data:Data)->Result<()>;
    async fn set_error(&self,serial:i64,err:anyhow::Error)->Result<()>;
    async fn set_request_manager(&self,request:Arc<Actor<RequestManager<T>>>)->Result<()>;
    async fn call_special_function(&self, cmd: i32) -> Result<()>;
    async fn call_controller(&self, tt:u8,cmd:i32,data:Data)->RetResult;
    async fn close(&self)-> Result<()>;
    async fn call(&self, serial:i64, buff:Data) ->Result<RetResult>;
    async fn run(&self, buff:Data)-> Result<()>;
}


#[allow(clippy::too_many_arguments)]
#[async_trait::async_trait]
impl<T:SessionSave+'static> INetXClient<T> for Actor<NetXClient<T>>{
    #[inline]
    async fn init<C:IController+Sync+Send+'static>(&self, controller: C) -> Result<()> {
        self.inner_call(async move|inner|{
            inner.get_mut().init(controller);
            Ok(())
        }).await
    }

    #[inline]
    async fn connect_network(self: &Arc<Self>) -> Result<()> {
        let netx_client = self.clone();
        let mut wait_handler: WReceiver<(bool, String)> = self.inner_call(async move |inner| {
            if inner.get().is_connect() {
                return match inner.get().connect_stats {
                    Some(ref stats) => {
                        Ok(stats.clone())
                    },
                    None => {
                        bail!("inner is connect,but not get stats")
                    }
                }
            }

            let (set_connect, wait_connect) = channel((false, "not connect".to_string()));
            let client=TcpClient::connect(netx_client.get_address(), NetXClient::input_buffer, (netx_client.clone(), set_connect)).await?;
            let ref_inner = inner.get_mut();
            ref_inner.set_network_client(client);
            ref_inner.connect_stats = Some(wait_connect.clone());
            Ok(wait_connect)

        }).await?;

        match wait_handler.changed().await {
            Err(err) => {
                self.reset_connect_stats().await?;
                bail!("connect err:{}", err)
            },
            Ok(_) => {
                self.reset_connect_stats().await?;
                let (is_connect, msg) = &(*wait_handler.borrow());
                if !is_connect {
                    bail!("connect err:{}",msg);
                }
            }
        }

        Ok(())
    }

    #[inline]
    fn get_address(&self) -> String {
         unsafe {
             self.deref_inner().get_addr_string()
         }
    }

    #[inline]
    fn get_serviceinfo(&self) -> ServerOption {
        unsafe {
            self.deref_inner().get_service_info()
        }
    }

    #[inline]
    fn get_sessionid(&self) -> i64 {
        unsafe {
            self.deref_inner().get_sessionid()
        }
    }

    #[inline]
    fn get_mode(&self) -> u8 {
        unsafe {
            self.deref_inner().get_mode()
        }
    }

    #[inline]
    fn new_serial(&self) -> i64 {
        unsafe {
            self.deref_inner().new_serial()
        }
    }

    #[inline]
    fn is_connect(&self) -> bool {
        unsafe {
            self.deref_inner().is_connect()
        }
    }

    #[inline]
    async fn get_peer(&self) -> Result<Option<Arc<Actor<TcpClient>>>> {
        self.inner_call(async move|inner|{
            Ok(inner.get().net.clone())
        }).await
    }

    #[inline]
    async fn store_sessionid(&self, sessionid: i64) -> Result<()> {
        self.inner_call(async move|inner|{
            inner.get_mut().store_sessionid(sessionid);
            Ok(())
        }).await
    }

    #[inline]
    async fn set_mode(&self, mode: u8) -> Result<()> {
        self.inner_call(async move|inner|{
            inner.get_mut().set_mode(mode);
            Ok(())
        }).await
    }


    #[inline]
    async fn set_network_client(&self, client: Arc<Actor<TcpClient>>) -> Result<()> {
        self.inner_call(async move|inner|{
            inner.get_mut().set_network_client(client);
            Ok(())
        }).await
    }

    #[inline]
    async fn reset_connect_stats(&self) -> Result<()> {
        self.inner_call(async move|inner|{
            inner.get_mut().set_connect_stats(None);
            Ok(())
        }).await
    }

    #[inline]
    async fn get_callback_len(&self) -> Result<usize> {
        self.inner_call(async move|inner|{
            Ok(inner.get_mut().get_callback_len())
        }).await
    }


    #[inline]
    async fn set_result(&self, serial: i64, data: Data) -> Result<()> {

       let have_tx:Option<Sender<Result<Data>>>= self.inner_call(async move|inner|{
            Ok(inner.get_mut().result_dict.remove(&serial))
        }).await?;

        if let Some(tx)=have_tx{
            tx.send(Ok(data)).map_err(|_|anyhow!("rx is close"))?;
        } else{
            match RetResult::from(data){
                Ok(res)=>{
                    match res.check(){
                        Ok(_)=> error!("not found 2 {}",serial),
                        Err(err)=> error!("{}",err)
                    }
                },
                Err(er)=> error!("not found {} :{}",serial,er)
            }
        }
        Ok(())
    }
    #[inline]
    async fn set_error(&self, serial: i64, err: anyhow::Error) -> Result<()> {
        let have_tx:Option<Sender<Result<Data>>>= self.inner_call(async move|inner|{
            Ok(inner.get_mut().result_dict.remove(&serial))
        }).await?;
        if let Some(tx)=have_tx{
            tx.send(Err(err)).map_err(|_|anyhow!("rx is close"))?;
            Ok(())
        } else{
            Ok(())
        }
    }
    #[inline]
    async fn set_request_manager(&self, request: Arc<Actor<RequestManager<T>>>) -> Result<()> {
        self.inner_call(async move|inner|{
            inner.get_mut().set_request_manager(request);
            Ok(())
        }).await
    }

    #[inline]
    async fn call_special_function(&self, cmd: i32) -> Result<()> {
        unsafe{
            self.deref_inner().call_special_function(cmd).await
        }
    }

    #[inline]
    async fn call_controller(&self, tt:u8, cmd: i32, data: Data) ->RetResult {
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
    async fn close(&self) -> Result<()> {
        let net:Result<Arc<Actor<TcpClient>>>= self.inner_call(async move|inner|{
            inner.get_mut().controller_fun_register_dict.clear();
            inner.get_mut().net.take().context("not connect")
        }).await;
        match net{
            Err(_)=>Ok(()),
            Ok(net)=>{
                net.disconnect().await?;
                sleep(Duration::from_millis(100)).await;
                Ok(())
            }
        }
    }

    #[inline]
    async fn call(&self,serial:i64,buff: Data) -> Result<RetResult> {
        let (net,rx):(Arc<Actor<TcpClient>>,Receiver<Result<Data>>)=self.inner_call(async move|inner|{
            if let Some(ref net)=inner.get().net{
                let (tx,rx):(Sender<Result<Data>>,Receiver<Result<Data>>)=oneshot();
                if inner.get_mut().result_dict.contains_key(&serial){
                    bail!("serial is have")
                }
                inner.get_mut().result_dict.insert(serial,tx);
                Ok((net.clone(),rx))
            }else{
                bail!("not connect")
            }
        }).await?;
        unsafe {
            self.deref_inner().set_request_sessionid(serial).await?;
        }
        if self.get_mode()==0 {
            net.send(buff).await?;
        }
        else{
            let len=buff.len()+4;
            let mut data=Data::with_capacity(len);
            data.write_to_le(&(len as u32));
            data.write(&buff);
            net.send(data).await?;
        }
        match rx.await {
            Err(_)=>{
                bail!("tx is Close")
            },
            Ok(data)=>{
                Ok(RetResult::from(data?)?)
            }
        }
    }

    #[inline]
    async fn run(&self, buff: Data) -> Result<()> {
        let net= self.inner_call(async move|inner|{
            if let Some(ref net)=inner.get().net{
                Ok(net.clone())
            }else{
                bail!("not connect")
            }
        }).await?;
        if self.get_mode()==0 {
            net.send(buff).await?;
        }
        else{
            let len=buff.len()+4;
            let mut data=Data::with_capacity(len);
            data.write_to_le(&(len as u32));
            data.write(&buff);
            net.send(data).await?;
        }

        Ok(())
    }

}

#[macro_export]
macro_rules! call {
    (@uint $($x:tt)*)=>(());
    (@count $($rest:expr),*)=>(<[()]>::len(&[$(call!(@uint $rest)),*]));
    ($client:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            if $client.is_connect() ==false{
                $client.connect_network().await?;
            }
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call!(@count $($args),*) as i32;
            let serial=$client.new_serial();
            data.write_to_le(&2400u32);
            data.write_to_le(&2u8);
            data.write_to_le(&$cmd);
            data.write_to_le(&serial);
            data.write_to_le(&args_count);
            $(data.msgpack_serialize($args)?;)*
            let mut ret= $client.call(serial,data).await?.check()?;
            ret.deserialize()?
    });
    (@result $client:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            if $client.is_connect() ==false{
               $client.connect_network().await?;
            }
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call!(@count $($args),*) as i32;
            let serial=$client.new_serial();
            data.write_to_le(&2400u32);
            data.write_to_le(&2u8);
            data.write_to_le(&$cmd);
            data.write_to_le(&serial);
            data.write_to_le(&args_count);
            $(data.msgpack_serialize($args)?;)*
            $client.call(serial,data).await?
    });
    (@run $client:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            if $client.is_connect() ==false{
                $client.connect_network().await?;
            }
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call!(@count $($args),*) as i32;
            let serial=$client.new_serial();
            data.write_to_le(&2400u32);
            data.write_to_le(&0u8);
            data.write_to_le(&$cmd);
            data.write_to_le(&serial);
            data.write_to_le(&args_count);
            $(data.msgpack_serialize($args)?;)*
            $client.run(data).await?;
    });
     (@run_not_err $client:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            if $client.is_connect() ==false{
               if let Err(err)= $client.connect_network().await{
                    log::error!{"run {} is error:{}",$cmd,err}
               }
            }
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call!(@count $($args),*) as i32;
            let serial=$client.new_serial();
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
            if let Err(err)= $client.run(data).await{
                 log::error!{"run {} is error:{}",$cmd,err}
            }
    });
    (@checkrun $client:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            if $client.is_connect() ==false{
                $client.connect_network().await?;
            }
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call!(@count $($args),*) as i32;
            let serial=$client.new_serial();
            data.write_to_le(&2400u32);
            data.write_to_le(&1u8);
            data.write_to_le(&$cmd);
            data.write_to_le(&serial);
            data.write_to_le(&args_count);
            $(data.msgpack_serialize($args)?;)*
            $client.call(serial,data).await?.check()?;

    });

}

#[macro_export]
macro_rules! impl_interface {
    ($client:expr=>$interface:ty) => (
      paste::paste!{
            Box::new([<___impl_ $interface _call>]::new($client.clone()))  as  Box<dyn $interface>
      }
    )
}