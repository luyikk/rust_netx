use netxserver::*;


#[build]
pub trait IClient{
    #[tag(2001)]
    async fn message(&self,nickname:String,msg:String,to_me:bool);
}