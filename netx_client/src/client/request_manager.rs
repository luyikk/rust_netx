use std::collections::VecDeque;
use std::sync::{Weak, Arc};
use aqueue::{Actor, AError, AResult};
use crate::client::{NetXClient, SessionSave, INetXClient};
use std::time::{Instant, Duration};
use tokio::time::sleep;
use log::*;


pub struct RequestManager<T>{
    queue:VecDeque <(i64,Instant)>,
    request_out_time:u32,
    netx_client:Weak<Actor<NetXClient<T>>>
}

impl<T> Drop for RequestManager<T>{
    fn drop(&mut self) {
        debug!("request manager is drop");
    }
}

impl<T:SessionSave+'static> RequestManager<T>{
    pub fn new(request_out_time:u32,netx_client:Weak<Actor<NetXClient<T>>>)->Arc<Actor<RequestManager<T>>> {
        let ptr= Arc::new(Actor::new(RequestManager{
            queue:VecDeque::new(),
            request_out_time,
            netx_client
        }));

        Self::start_check(Arc::downgrade(&ptr));
        ptr
    }

    fn start_check(request_manager:Weak<Actor<RequestManager<T>>>){
        tokio::spawn(async move{
            while let Some(req)=  request_manager.upgrade() {
                if let Err(er) = req.check().await {
                    error!("check request error:{}", er);
                }
                sleep(Duration::from_millis(500)).await
            }
        });
    }

    #[inline]
    pub async fn check(&mut self) {
        while let Some(item) =  self.queue.pop_back() {
            if item.1.elapsed().as_millis() as u32 >= self.request_out_time {
                if let Some(client) = self.netx_client.upgrade() {
                    if let Err(er) = client
                        .set_error(item.0, AError::StrErr(format!("serial:{} time out",item.0))).await {
                        error!("check err:{}", er);
                    }
                }
            } else {
                self.queue.push_back(item);
                break;
            }
        }
    }

    #[inline]
    pub fn set(&mut self,sessionid:i64){
        self.queue.push_front((sessionid, Instant::now()));
    }
}

#[aqueue::aqueue_trait]
pub trait IRequestManager{
    async fn check(&self)->AResult<()>;
    async fn set(&self,sessionid:i64)->AResult<()>;
}

#[aqueue::aqueue_trait]
impl <T:SessionSave+'static> IRequestManager for Actor<RequestManager<T>>{
    #[inline]
    async fn check(&self) -> AResult<()> {
        self.inner_call(async move|inner|{
            inner.get_mut().check().await;
            Ok(())
        }).await
    }
    #[inline]
    async fn set(&self,sessionid:i64) -> AResult<()> {
        self.inner_call(async move|inner|{
            inner.get_mut().set(sessionid);
            Ok(())
        }).await
    }
}