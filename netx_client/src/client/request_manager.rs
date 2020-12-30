use tokio::sync::Mutex;
use std::collections::VecDeque;
use std::sync::{Weak, Arc};
use aqueue::{Actor, AError};
use crate::client::{NetXClient, SessionSave, INetXClient};
use std::time::{Instant, Duration};
use tokio::time::sleep;
use log::*;


pub struct RequestManager<T>{
    queue:Mutex<VecDeque <(i64,Instant)>>,
    request_out_time:u32,
    netx_client:Weak<Actor<NetXClient<T>>>
}

impl<T> Drop for RequestManager<T>{
    fn drop(&mut self) {
        debug!("request manager is drop");
    }
}

impl<T:SessionSave+'static> RequestManager<T>{
    pub fn new(request_out_time:u32,netx_client:Weak<Actor<NetXClient<T>>>)->Arc<RequestManager<T>> {
        let ptr= Arc::new(RequestManager{
            queue:Mutex::new(VecDeque::new()),
            request_out_time,
            netx_client
        });

        Self::start_check(Arc::downgrade(&ptr));
        ptr
    }

    fn start_check(request_manager:Weak<RequestManager<T>>){
        tokio::spawn(async move{
            while let Some(req)=  request_manager.upgrade() {
                req.check().await;
                sleep(Duration::from_millis(500)).await
            }
        });
    }

    pub async fn check(&self) {
        let mut queue = self.queue.lock().await;
        while let Some(item) = queue.pop_back() {
            if item.1.elapsed().as_millis() as u32 >= self.request_out_time {
                if let Some(client) = self.netx_client.upgrade() {
                    if let Err(er) = client.set_error(item.0, AError::StrErr("time out".into())).await {
                        error!("check err:{}", er);
                        break;
                    }
                } else {
                    break;
                }
            } else {
                queue.push_back(item);
                break;
            }
        }
    }


    pub async fn set(&self,sessionid:i64){
        let mut queue= self.queue.lock().await;
        queue.push_front((sessionid, Instant::now()));
    }
}