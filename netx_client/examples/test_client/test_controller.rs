use crate::server::*;
use anyhow::Result;
use netxclient::prelude::*;
use std::cell::RefCell;

#[build(TestController)]
pub trait ITestController {
    #[tag(connect)]
    async fn connect_ok(&self) -> Result<()>;
    #[tag(disconnect)]
    async fn disconnect(&self) -> Result<()>;
    #[tag(closed)]
    async fn closed(&self) -> Result<()>;
    #[tag(2000)]
    async fn add_one(&self, i: i32) -> Result<i32>;
    #[tag(3000)]
    async fn print(&self, i: i32);
    #[tag(4000)]
    async fn run(&self, name: Option<String>) -> Result<Option<String>>;
    #[tag(5000)]
    async fn print2(&self, i: i32, s: String) -> Result<()>;
    #[tag(2002)]
    async fn recursive_test(&self, a: i32) -> Result<i32>;
}

#[allow(dead_code)]
pub struct TestController {
    server: Box<dyn IServer>,
    name: RefCell<String>,
}

impl TestController {
    pub fn new(client: NetxClientArcDef) -> TestController {
        TestController {
            server: impl_owned_interface!(client=>IServer),
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

    #[inline]
    async fn closed(&self) -> Result<()> {
        println!("clean up world");
        Ok(())
    }

    #[inline]
    async fn add_one(&self, i: i32) -> Result<i32> {
        Ok(i + 1)
    }
    #[inline]
    async fn print(&self, i: i32) {
        println!("{}", i);
    }
    #[inline]
    async fn run(&self, name: Option<String>) -> Result<Option<String>> {
        if let Some(name) = name {
            println!("name:{:?}", name);
            (*self.name.borrow_mut()) = name.clone();
        }
        Ok(None)
    }
    #[inline]
    async fn print2(&self, i: i32, s: String) -> Result<()> {
        println!("{}-{:?}-{}", i, s, self.name.borrow());
        Ok(())
    }

    #[inline]
    async fn recursive_test(&self, mut a: i32) -> Result<i32> {
        a -= 1;
        if a > 0 {
            let x: i32 = self.server.recursive_test(a).await?;
            Ok(x)
        } else {
            Ok(a)
        }
    }
}
