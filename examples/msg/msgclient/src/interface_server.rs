use std::error::Error;
use packer::*;
use netxclient::*;


//服务器接口,调用服务器需要使用它
//server interface,it is required to call the server
#[build]
pub trait IServer {
    #[tag(1000)]
    async fn login(&self,msg:LogOn)->Result<LogOnRes,Box<dyn Error>>;
    #[tag(1001)]
    async fn get_users(&self)->Result<Vec<User>,Box<dyn Error>>;
    #[tag(1002)]
    async fn talk(&self,msg:String)->Result<(),Box<dyn Error>>;
    #[tag(1003)]
    async fn to(&self,target:String,msg:String)->Result<(),Box<dyn Error>>;
    #[tag(1004)]
    async fn ping(&self,target:String,time:i64)->Result<i64,Box<dyn Error>>;
}