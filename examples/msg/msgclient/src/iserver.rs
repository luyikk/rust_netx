use std::error::Error;
use packer::*;
use netxclient::*;

#[build]
pub trait IServer {
    #[tag(1000)]
    async fn login(&self,msg:LogOn)->Result<LogOnRes,Box<dyn Error>>;
    #[tag(1001)]
    async fn get_users(&self)->Result<Vec<User>,Box<dyn Error>>;
    #[tag(1002)]
    async fn talk(&self,msg:String)->Result<(),Box<dyn Error>>;
}