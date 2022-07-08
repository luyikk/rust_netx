use netxserver::prelude::*;
use std::sync::Arc;
use anyhow::Result;

#[build(TestController)]
pub trait ITestController{
    #[tag(100)]
    async fn hello(&self,msg:String)->Result<String>;
}

pub struct TestController {
    _token: NetxToken
}

unsafe impl Send for TestController {}
unsafe impl Sync for TestController {}

#[build_impl]
impl ITestController for TestController {
    async fn hello(&self,msg: String) -> Result<String> {
        log::info!("client:{}",msg);
        Ok(format!("{} hello",msg))
    }
}


pub struct ImplCreateController;
impl ICreateController for ImplCreateController {
    fn create_controller(&self, token: NetxToken) -> Result<Arc<dyn IController>> {
        Ok(Arc::new(TestController {
            _token:token,
        }))
    }
}
