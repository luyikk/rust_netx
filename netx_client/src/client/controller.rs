use crate::client::RetResult;
use anyhow::*;
use data_rw::Data;
use std::collections::HashMap;
use std::sync::Arc;

pub trait IController {
    fn register(self: Arc<Self>) -> Result<HashMap<i32, Box<dyn FunctionInfo>>>;
}

#[derive(Default)]
pub struct DefaultController;
impl IController for DefaultController {
    fn register(self: Arc<Self>) -> Result<HashMap<i32, Box<dyn FunctionInfo>>> {
        Ok(HashMap::new())
    }
}

#[async_trait::async_trait]
pub trait FunctionInfo: Send + Sync {
    fn function_type(&self) -> u8;
    async fn call(&self, data: Data) -> Result<RetResult>;
}
