use anyhow::Result;
use netxclient::prelude::*;

#[build]
pub trait IServer: Sync + Send{
    #[tag(100)]
    async fn hello(&self,msg:&str)->Result<String>;
    #[tag(101)]
    async fn get_static_str(&self)->Result<String>;
    #[tag(102)]
    async fn get_static_str2(&self)->Result<(i32,String)>;
    #[tag(103)]
    async fn test_error(&self)->Result<()>;
}