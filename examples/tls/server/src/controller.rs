use netxserver::prelude::*;
use std::sync::Arc;
use anyhow::{bail, Result};

#[build(TestController)]
pub trait ITestController{
    #[tag(100)]
    async fn hello(&self,msg:String)->Result<String>;
    #[tag(101)]
    async fn get_static_str(&self)->Result<&'static str>;
    #[tag(102)]
    async fn get_static_str2(&self)->Result<(i32,&'static str)>;
    #[tag(103)]
    async fn test_error(&self)->Result<()>;
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

    async fn get_static_str(&self) -> Result<&'static str> {
        Ok("test ok")
    }

    async fn get_static_str2(&self) -> Result<(i32, &'static str)> {
        Ok((1,"test ok"))
    }

    async fn test_error(&self) -> Result<()> {
        bail!("test error")
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
