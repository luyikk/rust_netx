use crate::server::*;
use anyhow::*;
use netxclient::prelude::*;
use std::cell::RefCell;

#[build(TestController)]
pub trait ITestController {
    #[tag(connect)]
    async fn connect_ok(&self) -> Result<()>;
    #[tag(disconnect)]
    async fn disconnect(&self) -> Result<()>;
}


#[allow(dead_code)]
pub struct TestController {
    client: NetxClientArcDef,
    server: Box<dyn IServer>,
    name: RefCell<String>,
}

impl TestController {
    pub fn new(client: NetxClientArcDef) -> TestController {
        TestController {
            server: impl_interface!(client=>IServer),
            client,
            name: RefCell::new("".to_string()),
        }
    }
}

unsafe impl Sync for TestController {}
unsafe impl Send for TestController {}

#[build_impl]
impl ITestController for TestController {
    #[inline]
    async fn connect_ok(&self) -> Result<()> {
        println!("Connect OK");
        Ok(())
    }
    #[inline]
    async fn disconnect(&self) -> Result<()> {
        println!("Disconnect");
        Ok(())
    }
}
