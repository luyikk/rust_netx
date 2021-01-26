use std::error::Error;
use netxserver::*;
use log::*;
use tcpserver::IPeer;
use packer::{LogOn, LogOnRes, User};
use crate::user_manager::{USERMANAGER, IUserManager};


#[build(ServerController)]
pub trait IServerController {
    #[tag(connect)]
    async fn connect(&self)->Result<(),Box<dyn Error>>;
    #[tag(disconnect)]
    async fn disconnect(&self)->Result<(),Box<dyn Error>>;

    #[tag(1000)]
    async fn login(&self,msg:LogOn)->Result<LogOnRes,Box<dyn Error>>;
    #[tag(1001)]
    async fn get_users(&self)->Result<Vec<User>,Box<dyn Error>>;
    #[tag(1002)]
    async fn talk(&self,msg:String)->Result<(),Box<dyn Error>>;

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
    async fn connect(&self) -> Result<(), Box<dyn Error>> {
        if let Some(weak) = self.token.get_peer().await? {
            if let Some(peer)=weak.upgrade(){
                info!("addr:{} session {} connect",peer.addr(),self.token.get_sessionid())
            }
        }
        Ok(())
    }
    #[inline]
    async fn disconnect(&self) -> Result<(), Box<dyn Error>> {
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
    async fn login(&self, msg: LogOn) -> Result<LogOnRes, Box<dyn Error>> {
        info!("{} is logon",msg.nickname);

        if ! USERMANAGER.check_nickname(msg.nickname.clone()).await? {
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
    async fn get_users(&self) -> Result<Vec<User>, Box<dyn Error>> {
        Ok(USERMANAGER.get_users().await)
    }
    #[inline]
    async fn talk(&self, msg: String) -> Result<(), Box<dyn Error>> {
        for user in USERMANAGER.get_users().await{
            let token=  self.token.get_token(user.sessionid).await?;
            if let Some(peer)=token{

            }
        }

        Ok(())
    }
}



pub struct ImplCreateController;
impl ICreateController for ImplCreateController{
    #[inline]
    fn create_controller(&self, token: NetxToken) -> Result<Arc<dyn IController>, Box<dyn Error>> {
        Ok(Arc::new(ServerController {
            token
        }))
    }
}