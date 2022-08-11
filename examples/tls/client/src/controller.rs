use anyhow::Result;
use netxclient::prelude::*;

#[build]
pub trait IServer: Sync + Send{
    #[tag(100)]
    async fn hello(&self,msg:&str)->Result<String>;
    #[tag(101)]
    async fn get_static_str(&self)->Result<String>;
}