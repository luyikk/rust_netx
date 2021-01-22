use netxclient::*;
use crate::test_struct::{LogOn, LogOnResult};


#[build]
pub trait IServer:Sync+Send{
    #[tag(1000)]
    async fn add(&self,a:i32,b:i32)->Result<i32,Box<dyn Error>>;
    #[tag(800)]
    async fn print(&self,a:i32)->Result<(),Box<dyn Error>>;
    #[tag(600)]
    async fn print2(&self,a:i32,b:&str)->Result<(),Box<dyn Error>>;
    #[tag(700)]
    async fn run_test(&self,a:&str)->Result<(),Box<dyn Error>>;
    #[tag(5001)]
    async fn test(&self);
    #[tag(1003)]
    async fn to_client_add_one(&self,a:i32)->Result<i32,Box<dyn Error>>;
    #[tag(1005)]
    async fn recursive_test(&self,a:i32)->Result<i32,Box<dyn Error>>;
    #[tag(10000)]
    async fn logon(&self,info:LogOn)->Result<LogOnResult,Box<dyn Error>>;
}
