use crate::client::RetResult;
use anyhow::{bail, Result};
use data_rw::DataOwnedReader;

#[async_trait::async_trait]
pub trait IController:Send+Sync {
    async fn call(&self,tt:u8,cmd_tag:i32,dr: DataOwnedReader) -> Result<RetResult>;
}

#[derive(Default)]
pub struct DefaultController;
#[async_trait::async_trait]
impl IController for DefaultController {
    async fn call(&self, _tt: u8, cmd_tag: i32, _dr: DataOwnedReader) -> Result<RetResult> {
        bail!("not found cmd tag:{} for DefaultController",cmd_tag)
    }
}