use aqueue::{Actor, AResult, AError};
use tcpclient::{TcpClient, SocketClientTrait};
use std::sync::Arc;
use std::error::Error;
use tokio::net::tcp::OwnedReadHalf;
use tokio::io::AsyncReadExt;
use data_rw::{Data, Writer};
use log::*;
use async_oneshot::{oneshot, Receiver, Sender};
use std::sync::atomic::{AtomicI64, Ordering};
use crate::client::result::RetResult;
use tokio::time::{Duration, sleep};
use crate::client::request_manager::{RequestManager,IRequestManager};
use std::collections::HashMap;
use serde::{Serialize,Deserialize};


use bytes::Buf;
use crate::client::controller::{FunctionInfo, IController};

pub trait SessionSave{
    fn get_sessionid(&self)->i64;
    fn store_sessionid(&mut self,sessionid:i64);
}

pub struct NetXClient<T>{
    session:T,
    serverinfo:ServerInfo,
    net:Option<Arc<Actor<TcpClient>>>,
    result_dict:HashMap<i64,Sender<AResult<Data>>>,
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
pub struct ServerInfo{
    addr:String,
    service_name:String,
    verify_key:String
}

impl std::fmt::Display for ServerInfo{
    fn fmt(&self, f: &mut  std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f,"{}[{}]",self.service_name,self.addr)
    }
}


impl ServerInfo{
    pub fn new(addr:String,service_name:String,verify_key:String)->ServerInfo{
        ServerInfo{
            addr,
            service_name,
            verify_key,
        }
    }
}

impl<T:SessionSave+'static> NetXClient<T>{
    pub async fn new(serverinfo:ServerInfo, session:T, request_out_time_ms:u32) ->Result<Arc<Actor<NetXClient<T>>>,Box<dyn Error>>{
        let netx_client=Arc::new(Actor::new(NetXClient{
            session,
            serverinfo,
            net:None,
            result_dict:HashMap::new(),
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
        let controller=Arc::new(controller);
        let table=controller.register().expect("init error");
        self.controller_fun_register_dict=table;
    }


    #[inline]
    pub async fn connect_network(netx_client:Arc<Actor<NetXClient<T>>>)->Result<(),Box<dyn Error>> {
        let (set_connect,wait_connect)=oneshot();
        let client= TcpClient::connect(netx_client.get_address(), Self::input_buffer, (netx_client.clone(),set_connect)).await?;
        netx_client.set_network_client(client).await?;
        match wait_connect.await{
            Err(_)=>{
                return Err("talk connect tx is close".into());
            },
            Ok((is_connect,msg))=>{
                if !is_connect{
                    return Err(msg.into());
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::type_complexity)]
    async fn input_buffer((netx_client,set_connect):(Arc<Actor<NetXClient<T>>>,Sender<(bool,String)>), client:Arc<Actor<TcpClient>>,mut reader:OwnedReadHalf)->Result<bool,Box<dyn Error>>{
        let serverinfo=netx_client.get_serviceinfo();
        let mut sessionid=netx_client.get_sessionid();
        client.send(Self::get_verify_buff(&serverinfo.service_name,&serverinfo.verify_key,&sessionid)).await?;
        let mut option_connect=Some(set_connect);
        while let Ok(len)=reader.read_u32_le().await {
            let len=(len-4) as usize;
            let mut data=Data::with_len(len,0);
            reader.read_exact(&mut data).await?;
            let cmd=data.get_le::<i32>()?;
            match cmd {
                1000=>{
                    match data.get_le::<bool>()? {
                        false=>{
                            info!("{} {}",serverinfo,data.get_le::<String>()?);
                        },
                        true=>{
                            let err= data.get_le::<String>()?;
                            error!("connect {} error:{}",serverinfo,err);
                            if let Some(set_connect)=option_connect.take(){
                                if  set_connect.send((false,err)).is_err(){
                                    error!("talk connect rx is close");
                                }
                            }
                            break;
                        }
                    }
                },
                2000=>{
                    sessionid=data.get_le::<i64>()?;
                    info!("{} save sessionid:{}",serverinfo, sessionid);
                    netx_client.store_sessionid(sessionid).await?;
                    if data.have_len() ==1 {
                       if data.get_u8() ==1{
                           netx_client.set_mode(1).await?;
                       }
                    }
                    if let Some(set_connect)=option_connect.take(){
                        if  set_connect.send((true,"success".into())).is_err(){
                            error!("talk connect rx is close");
                        }
                    }
                },
                2400=>{
                    let tt=data.get_le::<u8>()?;
                    let cmd=data.get_le::<i32>()?;
                    let sessionid=data.get_le::<i64>()?;

                    match tt{
                        0=>{
                            let run_netx_client=netx_client.clone();
                            tokio::spawn(async move {
                                let _ = run_netx_client.call_controller(tt, cmd, data).await;
                            });
                        },
                        1=>{
                            let run_netx_client=netx_client.clone();
                            let send_client=client.clone();
                            tokio::spawn(async move{
                                let res= run_netx_client.call_controller(tt,cmd,data).await;
                                if let Err(er)= send_client.send(Self::get_result_buff(sessionid,res,run_netx_client.get_mode())).await{
                                    error!("send buff 1 error:{}",er);
                                }
                            });
                        },
                        2=>{
                            let run_netx_client=netx_client.clone();
                            let send_client=client.clone();
                            tokio::spawn(async move{
                                let res= run_netx_client.call_controller(tt,cmd,data).await;
                                if let Err(er)=  send_client.send(Self::get_result_buff(sessionid,res,run_netx_client.get_mode())).await{
                                    error!("send buff 2 error:{}",er);
                                }
                            });
                        },
                        _=>{
                            error!("not found call type:{}",tt);
                            panic!("not found call type:{}",tt);
                        }
                    }
                },
                2500=>{
                    let serial=data.get_le::<i64>()?;
                    netx_client.set_result(serial,data).await?;
                }
                _=>{
                    error!("{} Unknown command:{}->{:?}",serverinfo, cmd,data);
                    break;
                }
            }
        }

        netx_client.close().await?;
        info!{"disconnect to {}",serverinfo};
        Ok(true)
    }

    #[inline]
    pub (crate)  async fn run_controller(&self, tt:u8,cmd:i32,data:Data)->Result<RetResult,Box<dyn Error>>{
        if let Some(func)= self.controller_fun_register_dict.get(&cmd){
            if func.function_type() !=tt{
                return Err(format!(" cmd:{} function type error:{}",cmd,tt).into());
            }
            func.call(data).await
        }
        else{
            return Err(format!("not found cmd:{}",cmd).into());
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
    pub fn get_service_info(&self)->ServerInfo{
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
    pub fn is_connect(&self)->bool{
        self.net.is_some()
    }

    #[inline]
    pub fn new_serial(&self)->i64{
        self.serial_atomic.fetch_add(1,Ordering::Acquire)
    }

    #[inline]
    pub fn set_result(&mut self,serial:i64,data:Data)->Result<(),Box<dyn Error+ Send + Sync>>{
        println!("set result {}",serial);
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
        println!("set error {}",serial);
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
    pub fn set_request_manager(&mut self, request:Arc<Actor<RequestManager<T>>>){
        self.request_manager=Some(request);
    }

    #[inline]
    pub (crate) async fn set_request_sessionid(&self,sessionid:i64)->AResult<()>{
        if let Some(ref request)=self.request_manager{
            return request.set(sessionid).await
        }
        Ok(())
    }


}

#[allow(clippy::too_many_arguments)]
#[aqueue::aqueue_trait]
pub trait INetXClient<T>{
    async fn init<C:IController+Sync+Send+'static>(&self,controller:C)->AResult<()>;
    fn get_address(&self)->String;
    fn get_serviceinfo(&self)->ServerInfo;
    fn get_sessionid(&self)->i64;
    fn get_mode(&self)->u8;
    fn new_serial(&self)->i64;
    fn is_connect(&self)->bool;
    async fn store_sessionid(&self,sessionid:i64)->AResult<()>;
    async fn set_mode(&self,mode:u8)->AResult<()>;
    async fn set_network_client(&self,client:Arc<Actor<TcpClient>>)->AResult<()>;
    async fn set_result(&self,serial:i64,data:Data)->AResult<()>;
    async fn set_error(&self,serial:i64,err:AError)->AResult<()>;
    async fn set_request_manager(&self,request:Arc<Actor<RequestManager<T>>>)->AResult<()>;
    async fn call_controller(&self, tt:u8,cmd:i32,data:Data)->RetResult;
    async fn close(&self)-> Result<(),Box<dyn Error>>;

    async fn call(&self, serial:i64, buff:Data) ->Result<RetResult,Box<dyn Error>>;
    async fn call_0(&self, cmd:i32) ->Result<RetResult,Box<dyn Error>>;
    async fn call_1<A1:Writer+Send+Sync>
    (&self,cmd:i32,arg1:A1)->Result<RetResult,Box<dyn Error>>;
    async fn call_2<A1:Writer+Send+Sync,A2:Writer+Send+Sync>
    (&self,cmd:i32,arg1:A1,arg2:A2)->Result<RetResult,Box<dyn Error>>;
    async fn call_3<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync>
    (&self,cmd:i32,arg1: A1, arg2: A2,arg3:A3)->Result<RetResult,Box<dyn Error>>;
    async fn call_4<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync,A4: Writer + Send + Sync>
    (&self,cmd:i32,arg1: A1, arg2: A2,arg3:A3,arg4:A4)->Result<RetResult,Box<dyn Error>>;
    async fn call_5<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync,A4: Writer + Send + Sync,A5: Writer + Send + Sync>
    (&self,cmd:i32,arg1: A1, arg2: A2,arg3:A3,arg4:A4,arg5:A5)->Result<RetResult,Box<dyn Error>>;
    async fn call_6<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync,A4: Writer + Send + Sync,A5: Writer + Send + Sync,A6: Writer + Send + Sync>
    (&self,cmd:i32,arg1: A1, arg2: A2,arg3:A3,arg4:A4,arg5:A5,arg6:A6)->Result<RetResult,Box<dyn Error>>;
    async fn call_7<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync,A4: Writer + Send + Sync,A5: Writer + Send + Sync,A6: Writer + Send + Sync,A7: Writer + Send + Sync>
    (&self,cmd:i32,arg1: A1, arg2: A2,arg3:A3,arg4:A4,arg5:A5,arg6:A6,arg7:A7)->Result<RetResult,Box<dyn Error>>;
    async fn call_8<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync,A4: Writer + Send + Sync,A5: Writer + Send + Sync,A6: Writer + Send + Sync, A7: Writer + Send + Sync,A8: Writer + Send + Sync>
    (&self,cmd:i32,arg1: A1, arg2: A2,arg3:A3,arg4:A4,arg5:A5,arg6:A6,arg7:A7,arg8:A8)->Result<RetResult,Box<dyn Error>>;
    async fn call_9<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync,A4: Writer + Send + Sync,A5: Writer + Send + Sync,A6: Writer + Send + Sync,  A7: Writer + Send + Sync,A8: Writer + Send + Sync,A9: Writer + Send + Sync>
    (&self,cmd:i32,arg1: A1, arg2: A2,arg3:A3,arg4:A4,arg5:A5,arg6:A6,arg7:A7,arg8:A8,arg9:A9)->Result<RetResult,Box<dyn Error>>;
    async fn call_10<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync,A4: Writer + Send + Sync,A5: Writer + Send + Sync,A6: Writer + Send + Sync,  A7: Writer + Send + Sync,A8: Writer + Send + Sync,A9: Writer + Send + Sync,A10: Writer + Send + Sync>
    (&self,cmd:i32,arg1: A1, arg2: A2,arg3:A3,arg4:A4,arg5:A5,arg6:A6,arg7:A7,arg8:A8,arg9:A9,arg10:A10)->Result<RetResult,Box<dyn Error>>;


    async fn run(&self, buff:Data)-> Result<(),Box<dyn Error>>;
    async fn run0(&self,cmd:i32)->Result<(),Box<dyn Error>>;
    async fn run1<A1:Writer+Send+Sync>(&self,cmd:i32,arg1:A1)->Result<(),Box<dyn Error>>;
    async fn run2<A1:Writer+Send+Sync,A2:Writer+Send+Sync>(&self,cmd:i32,arg1:A1,arg2:A2)->Result<(),Box<dyn Error>>;
    async fn run3<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync>
    (&self,cmd:i32,arg1: A1, arg2: A2,arg3:A3)->Result<(),Box<dyn Error>>;
    async fn run4<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync,A4: Writer + Send + Sync>
    (&self,cmd:i32,arg1: A1, arg2: A2,arg3:A3,arg4:A4)->Result<(),Box<dyn Error>>;
    async fn run5<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync,A4: Writer + Send + Sync,A5: Writer + Send + Sync>
    (&self,cmd:i32,arg1: A1, arg2: A2,arg3:A3,arg4:A4,arg5:A5)->Result<(),Box<dyn Error>>;
    async fn run6<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync,A4: Writer + Send + Sync,A5: Writer + Send + Sync,A6: Writer + Send + Sync >
    (&self,cmd:i32,arg1: A1, arg2: A2,arg3:A3,arg4:A4,arg5:A5,arg6:A6)->Result<(),Box<dyn Error>>;
    async fn run7<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync,A4: Writer + Send + Sync,A5: Writer + Send + Sync,A6: Writer + Send + Sync,  A7: Writer + Send+ Sync >
    (&self,cmd:i32,arg1: A1, arg2: A2,arg3:A3,arg4:A4,arg5:A5,arg6:A6,arg7:A7)->Result<(),Box<dyn Error>>;
    async fn run8<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync,A4: Writer + Send + Sync,A5: Writer + Send + Sync,A6: Writer + Send + Sync,  A7: Writer + Send + Sync,A8: Writer + Send + Sync>
    (&self,cmd:i32,arg1: A1, arg2: A2,arg3:A3,arg4:A4,arg5:A5,arg6:A6,arg7:A7,arg8:A8)->Result<(),Box<dyn Error>>;
    async fn run9<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync,A4: Writer + Send + Sync,A5: Writer + Send + Sync,A6: Writer + Send + Sync,  A7: Writer + Send + Sync,A8: Writer + Send + Sync,A9: Writer + Send + Sync>
    (&self,cmd:i32,arg1: A1, arg2: A2,arg3:A3,arg4:A4,arg5:A5,arg6:A6,arg7:A7,arg8:A8,arg9:A9)->Result<(),Box<dyn Error>>;
    async fn run10<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync,A4: Writer + Send + Sync,A5: Writer + Send + Sync,A6: Writer + Send + Sync,  A7: Writer + Send + Sync,A8: Writer + Send + Sync,A9: Writer + Send + Sync,A10: Writer + Send + Sync>
    (&self,cmd:i32,arg1: A1, arg2: A2,arg3:A3,arg4:A4,arg5:A5,arg6:A6,arg7:A7,arg8:A8,arg9:A9,arg10:A10)->Result<(),Box<dyn Error>>;

    async fn runcheck0(&self,cmd:i32)->Result<RetResult,Box<dyn Error>>;
    async fn runcheck1<A1:Writer+Send+Sync>
    (&self,cmd:i32,arg1:A1)->Result<RetResult,Box<dyn Error>>;
    async fn runcheck2<A1:Writer+Send+Sync,A2:Writer+Send+Sync>
    (&self,cmd:i32,arg1:A1,arg2:A2)->Result<RetResult,Box<dyn Error>>;
    async fn runcheck3<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync>
    (&self,cmd:i32,arg1: A1, arg2: A2,arg3:A3)->Result<RetResult,Box<dyn Error>>;
    async fn runcheck4<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync,A4: Writer + Send + Sync>
    (&self,cmd:i32,arg1: A1, arg2: A2,arg3:A3,arg4:A4)->Result<RetResult,Box<dyn Error>>;
    async fn runcheck5<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync,A4: Writer + Send + Sync,A5: Writer + Send + Sync>
    (&self,cmd:i32,arg1: A1, arg2: A2,arg3:A3,arg4:A4,arg5:A5)->Result<RetResult,Box<dyn Error>>;
    async fn runcheck6<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync,A4: Writer + Send + Sync,A5: Writer + Send + Sync,A6: Writer + Send + Sync>
    (&self,cmd:i32,arg1: A1, arg2: A2,arg3:A3,arg4:A4,arg5:A5,arg6:A6)->Result<RetResult,Box<dyn Error>>;
    async fn runcheck7<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync,A4: Writer + Send + Sync,A5: Writer + Send + Sync,A6: Writer + Send + Sync,A7: Writer + Send + Sync>
    (&self,cmd:i32,arg1: A1, arg2: A2,arg3:A3,arg4:A4,arg5:A5,arg6:A6,arg7:A7)->Result<RetResult,Box<dyn Error>>;
    async fn runcheck8<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync,A4: Writer + Send + Sync,A5: Writer + Send + Sync,A6: Writer + Send + Sync, A7: Writer + Send + Sync,A8: Writer + Send + Sync>
    (&self,cmd:i32,arg1: A1, arg2: A2,arg3:A3,arg4:A4,arg5:A5,arg6:A6,arg7:A7,arg8:A8)->Result<RetResult,Box<dyn Error>>;
    async fn runcheck9<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync,A4: Writer + Send + Sync,A5: Writer + Send + Sync,A6: Writer + Send + Sync,  A7: Writer + Send + Sync,A8: Writer + Send + Sync,A9: Writer + Send + Sync>
    (&self,cmd:i32,arg1: A1, arg2: A2,arg3:A3,arg4:A4,arg5:A5,arg6:A6,arg7:A7,arg8:A8,arg9:A9)->Result<RetResult,Box<dyn Error>>;
    async fn runcheck10<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync,A4: Writer + Send + Sync,A5: Writer + Send + Sync,A6: Writer + Send + Sync,  A7: Writer + Send + Sync,A8: Writer + Send + Sync,A9: Writer + Send + Sync,A10: Writer + Send + Sync>
    (&self,cmd:i32,arg1: A1, arg2: A2,arg3:A3,arg4:A4,arg5:A5,arg6:A6,arg7:A7,arg8:A8,arg9:A9,arg10:A10)->Result<RetResult,Box<dyn Error>>;

}


macro_rules! make_call_ret_result {
        (@uint $($x:tt)*)=>(());
        (@count $($rest:expr),*)=>(<[()]>::len(&[$(make_call_ret_result!(@uint $rest)),*]));
        ($client:expr=>$cmd:expr;$($args:expr),*$(,)*) => {
            let serial=$client.new_serial();
            let args_count=make_call_ret_result!(@count $($args),*) as i32;
            let mut data=Data::with_capacity(128);
            data.write_to_le(&2400u32);
            data.write_to_le(&2u8);
            data.write_to_le(&$cmd);
            data.write_to_le(&serial);
            data.write_to_le(&args_count);
            $(data.write_to_le(&$args);)*
            return Ok($client.call(serial,data).await?);
        }
}

macro_rules! make_run {
        (@uint $($x:tt)*)=>(());
        (@count $($rest:expr),*)=>(<[()]>::len(&[$(make_run!(@uint $rest)),*]));
        ($client:expr=>$cmd:expr;$($args:expr),*$(,)*) => {
            let serial=$client.new_serial();
            let args_count=make_run!(@count $($args),*) as i32;
            let mut data=Data::with_capacity(128);
            data.write_to_le(&2400u32);
            data.write_to_le(&0u8);
            data.write_to_le(&$cmd);
            data.write_to_le(&serial);
            data.write_to_le(&args_count);
            $(data.write_to_le(&$args);)*
            $client.run(data).await?;
            return Ok(());
        }

}

macro_rules! make_runcheck {
        (@uint $($x:tt)*)=>(());
        (@count $($rest:expr),*)=>(<[()]>::len(&[$(make_runcheck!(@uint $rest)),*]));
        ($client:expr=>$cmd:expr;$($args:expr),*$(,)*) => {
            let serial=$client.new_serial();
            let args_count=make_runcheck!(@count $($args),*) as i32;
            let mut data=Data::with_capacity(128);
            data.write_to_le(&2400u32);
            data.write_to_le(&1u8);
            data.write_to_le(&$cmd);
            data.write_to_le(&serial);
            data.write_to_le(&args_count);
            $(data.write_to_le(&$args);)*
            return Ok($client.call(serial,data).await?);
        }
}

#[allow(clippy::too_many_arguments)]
#[aqueue::aqueue_trait]
impl<T:SessionSave+'static> INetXClient<T> for Actor<NetXClient<T>>{
    #[inline]
    async fn init<C:IController+Sync+Send+'static>(&self, controller: C) -> AResult<()> {
        self.inner_call(async move|inner|{
            inner.get_mut().init(controller);
            Ok(())
        }).await
    }

    #[inline]
    fn get_address(&self) -> String {
         unsafe {
             self.deref_inner().get_addr_string()
         }
    }

    #[inline]
    fn get_serviceinfo(&self) -> ServerInfo {
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
    async fn store_sessionid(&self, sessionid: i64) -> AResult<()> {
        self.inner_call(async move|inner|{
            inner.get_mut().store_sessionid(sessionid);
            Ok(())
        }).await
    }

    #[inline]
    async fn set_mode(&self, mode: u8) -> AResult<()> {
        self.inner_call(async move|inner|{
            inner.get_mut().set_mode(mode);
            Ok(())
        }).await
    }


    #[inline]
    async fn set_network_client(&self, client: Arc<Actor<TcpClient>>) -> AResult<()> {
        self.inner_call(async move|inner|{
            inner.get_mut().set_network_client(client);
            Ok(())
        }).await
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
    async fn set_request_manager(&self, request: Arc<Actor<RequestManager<T>>>) -> AResult<()> {
        self.inner_call(async move|inner|{
            inner.get_mut().set_request_manager(request);
            Ok(())
        }).await
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
    async fn close(&self) -> Result<(),Box<dyn Error>> {
        let net:AResult<Arc<Actor<TcpClient>>>= self.inner_call(async move|inner|{
            inner.get_mut().controller_fun_register_dict.clear();
            match inner.get_mut().net.take() {
                Some(net)=>Ok(net),
                None=>Err(AError::StrErr("not connect".into()))
            }
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
    async fn call(&self,serial:i64,buff: Data) -> Result<RetResult,Box<dyn Error>> {
        let (net,rx):(Arc<Actor<TcpClient>>,Receiver<AResult<Data>>)=self.inner_call(async move|inner|{
            if let Some(ref net)=inner.get().net{
                let (tx,rx):(Sender<AResult<Data>>,Receiver<AResult<Data>>)=oneshot();
                if inner.get_mut().result_dict.contains_key(&serial){
                    return Err(AError::StrErr("serial is have".into()))
                }

                inner.get_mut().result_dict.insert(serial,tx);
                Ok((net.clone(),rx))
            }else{
                Err(AError::StrErr("not connect".into()))
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
        println!("wait rx {}",serial);
        match rx.await {
            Err(_)=>{
                Err("tx is Close".into())
            }
            ,
            Ok(data)=>{
                Ok(RetResult::from(data?)?)
            }
        }
    }
    #[inline]
    async fn call_0(&self, cmd: i32) -> Result<RetResult, Box<dyn Error>> {
        make_call_ret_result!(self=>cmd;);
    }
    #[inline]
    async fn call_1<A1: Writer + Send + Sync>(&self, cmd: i32, arg1: A1) -> Result<RetResult, Box<dyn Error>> {
        make_call_ret_result!(self=>cmd;arg1);
    }
    #[inline]
    async fn call_2<A1: Writer + Send + Sync, A2: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2) -> Result<RetResult, Box<dyn Error>> {
        make_call_ret_result!(self=>cmd;arg1,arg2);
    }
    #[inline]
    async fn call_3<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2, arg3: A3) -> Result<RetResult, Box<dyn Error>> {
        make_call_ret_result!(self=>cmd;arg1,arg2,arg3);
    }
    #[inline]
    async fn call_4<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync, A4: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2, arg3: A3, arg4: A4) -> Result<RetResult, Box<dyn Error>> {
        make_call_ret_result!(self=>cmd;arg1,arg2,arg3,arg4);
    }
    #[inline]
    async fn call_5<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync, A4: Writer + Send + Sync, A5: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2, arg3: A3, arg4: A4, arg5: A5) -> Result<RetResult, Box<dyn Error>> {
        make_call_ret_result!(self=>cmd;arg1,arg2,arg3,arg4,arg5);
    }
    #[inline]
    async fn call_6<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync, A4: Writer + Send + Sync, A5: Writer + Send + Sync, A6: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2, arg3: A3, arg4: A4, arg5: A5, arg6: A6) -> Result<RetResult, Box<dyn Error>> {
        make_call_ret_result!(self=>cmd;arg1,arg2,arg3,arg4,arg5,arg6);
    }
    #[inline]
    async fn call_7<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync, A4: Writer + Send + Sync, A5: Writer + Send + Sync, A6: Writer + Send + Sync, A7: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2, arg3: A3, arg4: A4, arg5: A5, arg6: A6, arg7: A7) -> Result<RetResult, Box<dyn Error>> {
        make_call_ret_result!(self=>cmd;arg1,arg2,arg3,arg4,arg5,arg6,arg7);
    }
    #[inline]
    async fn call_8<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync, A4: Writer + Send + Sync, A5: Writer + Send + Sync, A6: Writer + Send + Sync, A7: Writer + Send + Sync, A8: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2, arg3: A3, arg4: A4, arg5: A5, arg6: A6, arg7: A7, arg8: A8) -> Result<RetResult, Box<dyn Error>> {
        make_call_ret_result!(self=>cmd;arg1,arg2,arg3,arg4,arg5,arg6,arg7,arg8);
    }
    #[inline]
    async fn call_9<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync, A4: Writer + Send + Sync, A5: Writer + Send + Sync, A6: Writer + Send + Sync, A7: Writer + Send + Sync, A8: Writer + Send + Sync, A9: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2, arg3: A3, arg4: A4, arg5: A5, arg6: A6, arg7: A7, arg8: A8, arg9: A9) -> Result<RetResult, Box<dyn Error>> {
        make_call_ret_result!(self=>cmd;arg1,arg2,arg3,arg4,arg5,arg6,arg7,arg8,arg9);
    }
    #[inline]
    async fn call_10<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync, A4: Writer + Send + Sync, A5: Writer + Send + Sync, A6: Writer + Send + Sync, A7: Writer + Send + Sync, A8: Writer + Send + Sync, A9: Writer + Send + Sync, A10: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2, arg3: A3, arg4: A4, arg5: A5, arg6: A6, arg7: A7, arg8: A8, arg9: A9, arg10: A10) -> Result<RetResult, Box<dyn Error>> {
        make_call_ret_result!(self=>cmd;arg1,arg2,arg3,arg4,arg5,arg6,arg7,arg8,arg9,arg10);
    }


    #[inline]
    async fn run(&self, buff: Data) -> Result<(),Box<dyn Error>> {
        let net= self.inner_call(async move|inner|{
            if let Some(ref net)=inner.get().net{
                Ok(net.clone())
            }else{
                Err(AError::StrErr("not connect".into()))
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
    #[inline]
    async fn run0(&self, cmd: i32) -> Result<(), Box<dyn Error>> {
        make_run!(self=>cmd;);
    }
    #[inline]
    async fn run1<A1: Writer + Send + Sync>(&self, cmd:i32, arg1: A1) -> Result<(), Box<dyn Error>> {
        make_run!(self=>cmd;arg1);
    }
    #[inline]
    async fn run2<A1: Writer + Send + Sync, A2: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2) -> Result<(), Box<dyn Error>> {
        make_run!(self=>cmd;arg1,arg2);
    }
    #[inline]
    async fn run3<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2, arg3: A3) -> Result<(), Box<dyn Error>> {
        make_run!(self=>cmd;arg1,arg2,arg3);
    }
    #[inline]
    async fn run4<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync, A4: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2, arg3: A3, arg4: A4) -> Result<(), Box<dyn Error>> {
        make_run!(self=>cmd;arg1,arg2,arg3,arg4);
    }
    #[inline]
    async fn run5<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync, A4: Writer + Send + Sync, A5: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2, arg3: A3, arg4: A4, arg5: A5) -> Result<(), Box<dyn Error>> {
        make_run!(self=>cmd;arg1,arg2,arg3,arg4,arg5);
    }
    #[inline]
    async fn run6<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync, A4: Writer + Send + Sync, A5: Writer + Send + Sync, A6: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2, arg3: A3, arg4: A4, arg5: A5, arg6: A6) -> Result<(), Box<dyn Error>> {
        make_run!(self=>cmd;arg1,arg2,arg3,arg4,arg5,arg6);
    }
    #[inline]
    async fn run7<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync, A4: Writer + Send + Sync, A5: Writer + Send + Sync, A6: Writer + Send + Sync, A7: Writer + Send+ Sync>(&self, cmd: i32, arg1: A1, arg2: A2, arg3: A3, arg4: A4, arg5: A5, arg6: A6, arg7: A7) -> Result<(), Box<dyn Error>> {
        make_run!(self=>cmd;arg1,arg2,arg3,arg4,arg5,arg6,arg7);
    }
    #[inline]
    async fn run8<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync, A4: Writer + Send + Sync, A5: Writer + Send + Sync, A6: Writer + Send + Sync, A7: Writer + Send + Sync, A8: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2, arg3: A3, arg4: A4, arg5: A5, arg6: A6, arg7: A7, arg8: A8) -> Result<(), Box<dyn Error>> {
        make_run!(self=>cmd;arg1,arg2,arg3,arg4,arg5,arg6,arg7,arg8);
    }
    #[inline]
    async fn run9<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync, A4: Writer + Send + Sync, A5: Writer + Send + Sync, A6: Writer + Send + Sync, A7: Writer + Send + Sync, A8: Writer + Send + Sync, A9: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2, arg3: A3, arg4: A4, arg5: A5, arg6: A6, arg7: A7, arg8: A8, arg9: A9) -> Result<(), Box<dyn Error>> {
        make_run!(self=>cmd;arg1,arg2,arg3,arg4,arg5,arg6,arg7,arg8,arg9);
    }
    #[inline]
    async fn run10<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync, A4: Writer + Send + Sync, A5: Writer + Send + Sync, A6: Writer + Send + Sync, A7: Writer + Send + Sync, A8: Writer + Send + Sync, A9: Writer + Send + Sync, A10: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2, arg3: A3, arg4: A4, arg5: A5, arg6: A6, arg7: A7, arg8: A8, arg9: A9, arg10: A10) -> Result<(), Box<dyn Error>> {
        make_run!(self=>cmd;arg1,arg2,arg3,arg4,arg5,arg6,arg7,arg8,arg9,arg10);
    }
    #[inline]
    async fn runcheck0(&self, cmd: i32) -> Result<RetResult, Box<dyn Error>> {
        make_runcheck!(self=>cmd;);
    }
    #[inline]
    async fn runcheck1<A1: Writer + Send + Sync>(&self, cmd: i32, arg1: A1) -> Result<RetResult, Box<dyn Error>> {
        make_runcheck!(self=>cmd;arg1);
    }
    #[inline]
    async fn runcheck2<A1: Writer + Send + Sync, A2: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2) -> Result<RetResult, Box<dyn Error>> {
        make_runcheck!(self=>cmd;arg1,arg2);
    }
    #[inline]
    async fn runcheck3<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2, arg3: A3) -> Result<RetResult, Box<dyn Error>> {
        make_runcheck!(self=>cmd;arg1,arg2,arg3);
    }
    #[inline]
    async fn runcheck4<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync, A4: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2, arg3: A3, arg4: A4) -> Result<RetResult, Box<dyn Error>> {
        make_runcheck!(self=>cmd;arg1,arg2,arg3,arg4);
    }
    #[inline]
    async fn runcheck5<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync, A4: Writer + Send + Sync, A5: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2, arg3: A3, arg4: A4, arg5: A5) -> Result<RetResult, Box<dyn Error>> {
        make_runcheck!(self=>cmd;arg1,arg2,arg3,arg4,arg5);
    }
    #[inline]
    async fn runcheck6<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync, A4: Writer + Send + Sync, A5: Writer + Send + Sync, A6: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2, arg3: A3, arg4: A4, arg5: A5, arg6: A6) -> Result<RetResult, Box<dyn Error>> {
        make_runcheck!(self=>cmd;arg1,arg2,arg3,arg4,arg5,arg6);
    }
    #[inline]
    async fn runcheck7<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync, A4: Writer + Send + Sync, A5: Writer + Send + Sync, A6: Writer + Send + Sync, A7: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2, arg3: A3, arg4: A4, arg5: A5, arg6: A6, arg7: A7) -> Result<RetResult, Box<dyn Error>> {
        make_runcheck!(self=>cmd;arg1,arg2,arg3,arg4,arg5,arg6,arg7);
    }
    #[inline]
    async fn runcheck8<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync, A4: Writer + Send + Sync, A5: Writer + Send + Sync, A6: Writer + Send + Sync, A7: Writer + Send + Sync, A8: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2, arg3: A3, arg4: A4, arg5: A5, arg6: A6, arg7: A7, arg8: A8) -> Result<RetResult, Box<dyn Error>> {
        make_runcheck!(self=>cmd;arg1,arg2,arg3,arg4,arg5,arg6,arg7,arg8);
    }
    #[inline]
    async fn runcheck9<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync, A4: Writer + Send + Sync, A5: Writer + Send + Sync, A6: Writer + Send + Sync, A7: Writer + Send + Sync, A8: Writer + Send + Sync, A9: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2, arg3: A3, arg4: A4, arg5: A5, arg6: A6, arg7: A7, arg8: A8, arg9: A9) -> Result<RetResult, Box<dyn Error>> {
        make_runcheck!(self=>cmd;arg1,arg2,arg3,arg4,arg5,arg6,arg7,arg8,arg9);
    }
    #[inline]
    async fn runcheck10<A1: Writer + Send + Sync, A2: Writer + Send + Sync, A3: Writer + Send + Sync, A4: Writer + Send + Sync, A5: Writer + Send + Sync, A6: Writer + Send + Sync, A7: Writer + Send + Sync, A8: Writer + Send + Sync, A9: Writer + Send + Sync, A10: Writer + Send + Sync>(&self, cmd: i32, arg1: A1, arg2: A2, arg3: A3, arg4: A4, arg5: A5, arg6: A6, arg7: A7, arg8: A8, arg9: A9, arg10: A10) -> Result<RetResult, Box<dyn Error>> {
        make_runcheck!(self=>cmd;arg1,arg2,arg3,arg4,arg5,arg6,arg7,arg8,arg9,arg10);
    }
}




#[macro_export]
macro_rules! call {
    (@uint $($x:tt)*)=>(());
    (@count $($rest:expr),*)=>(<[()]>::len(&[$(call!(@uint $rest)),*]));
    ($client:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            if $client.is_connect() ==false{
                NetXClient::connect_network($client.clone()).await?;
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
            $(data.serde_serialize(&$args)?;)*
            let ret= $client.call(serial,data).await?;
            let mut ret= ret.check()?;
            ret.deserialize()?
    });
    (@result $client:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            if $client.is_connect() ==false{
               NetXClient::connect_network($client.clone()).await?;
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
            $(data.serde_serialize(&$args)?;)*
            $client.call(serial,data).await?
    });
    (@run $client:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            if $client.is_connect() ==false{
                NetXClient::connect_network($client.clone()).await?;
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
            $(data.serde_serialize(&$args)?;)*
            $client.run(data).await?;
    });
     (@run_not_err $client:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            if $client.is_connect() ==false{
               if let Err(err)= NetXClient::connect_network($client.clone()).await{
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
            $(data.serde_serialize(&$args)?;)*
            if let Err(err)= $client.run(data).await{
                 log::error!{"run {} is error:{}",$cmd,err}
            }
    });
    (@checkrun $client:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            if $client.is_connect() ==false{
                NetXClient::connect_network($client.clone()).await?;
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
            $(data.serde_serialize(&$args)?;)*
            let ret=$client.call(serial,data).await?;
            ret.check()?;
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