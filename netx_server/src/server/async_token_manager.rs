use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Weak};
use aqueue::{Actor, AResult, AError};
use crate::server::async_token::{AsyncToken, NetxToken};
use crate::controller::ICreateController;
use crate::async_token::IAsyncToken;
use std::error::Error;
use tokio::time::{sleep, Duration, Instant};
use log::*;

pub struct AsyncTokenManager<T>{
    impl_controller:T,
    dict:HashMap<i64,NetxToken>,
    request_out_time:u32,
    session_save_time:u32,
    request_disconnect_clear_queue:VecDeque<(i64,Instant)>
}

unsafe impl<T> Send for AsyncTokenManager<T>{}
unsafe impl<T> Sync for AsyncTokenManager<T>{}

pub type TokenManager<T>=Arc<Actor<AsyncTokenManager<T>>>;

impl<T: ICreateController +'static> AsyncTokenManager<T>{
    #[inline]
    pub(crate) fn new(impl_controller:T,request_out_time:u32,session_save_time:u32)->TokenManager<T> {
        let ptr = Arc::new(Actor::new(
            AsyncTokenManager {
                impl_controller,
                dict: HashMap::new(),
                request_out_time,
                session_save_time,
                request_disconnect_clear_queue:Default::default()
            }
        ));

        Self::start_check(Arc::downgrade(&ptr));
        ptr
    }

    #[inline]
    fn start_check(wk:Weak<Actor<AsyncTokenManager<T>>>){
        tokio::spawn(async move{
            while let Some(manager)=wk.upgrade() {
                if let Err(err)= manager.check_tokens_request_timeout().await{
                    error!("check request time out err:{}",err);
                }

                if let Err(err)=manager.check_tokens_disconnect_timeout().await{
                    error!("check tokens disconnect out err:{}",err);
                }

                sleep(Duration::from_millis(50)).await
            }
        });
    }

    #[inline]
    async fn check_tokens_request_timeout(&self)->AResult<()>{
        for token in self.dict.values() {
            token.check_request_timeout(self.request_out_time).await?;
        }
        Ok(())
    }

    #[inline]
    async fn check_tokens_disconnect_timeout(&mut self)->AResult<()>{
        while let Some(item) = self.request_disconnect_clear_queue.pop_back() {
            if item.1.elapsed().as_millis() as u32 >= self.session_save_time {
                if let Some(token)= self.dict.get(&item.0) {
                    if token.is_disconnect().await? {
                        if let Some(token) = self.dict.remove(&item.0) {
                            token.clear_controller_fun_maps().await?;
                            debug!("token {} remove", token.get_sessionid());
                        } else{
                            debug!("remove token {} fail",item.0);
                        }
                    }
                    else{
                        debug!("remove token {},but it not disconnect",item.0);
                    }
                }
                else{
                    debug!("remove token not found {}",item.0);
                }
            } else {
                self.request_disconnect_clear_queue.push_back(item);
                break;
            }
        }

        Ok(())
    }

    #[inline]
    fn make_new_sessionid(&mut self)->i64{
       chrono::Local::now().timestamp_nanos()
    }
    #[inline]
    pub async fn create_token(&mut self,manager:Weak<dyn IAsyncTokenManager>)->Result<NetxToken,Box<dyn Error>>{
        let sessionid=self.make_new_sessionid();
        let token= Arc::new(Actor::new( AsyncToken::new(sessionid,manager)));
        let controller=self.impl_controller.create_controller(token.clone())?;
        let map=controller.register()?;
        token.set_controller_fun_maps(map).await?;
        self.dict.insert(sessionid,token.clone());
        Ok(token)
    }


    #[inline]
    pub fn get_token(&self,sessionid:i64)->Option<NetxToken>{
        self.dict.get(&sessionid).cloned()
    }

    #[inline]
    pub fn get_all_tokens(&self)->Vec<NetxToken>{
        self.dict.values().cloned().collect()
    }
}

#[aqueue::aqueue_trait]
pub trait IAsyncTokenManager:Send+Sync{
    async fn create_token(&self,manager:Weak<dyn IAsyncTokenManager>)->AResult<NetxToken>;
    async fn get_token(&self,sessionid:i64)->AResult<Option<NetxToken>>;
    async fn get_all_tokens(&self)->AResult<Vec<NetxToken>>;
    async fn check_tokens_request_timeout(&self)->AResult<()>;
    async fn check_tokens_disconnect_timeout(&self)->AResult<()>;
    async fn peer_disconnect(&self,sessionid:i64)->AResult<()>;
}

#[aqueue::aqueue_trait]
impl<T: ICreateController +'static> IAsyncTokenManager for Actor<AsyncTokenManager<T>>{
    #[inline]
    async fn create_token(&self,manager:Weak<dyn IAsyncTokenManager>) -> AResult<NetxToken> {
       self.inner_call(async move|inner|{
           match  inner.get_mut().create_token(manager).await {
               Ok(r)=>Ok(r),
               Err(er)=>Err(AError::StrErr(er.to_string()))
           }
       }).await
    }
    #[inline]
    async fn get_token(&self,sessionid:i64) -> AResult<Option<NetxToken>> {
        self.inner_call(async move|inner|{
            Ok(inner.get().get_token(sessionid))
        }).await
    }

    #[inline]
    async fn get_all_tokens(&self) -> AResult<Vec<NetxToken>> {
        self.inner_call(async move|inner|{
            Ok(inner.get().get_all_tokens())
        }).await
    }
    #[inline]
    async fn check_tokens_request_timeout(&self) -> AResult<()> {
        self.inner_call(async move|inner|{
            inner.get().check_tokens_request_timeout().await
        }).await
    }

    async fn check_tokens_disconnect_timeout(&self) -> AResult<()> {
        self.inner_call(async move|inner|{
            inner.get_mut().check_tokens_disconnect_timeout().await
        }).await
    }

    #[inline]
    async fn peer_disconnect(&self, sessionid: i64) -> AResult<()> {
        self.inner_call(async move|inner|{
            debug!("token {} start disconnect clear ",sessionid);
            inner.get_mut().request_disconnect_clear_queue.push_front((sessionid,Instant::now()));
            Ok(())
        }).await
    }
}