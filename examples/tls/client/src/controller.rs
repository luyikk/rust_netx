use anyhow::Result;
use netxclient::prelude::*;

#[build]
pub trait IServer: Sync + Send{
    #[tag(100)]
    async fn hello(&self,msg:&str)->Result<String>;
}