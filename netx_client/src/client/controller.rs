
use std::sync::Arc;


use std::error::Error;
use std::collections::HashMap;
use data_rw::Data;

use crate::client::RetResult;


pub trait IController{
    fn register(self:Arc<Self>)->Result<HashMap<i32,Box<dyn FunctionInfo>>,Box<dyn Error>>;
}


#[derive(Default)]
pub struct DefaultController;


impl IController for DefaultController{
    fn register(self:Arc<Self>) -> Result<HashMap<i32, Box<dyn FunctionInfo>>, Box<dyn Error>> {
        Ok(HashMap::new())
    }
}

#[aqueue::aqueue_trait]
pub trait FunctionInfo:Send+Sync{
    fn function_type(&self)->u8;
    async fn call(&self,data:Data)->Result<RetResult,Box<dyn Error>>;
}

