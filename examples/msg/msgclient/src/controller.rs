use netxclient::*;

#[build(ClientController)]
pub trait IClientController {
    #[tag(2001)]
    async fn message(&self,nickname:String,msg:String,to_me:bool);
}

type Client=Arc<Actor<NetXClient<DefaultSessionStore>>>;

pub struct ClientController{
    client:Client
}

impl IClientController for ClientController{

    #[inline]
    async fn message(&self, nickname: String, msg: String, to_me: bool) {
        match to_me {
            true => println!("{} -> {}", nickname, msg),
            false => println!("{}:{}", nickname, msg)
        }
    }
}