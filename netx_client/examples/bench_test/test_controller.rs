use std::cell::RefCell;
use netxclient::*;
use crate::server::*;
use std::error::Error;


#[build(TestController)]
pub trait ITestController{
    #[tag(connect)]
    async fn connect_ok(&self)->Result<(),Box<dyn Error>>;
    #[tag(disconnect)]
    async fn disconnect(&self)->Result<(),Box<dyn Error>>;

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
    async fn connect_ok(&self) -> Result<(), Box<dyn Error>> {
        println!("Connect OK");
        Ok(())
    }
    #[inline]
    async fn disconnect(&self) -> Result<(), Box<dyn Error>> {
        println!("Disconnect");
        Ok(())
    }
}