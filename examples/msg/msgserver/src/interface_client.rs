use netxserver::*;
use std::error::Error;

// client interface
// 客户端接口
#[build]
pub trait IClient:Sync+Send{
    #[tag(2001)]
    async fn message(&self,nickname:String,msg:String,to_me:bool);

    #[tag(3001)]
    async fn ping(&self,nickname:String,time:i64)->Result<i64,Box<dyn Error>>;
}