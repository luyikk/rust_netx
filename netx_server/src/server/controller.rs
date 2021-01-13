
use std::sync::Arc;


use std::error::Error;
use std::collections::HashMap;
use data_rw::Data;
use crate::result::RetResult;
use aqueue::Actor;
use crate::async_token::{AsyncToken, NetxToken};


pub trait IController{
    fn register(self:Arc<Self>)->Result<HashMap<i32,Box<dyn FunctionInfo>>,Box<dyn Error>>;
}


#[aqueue::aqueue_trait]
pub trait FunctionInfo:Send+Sync{
    fn function_type(&self)->u8;
    async fn call(&self,data:Data)->Result<RetResult,Box<dyn Error>>;
}

pub trait IGetController{
    fn get_controller(&self,token:NetxToken)->Result<Arc<dyn IController>,Box<dyn Error>>;
}