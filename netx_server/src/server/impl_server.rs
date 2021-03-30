use tcpserver::{TCPPeer, Builder, ITCPServer, IPeer};
use std::sync::{Arc, Weak};
use aqueue::{Actor, AResult};
use tokio::net::tcp::OwnedReadHalf;
use log::*;
use tokio::task::JoinHandle;
use std::error::Error;
use tokio::io::AsyncReadExt;
use data_rw::Data;
use crate::{OwnedReadHalfExt, ServerOption, RetResult};
use crate::server::async_token_manager::AsyncTokenManager;
use crate::async_token_manager::{IAsyncTokenManager, TokenManager};
use crate::async_token::{IAsyncToken, NetxToken};
use crate::controller::ICreateController;
use bytes::Buf;

enum SpecialFunctionTag{
   CONNECT=2147483647,
   DISCONNECT=2147483646
}

pub struct NetXServer<T>{
   option:ServerOption,
   serv:Arc<dyn ITCPServer<Arc<NetXServer<T>>>>,
   async_tokens:TokenManager<T>
}
unsafe impl<T> Send for NetXServer<T>{}
unsafe impl<T> Sync for NetXServer<T>{}


impl<T: ICreateController +'static> NetXServer<T> {
   #[inline]
   pub async fn new(option:ServerOption,impl_controller:T)->Arc<NetXServer<T>>{
      let serv= Builder::new(&option.addr).set_connect_event(|addr|{
         info!("{} connect",addr);
         true
      }).set_input_event(async move|mut reader,peer,serv|{
         let addr=peer.addr();
         let token= match Self::get_peer_token(&mut reader, &peer, &serv).await {
            Ok(token)=>token,
            Err(er)=>{
               info!("user:{}:{},disconnect it", addr, er);
               return
            }
         };

         if let Err(er)=Self::read_buff_byline(&mut reader,&peer,&token).await{
            error!("read buff err:{}",er)
         }
         if let Err(er)= token.call_special_function(SpecialFunctionTag::DISCONNECT as i32).await{
            error!("call token disconnect err:{}",er)
         }
         if let Err(er)= serv.async_tokens.peer_disconnect(token.get_sessionid()).await{
            error!("peer disconnect err:{}",er)
         }

      }).build().await;
      let request_out_time=option.request_out_time;
      let session_save_time=option.session_save_time;
      Arc::new(NetXServer{
         option,
         serv,
         async_tokens:AsyncTokenManager::new(impl_controller,request_out_time,session_save_time)
      })
   }

   #[inline]
   async fn get_peer_token(mut reader:&mut OwnedReadHalf, peer:&Arc<Actor<TCPPeer>>, serv:&Arc<NetXServer<T>>) ->Result<NetxToken,Box<dyn Error>>{
      let cmd=reader.read_i32_le().await?;
      if cmd !=1000{
         Self::send_to_key_verify_msg(&peer,true,"not verify key").await?;
         return Err("not verify key".into())
      }
      let name=reader.read_string().await?;
      if !serv.option.service_name.is_empty() && name !=serv.option.service_name{
         Self::send_to_key_verify_msg(&peer,true,"service name error").await?;
         return Err(format!("IP:{} service name:{} error",peer.addr(),name).into())
      }
      let password=reader.read_string().await?;
      if !serv.option.verify_key.is_empty() && password!=serv.option.verify_key{
         Self::send_to_key_verify_msg(&peer,true,"service verify key error").await?;
         return Err(format!("IP:{} verify key:{} error",peer.addr(),name).into())
      }
      Self::send_to_key_verify_msg(&peer,false,"verify success").await?;
      let session=reader.read_i64_le().await?;
      let token=
         if session==0{
           serv.async_tokens.create_token(Arc::downgrade(&serv.async_tokens) as Weak<dyn IAsyncTokenManager>).await?
         }
         else {
             match serv.async_tokens.get_token(session).await? {
               Some(token) => token,
               None => serv.async_tokens.create_token(Arc::downgrade(&serv.async_tokens) as Weak<dyn IAsyncTokenManager>).await?
            }
         };

      Ok(token)
   }

   #[inline]
   async fn read_buff_byline(mut reader:&mut OwnedReadHalf,peer:&Arc<Actor<TCPPeer>>,token:&NetxToken)->Result<(),Box<dyn Error>>{
      token.set_peer(Some(Arc::downgrade(peer))).await?;
      token.call_special_function(SpecialFunctionTag::CONNECT as i32).await?;
      Self::data_reading(&mut reader,peer,token).await?;
      Ok(())
   }

   #[inline]
   async fn data_reading(mut reader:&mut OwnedReadHalf,peer:&Arc<Actor<TCPPeer>>,token:&NetxToken)->Result<(),Box<dyn Error>>{

      while let Ok(mut data)=reader.read_buff().await{
         let cmd=data.get_le::<i32>()?;
         match cmd {
            2000=>{
               Self::sendto(peer,Self::send_to_sessionid( token.get_sessionid())).await?;
            },
            2400 => {
               let tt=data.get_le::<u8>()?;
               let cmd=data.get_le::<i32>()?;
               let serial=data.get_le::<i64>()?;
               match tt {
                  0=>{
                     let run_token=token.clone();
                     tokio::spawn(async move {
                        let _ = run_token.run_controller(tt, cmd, data).await;
                     });
                  },
                  1=>{
                     let run_token=token.clone();
                     tokio::spawn(async move{
                        let res= run_token.run_controller(tt,cmd,data).await;
                        if let Err(er)= run_token.send(Self::get_result_buff(serial,res)).await{
                           error!("send buff 1 error:{}",er);
                        }
                     });
                  },
                  2=>{
                     let run_token=token.clone();
                     tokio::spawn(async move{
                        let res= run_token.run_controller(tt,cmd,data).await;
                        if let Err(er)=  run_token.send(Self::get_result_buff(serial,res)).await{
                           error!("send buff {} error:{}",serial,er);
                        }
                     });
                  },
                  _=>{
                     error!("not found call type:{}",tt)
                  }
               }
            },
            2500=>{
               let serial=data.get_le::<i64>()?;
               token.set_result(serial,data).await?;
            },
            _ => {
               error!("not found cmd:{}",cmd)
            }
         }
      }
      Ok(())
   }

   #[inline]
   fn get_result_buff(serial:i64,result:RetResult)->Data {
      let mut data = Data::with_capacity(1024);
      data.write_to_le(&2500u32);
      data.write_to_le(&serial);

      if result.is_error {
         data.write_to_le(&true);
         data.write_to_le(&result.error_id);
         data.write_to_le(&result.msg);
      } else {
         data.write_to_le(&false);
         data.write_to_le(&(result.arguments.len() as u32));
         for argument in result.arguments {
            data.write_to_le(&argument.bytes());
         }
      }

      let len = data.len() + 4usize;
      let mut buff = Data::with_capacity(len);
      buff.write_to_le(&(len as u32));
      buff.write(&data);
      buff
   }
   #[inline]
   fn send_to_sessionid(sessionid:i64)->Data{
      let mut data=Data::new();
      data.write_to_le(&2000i32);
      data.write_to_le(&sessionid);
      data
   }
   #[inline]
   async fn send_to_key_verify_msg(peer:&Arc<Actor<TCPPeer>>, is_err:bool, msg:&str) -> AResult<usize> {
      let mut data=Data::new();
      data.write_to_le(&1000i32);
      data.write_to_le(&is_err);
      data.write_to_le(&msg);
      data.write_to_le(&1u8);
      Self::sendto(peer,data).await
   }
   #[inline]
   async fn sendto(peer:&Arc<Actor<TCPPeer>>,buff:Data)-> AResult<usize>{
      let buff=&*buff;
      let len=buff.len()+4;
      let mut data=Data::with_capacity(len);
      data.write_to_le(&(len as u32));
      data.write(buff);
      peer.send(data).await
   }
   #[inline]
   pub async fn start(self:&Arc<Self>) -> Result<JoinHandle<tokio::io::Result<()>>,Box<dyn Error>> {
      Ok(self.serv.start(self.clone()).await?)
   }
   #[inline]
   pub async fn start_block(self:&Arc<Self>)->Result<(),Box<dyn Error>>{
      self.serv.start_block(self.clone()).await
   }
}