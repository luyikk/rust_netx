use anyhow::Result;
use netxclient::prelude::*;
use std::sync::Weak;

//客户端接口和实现,用来被服务器调用
//Client interface and implementation, used to be called by the server
#[build(ClientController)]
pub trait IClientController {
    // connect 和 disconnect 在你需要的时候你可以实现它
    // connect and disconnect you can implement it when you need it

    #[tag(2001)]
    async fn message(&self, nickname: String, msg: String, to_me: bool);

    #[tag(3001)]
    async fn ping(&self, nickname: String, time: i64) -> Result<i64>;
}

pub type Client = Weak<Actor<NetXClient<DefaultSessionStore>>>;

pub struct ClientController {
    #[allow(dead_code)]
    client: Client, //store client,be used
}

impl ClientController {
    pub fn new(client: Client) -> ClientController {
        ClientController { client }
    }
}

#[build_impl]
impl IClientController for ClientController {
    //打印用户消息
    // print user message
    #[inline]
    async fn message(&self, nickname: String, msg: String, to_me: bool) {
        match to_me {
            true => println!("{} -> {}", nickname, msg),
            false => println!("{}:{}", nickname, msg),
        }
    }

    #[inline]
    async fn ping(&self, _nickname: String, time: i64) -> Result<i64> {
        Ok(time)
    }
}
