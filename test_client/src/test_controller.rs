
use std::cell::RefCell;

use netxclient::*;
use netxbuilder::*;
use crate::server::*;

#[build_trait(TestController)]
pub trait ITestController{
    #[tag(2000)]
    async fn add_one(&self,i:i32)->Result<i32,Box<dyn Error>>;
    #[tag(3000)]
    async fn print(&self,i:i32);
    #[tag(4000)]
    async fn run(&self,name:String)->Result<(),Box<dyn Error>>;
    #[tag(5000)]
    async fn print2(&self,i:i32,s:String);
    #[tag(2002)]
    async fn recursive_test(&self, a:i32)->Result<i32,Box<dyn Error>>;

}

type Client=Arc<Actor<NetXClient<DefaultSessionStore>>>;

#[allow(dead_code)]
pub struct TestController{
    client:Client,
    server:Box<dyn IServer>,
    name:RefCell<String>
}

impl TestController{
    pub fn new(client:Client)->TestController{
        TestController{
            server: impl_interface!(client=>IServer),
            client,
            name: RefCell::new("".to_string())
        }
    }
}

unsafe  impl Sync for TestController{}
unsafe  impl Send for TestController{}

#[build_impl]
impl ITestController for TestController{
    #[inline]
    async fn add_one(&self, i: i32) -> Result<i32, Box<dyn Error>> {
        Ok(i+1)
    }
    #[inline]
    async fn print(&self, i: i32) {
        println!("{}",i);
    }
    #[inline]
    async fn run(&self, name: String) -> Result<(), Box<dyn Error>> {
        println!("name:{}",name);
        (*self.name.borrow_mut())=name;
        Ok(())
    }
    #[inline]
    async fn print2(&self, i: i32, s: String) {
        println!("{}-{}-{}",i,s,self.name.borrow());
    }

    #[inline]
    async fn recursive_test(&self,mut a: i32) -> Result<i32, Box<dyn Error>> {
        a -= 1;
        if a > 0 {
            let x: i32 = self.server.recursive_test(a).await?;
            Ok(x)
        } else {
            Ok(a)
        }
    }
}