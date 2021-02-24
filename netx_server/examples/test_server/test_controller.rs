use std::error::Error;
use std::sync::Arc;
use netxserver::*;
use crate::client::*;
use log::*;
use tcpserver::IPeer;
use crate::test_struct::{LogOn, LogOnResult};

use std::cell::Cell;



#[build(TestController)]
pub trait ITestController{
    #[tag(connect)]
    async fn connect(&self)->Result<(),Box<dyn Error>>;
    #[tag(disconnect)]
    async fn disconnect(&self)->Result<(),Box<dyn Error>>;
    #[tag(600)]
    async fn print2(&self,a:i32,b:String)->Result<(),Box<dyn Error>>;
    #[tag(700)]
    async fn run_test(&self,a:String)->Result<(),Box<dyn Error>>;
    #[tag(800)]
    async fn print(&self,a:i32)->Result<(),Box<dyn Error>>;
    #[tag(1000)]
    async fn add(&self,a:i32,b:i32)->Result<i32,Box<dyn Error>>;
    #[tag(1003)]
    async fn to_client_add_one(&self,a:i32)->Result<i32,Box<dyn Error>>;
    #[tag(1005)]
    async fn recursive_test(&self, a:i32)->Result<i32,Box<dyn Error>>;
    #[tag(5001)]
    async fn test(&self,msg:String,i:i32);
    #[tag(10000)]
    async fn logon(&self,info:LogOn)->Result<(bool,String),Box<dyn Error>>;
    #[tag(10001)]
    async fn logon2(&self,info:(String,String))->Result<LogOnResult,Box<dyn Error>>;

    #[tag(999)]
    async fn add_one(&self,a:i32)->Result<i32,Box<dyn Error>>;

    #[tag(2500)]
    async fn get_all_count(&self)->Result<i64,Box<dyn Error>>;
}

pub struct TestController{
    token:NetxToken,
    client:Box<dyn IClient>,
    count:Cell<i64>
}

unsafe impl Send for TestController{}
unsafe impl Sync for TestController{}

impl Drop for TestController{
    #[inline]
    fn drop(&mut self) {
        info!("controller:{} is drop",self.token.get_sessionid())
    }
}



#[build_impl]
impl ITestController for TestController{
    #[inline]
    async fn connect(&self) -> Result<(), Box<dyn Error>> {
       info!("{} is connect",self.token.get_sessionid());

       if let Some(wk)= self.token.get_peer().await?{
           if let Some(peer)=wk.upgrade(){
               info!("{} addr is {} ",self.token.get_sessionid(),peer.addr())
           }
       }
       Ok(())
    }

    #[inline]
    async fn disconnect(&self) -> Result<(), Box<dyn Error>> {
        info!("{} is disconnect",self.token.get_sessionid());

        if let Some(wk)= self.token.get_peer().await?{
            if let Some(peer)=wk.upgrade(){
                info!("{} disconnect addr is {} ",self.token.get_sessionid(),peer.addr())
            }
        }

        Ok(())
    }

    #[inline]
    async fn print2(&self, a: i32, b:String) -> Result<(), Box<dyn Error>> {
        self.client.print2(a,b).await
    }

    #[inline]
    async fn run_test(&self, a: String) -> Result<(), Box<dyn Error>> {
        self.client.run(a.to_string()).await
    }

    #[inline]
    async fn print(&self, a: i32) -> Result<(), Box<dyn Error>> {
        println!("print {}",a);
        Ok(())
    }
    #[inline]
    async fn add(&self, a: i32, b: i32) -> Result<i32, Box<dyn Error>> {
        Ok(a+b)
    }
    #[inline]
    async fn to_client_add_one(&self, a: i32) -> Result<i32, Box<dyn Error>> {
        self.client.add_one(a).await
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
    async fn test(&self, msg: String,i:i32) {
        println!("{} {}",msg,i);
    }

    #[inline]
    async fn logon(&self, info: LogOn) -> Result<(bool,String), Box<dyn Error>> {
        assert_eq!(info,LogOn{
            username:"username".into(),
            password:"password".into()
        });

        Ok((true,"1 Ok".to_string()))
    }

    async fn logon2(&self, (username,password): (String, String)) -> Result<LogOnResult, Box<dyn Error>> {
        assert_eq!(username,"username");
        assert_eq!(password,"password");

        Ok(LogOnResult{
            success: true,
            msg: "2 Ok".to_string()
        })
    }

    #[inline]
    async fn add_one(&self, a: i32) -> Result<i32, Box<dyn Error>> {
        self.count.set(self.count.get()+1);
        Ok(a+1)
    }

    #[inline]
    async fn get_all_count(&self) -> Result<i64, Box<dyn Error>> {
        Ok(self.count.get())
    }
}

pub struct ImplCreateController;
impl ICreateController for ImplCreateController {
    fn create_controller(&self, token: NetxToken) -> Result<Arc<dyn IController>, Box<dyn Error>> {
       Ok(Arc::new(TestController{
           client: impl_interface!(token=>IClient),
           token,
           count:Cell::new(0)
       }))
    }
}

