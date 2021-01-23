use std::error::Error;
use netxserver::*;
use log::*;

#[build(MsgServer)]
pub trait IMsgServer{
    #[tag(connect)]
    async fn connect(&self)->Result<(),Box<dyn Error>>;
    #[tag(disconnect)]
    async fn disconnect(&self)->Result<(),Box<dyn Error>>;
}

pub struct MsgServer{
    token:NetxToken
}

#[build_impl]
impl IMsgServer for MsgServer{
    async fn connect(&self) -> Result<(), Box<dyn Error>> {
        if let Some(weak) = self.token.get_peer().await? {
            if let Some(peer)=weak.upgrade(){
                info!("addr:{} session {} connect",peer.addr(),self.token.get_sessionid())
            }
        }
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), Box<dyn Error>> {
        if let Some(weak) = self.token.get_peer().await? {
            if let Some(peer)=weak.upgrade(){
                info!("addr:{} session {} disconnect",peer.addr(),self.token.get_sessionid())
            }
        }
        Ok(())
    }
}



pub struct ImplCreateController;
impl ICreateController for ImplCreateController{
    fn create_controller(&self, token: NetxToken) -> Result<Arc<dyn IController>, Box<dyn Error>> {
        Ok(Arc::new(MsgServer{
            token
        }))
    }
}