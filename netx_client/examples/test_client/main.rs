mod server;
mod test_controller;
mod test_struct;

use std::time::Instant;
use netxclient::*;


use test_controller::TestController;
use server::*;
use crate::test_struct::{LogOn, LogOnResult};
use std::error::Error;
use log::LevelFilter;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main()->Result<(),Box<dyn Error>> {
    env_logger::Builder::default().filter_level(LevelFilter::Debug).init();

    let client =
        NetXClient::new(ServerOption::new("127.0.0.1:6666".into(),
                                          "".into(),
                                          "123123".into(),
                                            5000),
                        DefaultSessionStore::default());


    client.init(TestController::new(client.clone())).await?;
    client.connect_network().await?;
    let server:Box<dyn IServer>=impl_interface!(client=>IServer);

    //call!(@checkrun client=>800;5);
    server.print(5).await?;

   // call!(@checkrun client=>700;"joy");
     server.run_test("joy".into()).await?;

    //let x:i32=call!(client=>1003;1);
    let x=server.to_client_add_one(1).await?;
    assert_eq!(x,2);

   // call!(@checkrun client=>600;6,"my name is");
    server.print2(6,"my name is".into()).await?;

    let start = Instant::now();

    for i in 0..100000 {
       //call!(@result client=>1000;1,2);
       let _= server.add(1, i).await?;
     //  println!("{}",v);
    }

    //let r:i32=call!(client=>1005;10000);
    let r= server.recursive_test(10000).await?;
    println!("r:{} {}",r,start.elapsed().as_millis());

    let res= server.logon(LogOn{
        username: "username".to_string(),
        password: "password".to_string()
    }).await?;

    assert_eq!(res,(true,"1 Ok".to_string()));
    println!("{:?}",res);

    let res=server.logon2(("username".into(),"password".into())).await?;
    assert_eq!(res,LogOnResult{
        success: true,
        msg: "2 Ok".to_string()
    });

    println!("{:?}",res);


    let mut s="".to_string();
    std::io::stdin().read_line(&mut s)?;
    client.close().await?;

    drop(client);
    drop(server);

    let mut s="".to_string();
    std::io::stdin().read_line(&mut s)?;

    Ok(())
}

