use netxserver::*;

// client interface
// 客户端接口
#[build]
pub trait IClient:Sync+Send{
    #[tag(2001)]
    async fn message(&self,nickname:String,msg:String,to_me:bool);
}