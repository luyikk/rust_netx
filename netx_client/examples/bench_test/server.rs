use netxclient::prelude::{error::Result, *};

#[build]
pub trait IServer: Sync + Send {
    #[tag(1000)]
    async fn add(&self, a: i32, b: i32) -> Result<i32>;
}
