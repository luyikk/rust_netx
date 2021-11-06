mod interface_server;
mod controller;

use std::error::Error;
use netxclient::*;
use packer::*;


use crate::interface_server::*;
use crate::controller::*;
use log::*;
use std::sync::Arc;


#[tokio::main]
async fn main()->Result<(),Box<dyn Error>> {


    //新建日记输出
    //create logger
    env_logger::Builder::default().filter_level(LevelFilter::Debug).init();

    // 从option.json加载配置
    // load serverinfo from option.json
    let serverinfo=
        serde_json::from_str::<ServerOption>(&std::fs::read_to_string("./option.json")?)?;

    // 新建netxclient,这里使用了defaultSessionStore,如果你想把session存储到文件上,你需要自己实现 trail [SessionSave]
    //create netxclient
    //use defaultSessionStore. If you want to store sessions, you need impl trail [SessionSave]
    let client =
        NetXClient::new(serverinfo,
                        DefaultSessionStore::default());
    // 初始化控制器,用来被服务器调用
    //init controller,it will be called by server
    client.init(ClientController::new(Arc::downgrade(&client))).await?;
    //尝试连接到服务器,如果这里不尝试那么调用服务器接口的时候会尝试连接
    //try connect to server
    //if not try,will try connect by call server interface
    client.connect_network().await?;

    //实现接口对象,返回接口对象
    //impl trait,return it
    let server:Box<dyn IServer>=impl_interface!(client=>IServer);

    println!("Please enter your nickname:");

    let mut nickname="".to_string();
    std::io::stdin().read_line(&mut nickname)?;
    nickname=nickname.trim().to_string();

    //调用服务器登录接口
    //call server login interface
    let res= server.login(LogOn{
        nickname
    }).await?;

    info!("{}",res.msg);
    if res.success{
        print_cmd();
        loop{
            let mut input ="".to_string();
            std::io::stdin().read_line(&mut input)?;
            input = input.trim().to_string();

            let args:Vec<&str>= input.split(&[' ','\t'][..]).collect();
            if args.len()>0{
                match args[0]{
                    "--help"=> print_cmd(),
                    "--online"=>{
                        show_all_users(&server).await?
                    },
                    "--to"=>{
                        if args.len()>=3 {
                            let msg = args[2..].join(" ").to_string();
                            if let Err(er) = server.to(args[1].to_string(), msg).await {
                                println!("error:{}",er);
                            }
                        }
                    },
                    "--ping"=>{
                        if args.len()>=2 {
                            let count={
                                if args.len()>=3{
                                    if let Ok(v)=args[2].trim().parse::<i32>(){
                                        v
                                    }else{
                                        1
                                    }
                                }else {
                                    1
                                }
                            };

                            for _ in 0..count {
                                match server.ping(args[1].to_string(),
                                                  chrono::Local::now().timestamp_nanos()).await {
                                    Ok(time) => println!("{} ms", (chrono::Local::now().timestamp_nanos() - time) as f64 / 1000000f64),
                                    Err(err) => println!("error:{}", err)
                                }
                            }
                        }
                    }
                    _=>{
                        let msg=args[0..].join(" ").to_string();
                        server.talk(msg).await?;
                    }
                }

            }


        }

    }
    Ok(())
}


//返回在线所有用户
async fn show_all_users(server: &Box<dyn IServer>) ->Result<(), Box<dyn Error>> {
    let users = server.get_users().await?;
    println!("current online users:");
    for user in users {
        println!("{}", user.nickname)
    }
    Ok(())
}

//打印命令
fn print_cmd(){
    println!("--help show all cmd");
    println!("--online show all user nickname");
    println!("--to [nickname] [msg] send message to nickname");
    println!("--ping [nickname] send ping to nickname,show ping time");
    println!("enter message to all user\r\n\r\n");
}