use netxserver::prelude::*;
use log::*;
use tcpserver::IPeer;
use packer::{LogOn, LogOnRes, User};
use crate::user_manager::{USERMANAGER, IUserManager};
use crate::interface_client::*;
use anyhow::{bail, Context, Result};
use std::sync::Arc;


//实现服务器接口和业务,用来给服务器调用
//Realize the server interface and business, used to call the server
#[build(ServerController)]
pub trait IServerController {
    // connect 和 disconnect 在你需要的时候你可以实现它
    // connect and disconnect you can implement it when you need it
    #[tag(connect)]
    async fn connect(&self)->Result<()>;
    #[tag(disconnect)]
    async fn disconnect(&self)->Result<()>;

    #[tag(1000)]
    async fn login(&self,msg:LogOn)->Result<LogOnRes>;
    #[tag(1001)]
    async fn get_users(&self)->Result<Vec<User>>;
    #[tag(1002)]
    async fn talk(&self,msg:String)->Result<()>;
    #[tag(1003)]
    async fn to(&self,target_nickname:String,msg:String)->Result<()>;
    #[tag(1004)]
    async fn ping(&self,target_nickname:String,time:i64)->Result<i64>;
}

pub struct ServerController {
    token:NetxToken
}

impl Drop for ServerController {
    fn drop(&mut self) {
        let sessionid=self.token.get_sessionid();
        tokio::spawn(async move{
            match USERMANAGER.remove(sessionid).await{
                Ok(_)=>info!("remove user {} ok",sessionid),
                Err(er)=>error!("remove user {} err:{}",sessionid, er)
            }
        });
    }
}

#[build_impl]
impl IServerController for ServerController {
    #[inline]
    async fn connect(&self) -> Result<()> {
        if let Some(weak) = self.token.get_peer().await? {
            if let Some(peer)=weak.upgrade(){
                info!("addr:{} session {} connect",peer.addr(),self.token.get_sessionid())
            }
        }
        Ok(())
    }
    #[inline]
    async fn disconnect(&self) -> Result<()> {
        let user= USERMANAGER.find(self.token.get_sessionid()).await?;
        if let Some(weak) = self.token.get_peer().await? {
            if let Some(peer)=weak.upgrade(){
                if let Some(user)=user{
                    info!("nickname:{} addr:{} session {} disconnect",user.nickname, peer.addr(),self.token.get_sessionid())
                }else{
                    info!("addr:{} session {} disconnect",peer.addr(),self.token.get_sessionid())
                }

            }
        }
        Ok(())
    }

    #[inline]
    async fn login(&self, msg: LogOn) -> Result<LogOnRes> {
        info!("{} is logon",msg.nickname);

        if USERMANAGER.check_nickname(msg.nickname.clone()).await? {
            USERMANAGER.add(User {
                nickname: msg.nickname,
                sessionid: self.token.get_sessionid()
            }).await?;

            Ok(LogOnRes {
                success: true,
                msg: "login ok".to_string()
            })

        }else{
            Ok(LogOnRes {
                success: false,
                msg: "nickname is use".to_string()
            })
        }
    }

    #[inline]
    async fn get_users(&self) -> Result<Vec<User>> {
        Ok(USERMANAGER.get_users().await)
    }
    #[inline]
    async fn talk(&self, msg: String) -> Result<()> {
        let current=USERMANAGER.find(self.token.get_sessionid()).await?;
        if let Some(current_user)=current {
            for user in USERMANAGER.get_users().await {
                if user.sessionid!=current_user.sessionid {
                    let token = self.token.get_token(user.sessionid).await?;
                    if let Some(token) = token {
                        let peer: Box<dyn IClient> = impl_interface!(token=>IClient);
                        peer.message(current_user.nickname.clone(), msg.clone(), false).await;
                    }
                }
            }
            Ok(())

        }else{
            bail!("not login")
        }
    }
    #[inline]
    async fn to(&self, target_nickname: String, msg: String) -> Result<()> {
        let current_user = USERMANAGER.find(self.token.get_sessionid()).await?.
            context("not login")?;
        let target_user = USERMANAGER.find_by_nickname(target_nickname.clone()).await?.
            with_context(||format!("not found {}", target_nickname))?;
        let token = self.token.get_token(target_user.sessionid).await?.
            with_context(||format!("not found {}", target_nickname))?;

        let peer: Box<dyn IClient> = impl_interface!(token=>IClient);
        peer.message(current_user.nickname.clone(), msg, true).await;

        Ok(())
    }
    #[inline]
    async fn ping(&self,target_nickname:String,time:i64)->Result<i64> {
        let current_user = USERMANAGER.find(self.token.get_sessionid()).await?.
            context("not login")?;
        let target_user = USERMANAGER.find_by_nickname(target_nickname.clone()).await?.
            with_context(||format!("not found {}", target_nickname))?;
        let token = self.token.get_token(target_user.sessionid).await?.
            with_context(||format!("not found {}", target_nickname))?;
        let peer: Box<dyn IClient> = impl_interface!(token=>IClient);
        Ok(peer.ping(current_user.nickname.clone(), time).await?)
    }
}


// create struct impl ICreateController,used to create controllers for each user
// 新建一个结构,实现ICreateController,用来为每个用户创建控制器
pub struct ImplCreateController;
impl ICreateController for ImplCreateController{
    #[inline]
    fn create_controller(&self, token: NetxToken) -> Result<Arc<dyn IController>> {
        Ok(Arc::new(ServerController {
            token
        }))
    }
}