use std::sync::Arc;
use std::collections::HashMap;
use data_rw::Data;
use crate::result::RetResult;
use crate::async_token::NetxToken;
use anyhow::*;


pub trait IController:Send+Sync{
    fn register(self:Arc<Self>)->Result<HashMap<i32,Box<dyn FunctionInfo>>>;
}

#[async_trait::async_trait]
pub trait FunctionInfo:Send+Sync{
    fn function_type(&self)->u8;
    async fn call(&self,data:Data)->Result<RetResult>;
}

pub trait ICreateController {
    fn create_controller(&self, token:NetxToken) ->Result<Arc<dyn IController>>;
}

