use std::error::Error;
use std::sync::Arc;
use netxserver::*;
use crate::client::*;



#[build(TestController)]
pub trait ITestController{
    #[tag(1000)]
    async fn add(&self,a:i32,b:i32)->Result<i32,Box<dyn Error>>;
    #[tag(800)]
    async fn print(&self,a:i32)->Result<(),Box<dyn Error>>;
    #[tag(1005)]
    async fn recursive_test(&self, a:i32)->Result<i32,Box<dyn Error>>;
    #[tag(600)]
    async fn print2(&self,a:i32,b:String)->Result<(),Box<dyn Error>>;
}

pub struct TestController{
    token:NetxToken,
    client:Box<dyn IClient>
}

#[build_impl]
impl ITestController for TestController{
    #[inline]
    async fn add(&self, a: i32, b: i32) -> Result<i32, Box<dyn Error>> {
        Ok(a+b)
    }
    #[inline]
    async fn print(&self, a: i32) -> Result<(), Box<dyn Error>> {
        println!("print {}",a);
        Ok(())
    }
    #[inline]
    async fn recursive_test(&self, mut a: i32) -> Result<i32, Box<dyn Error>> {
        a -= 1;
        if a > 0 {
            let x: i32 = self.client.recursive_test(a).await?;
            Ok(x)
        } else {
            Ok(a)
        }


    }
    #[inline]
    async fn print2(&self, a: i32, b:String) -> Result<(), Box<dyn Error>> {
        self.client.print2(a,b).await
    }
}



pub struct ImplCreateController;
impl ICreateController for ImplCreateController {
    fn create_controller(&self, token: NetxToken) -> Result<Arc<dyn IController>, Box<dyn Error>> {
       Ok(Arc::new(TestController{
           client: impl_interface!(token=>IClient),
           token,
       }))
    }
}

