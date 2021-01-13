use std::error::Error;
use std::sync::Arc;
use netxserver::*;
use netxbuilder::*;
use std::collections::hash_map::RandomState;
use std::collections::HashMap;


#[build_trait(TestController)]
pub trait ITestController{
    #[tag(1000)]
    async fn add(&self,a:i32,b:i32)->Result<i32,Box<dyn Error>>;
    #[tag(800)]
    async fn print(&self,a:i32)->Result<(),Box<dyn Error>>;

}

pub struct TestController{
    token:NetxToken
}

#[build_impl]
impl ITestController for TestController{
    async fn add(&self, a: i32, b: i32) -> Result<i32, Box<dyn Error>> {
       // println!("a+b={}",a+b);
        Ok(a+b)
    }

    async fn print(&self, a: i32) -> Result<(), Box<dyn Error>> {
        println!("print {}",a);
        Ok(())
    }
}

pub struct ImplTestController;

impl IGetController for ImplTestController{
    fn get_controller(&self, token: NetxToken) -> Result<Arc<dyn IController>, Box<dyn Error>> {
       Ok(Arc::new(TestController{
           token
       }))
    }
}

