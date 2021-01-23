use std::error::Error;
use netxserver::*;


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
        unimplemented!()
    }

    async fn disconnect(&self) -> Result<(), Box<dyn Error>> {
        unimplemented!()
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