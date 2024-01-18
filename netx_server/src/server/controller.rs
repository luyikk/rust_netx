use crate::async_token::NetxToken;
use crate::result::RetResult;
use anyhow::Result;
use data_rw::DataOwnedReader;
use std::sync::Arc;

pub trait IController: Send + Sync {
    fn call(
        &self,
        tt: u8,
        cmd_tag: i32,
        dr: DataOwnedReader,
    ) -> impl std::future::Future<Output = Result<RetResult>> + Send;
}

pub trait ICreateController {
    type Controller: IController;
    fn create_controller(
        &self,
        token: NetxToken<Self::Controller>,
    ) -> Result<Arc<Self::Controller>>;
}
