use std::collections::HashMap;
use std::sync::Arc;
use aqueue::{Actor, AResult, AError};
use crate::server::async_token::{AsyncToken, NetxToken};
use crate::controller::IGetController;
use crate::async_token::IAsyncToken;
use std::error::Error;

pub struct AsyncTokenManager<T>{
    impl_controller:T,
    dict:HashMap<i64,NetxToken>,
    current:i64
}

unsafe impl<T> Send for AsyncTokenManager<T>{}
unsafe impl<T> Sync for AsyncTokenManager<T>{}

impl<T:IGetController+'static> AsyncTokenManager<T>{
    #[inline]
    pub(crate) fn new(impl_controller:T)->Arc<Actor<AsyncTokenManager<T>>> {
       Arc::new(Actor::new(
           AsyncTokenManager{
               impl_controller,
               dict:HashMap::new(),
               current:1
           }
       ))
    }
    #[inline]
    fn make_new_sessionid(&mut self)->i64{
        let value=self.current;
        self.current=self.current.wrapping_add(1);
        value
    }
    #[inline]
    pub async fn create_token(&mut self)->Result<NetxToken,Box<dyn Error>>{
        let sessionid=self.make_new_sessionid();
        let token= Arc::new(Actor::new( AsyncToken::new(sessionid)));
        let map=self.impl_controller.get_controller(token.clone())?.register()?;
        token.set_controller_fun_maps(map).await?;
        self.dict.insert(sessionid,token.clone());
        Ok(token)
    }
    #[inline]
    pub fn get_token(&self,sessionid:i64)->Option<NetxToken>{
        self.dict.get(&sessionid).cloned()
    }
}

#[aqueue::aqueue_trait]
pub trait IAsyncTokenManager{
    async fn create_token(&self)->AResult<NetxToken>;
    async fn get_token(&self,sessionid:i64)->AResult<Option<NetxToken>>;
}

#[aqueue::aqueue_trait]
impl<T:IGetController+'static> IAsyncTokenManager for Actor<AsyncTokenManager<T>>{
    #[inline]
    async fn create_token(&self) -> AResult<NetxToken> {
       self.inner_call(async move|inner|{
           match  inner.get_mut().create_token().await {
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
}