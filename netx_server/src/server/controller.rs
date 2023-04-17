use crate::async_token::NetxToken;
use crate::result::RetResult;
use anyhow::Result;
use data_rw::DataOwnedReader;
use std::sync::Arc;

#[async_trait::async_trait]
pub trait IController: Send + Sync {
    async fn call(&self, tt: u8, cmd_tag: i32, dr: DataOwnedReader) -> Result<RetResult>;
}

pub trait ICreateController {
    type Controller: IController;
    fn create_controller(
        &self,
        token: NetxToken<Self::Controller>,
    ) -> Result<Arc<Self::Controller>>
    where
        Self: Sized;
}
