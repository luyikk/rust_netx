use crate::client::*;
use crate::test_struct::{Foo, LogOn, LogOnResult};
use anyhow::Result;
use log::*;
use netxserver::impl_ref;
use netxserver::prelude::*;
use std::borrow::Cow;
use std::cell::Cell;
use std::sync::Arc;
use tcpserver::IPeer;

#[build(TestController)]
pub trait ITestController {
    #[tag(connect)]
    async fn connect(&self) -> anyhow::Result<()>;
    #[tag(disconnect)]
    async fn disconnect(&self) -> Result<()>;
    #[tag(closed)]
    async fn closed(&self) -> Result<()>;

    #[tag(1)]
    async fn test_base_type(
        &self,
        v: (bool, i8, u8, i16, u16, i32, u32, i64, u64, f32, f64),
    ) -> Result<(bool, i8, u8, i16, u16, i32, u32, i64, u64, f32, f64)>;
    #[tag(2)]
    async fn test_string(
        &self,
        v: (String, Option<String>, Option<String>),
    ) -> Result<(String, Option<String>, Option<String>)>;
    #[tag(3)]
    async fn test_buff(
        &self,
        v: (Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>),
    ) -> Result<(Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>)>;
    #[tag(4)]
    async fn test_struct(&self, foo: Foo) -> Result<Foo>;

    #[tag(5)]
    async fn test_base_type2(&self, v: (i64, u64, f32, f64)) -> Result<(i64, u64, f32, f64)>;
    #[tag(6)]
    async fn test_base_type3(
        &self,
        v: (bool, i8, u8, i16, u16, i32, u32),
    ) -> Result<(bool, i8, u8, i16, u16, i32, u32)>;

    #[tag(600)]
    async fn print2(&self, a: i32, b: String) -> Result<()>;
    #[tag(700)]
    async fn run_test(&self, a: Option<String>) -> Result<()>;
    #[tag(800)]
    async fn print(&self, a: i32) -> Result<()>;
    #[tag(1000)]
    async fn add(&self, a: i32, b: i32) -> Result<i32>;
    #[tag(1003)]
    async fn to_client_add_one(&self, a: i32) -> Result<i32>;
    #[tag(1005)]
    async fn recursive_test(&self, a: i32) -> Result<i32>;
    #[tag(5001)]
    async fn test(&self, msg: String, i: i32);
    #[tag(10000)]
    async fn logon(&self, info: LogOn) -> Result<(bool, String)>;
    #[tag(10001)]
    async fn logon2(&self, info: (String, String)) -> Result<LogOnResult>;
    #[tag(999)]
    async fn add_one(&self, a: i32) -> Result<i32>;
    #[tag(2500)]
    async fn get_all_count(&self) -> Result<i64>;
    #[tag(2501)]
    async fn test_cow(&self, is_str: bool) -> Result<Cow<'static, str>>;
}

pub struct TestController {
    token: NetxToken<Self>,
    count: Cell<i64>,
}

unsafe impl Send for TestController {}
unsafe impl Sync for TestController {}

impl Drop for TestController {
    #[inline]
    fn drop(&mut self) {
        info!("controller:{} is drop", self.token.get_session_id())
    }
}

#[build_impl]
impl ITestController for TestController {
    #[inline]
    async fn connect(&self) -> Result<()> {
        info!("{} is connect", self.token.get_session_id());

        if let Some(wk) = self.token.get_peer().await? {
            if let Some(peer) = wk.upgrade() {
                info!("{} addr is {} ", self.token.get_session_id(), peer.addr())
            }
        }
        Ok(())
    }

    #[inline]
    async fn disconnect(&self) -> Result<()> {
        info!("{} is disconnect", self.token.get_session_id());

        if let Some(wk) = self.token.get_peer().await? {
            if let Some(peer) = wk.upgrade() {
                info!(
                    "{} disconnect addr is {} ",
                    self.token.get_session_id(),
                    peer.addr()
                )
            }
        }

        Ok(())
    }

    #[inline]
    async fn closed(&self) -> Result<()> {
        println!("clean up world");
        Ok(())
    }

    #[inline]
    async fn test_base_type(
        &self,
        v: (bool, i8, u8, i16, u16, i32, u32, i64, u64, f32, f64),
    ) -> Result<(bool, i8, u8, i16, u16, i32, u32, i64, u64, f32, f64)> {
        println!("{:?}", v);
        Ok(v)
    }

    #[inline]
    async fn test_string(
        &self,
        v: (String, Option<String>, Option<String>),
    ) -> Result<(String, Option<String>, Option<String>)> {
        println!("{:?}", v);
        Ok(v)
    }

    async fn test_buff(
        &self,
        v: (Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>),
    ) -> Result<(Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>)> {
        println!("{:?}", v);
        Ok(v)
    }

    async fn test_struct(&self, foo: Foo) -> Result<Foo> {
        println!("{:?}", foo);
        Ok(foo)
    }

    async fn test_base_type2(&self, v: (i64, u64, f32, f64)) -> Result<(i64, u64, f32, f64)> {
        println!("{:?}", v);
        Ok(v)
    }

    async fn test_base_type3(
        &self,
        v: (bool, i8, u8, i16, u16, i32, u32),
    ) -> Result<(bool, i8, u8, i16, u16, i32, u32)> {
        println!("{:?}", v);
        Ok(v)
    }

    #[inline]
    async fn print2(&self, a: i32, b: String) -> Result<()> {
        let client: Box<dyn IClient> = impl_interface!(self.token=>IClient);
        client.print2(a, &b).await
    }

    #[inline]
    async fn run_test(&self, a: Option<String>) -> Result<()> {
        println!("{:?}", a);
        let client = impl_ref!(self.token=>IClient);
        let p = client.run(a).await?;
        println!("{:?}", p);
        Ok(())
    }

    #[inline]
    async fn print(&self, a: i32) -> Result<()> {
        println!("print {}", a);
        Ok(())
    }
    #[inline]
    async fn add(&self, a: i32, b: i32) -> Result<i32> {
        Ok(a + b)
    }
    #[inline]
    async fn to_client_add_one(&self, a: i32) -> Result<i32> {
        let client = impl_ref!(self.token=>IClient);
        client.add_one(a).await
    }
    #[inline]
    async fn recursive_test(&self, mut a: i32) -> Result<i32> {
        a -= 1;
        if a > 0 {
            let client = impl_ref!(self.token=>IClient);
            let x: i32 = client.recursive_test(a).await?;
            Ok(x)
        } else {
            Ok(a)
        }
    }

    #[inline]
    async fn test(&self, msg: String, i: i32) {
        println!("{} {}", msg, i);
    }

    #[inline]
    async fn logon(&self, info: LogOn) -> Result<(bool, String)> {
        println!("{:?}", info);
        assert_eq!(
            info,
            LogOn {
                username: "username".into(),
                password: "password".into()
            }
        );

        Ok((true, "1 Ok".to_string()))
    }

    async fn logon2(&self, (username, password): (String, String)) -> Result<LogOnResult> {
        assert_eq!(username, "username");
        assert_eq!(password, "password");

        Ok(LogOnResult {
            success: true,
            msg: "2 Ok".to_string(),
        })
    }

    #[inline]
    async fn add_one(&self, a: i32) -> Result<i32> {
        self.count.set(self.count.get() + 1);
        Ok(a + 1)
    }

    #[inline]
    async fn get_all_count(&self) -> Result<i64> {
        Ok(self.count.get())
    }

    #[inline]
    async fn test_cow(&self, is_str: bool) -> Result<Cow<'static, str>> {
        if is_str {
            Ok(Cow::Borrowed("is static str"))
        } else {
            Ok(Cow::Owned("is string owned".to_string()))
        }
    }
}

pub struct ImplCreateController;
impl ICreateController for ImplCreateController {
    type Controller = TestController;

    fn create_controller(
        &self,
        token: NetxToken<Self::Controller>,
    ) -> Result<Arc<Self::Controller>> {
        Ok(Arc::new(TestController {
            token,
            count: Cell::new(0),
        }))
    }
}
