mod server;
mod test_controller;
mod test_struct;

use std::time::Instant;
use netxclient::*;
use netxclient::log::*;

use test_controller::TestController;
use server::*;
use crate::test_struct::{LogOn, LogOnResult, Flag};
use std::error::Error;


#[tokio::main]
async fn main()->Result<(),Box<dyn Error>> {
    env_logger::Builder::default().filter_level(LevelFilter::Debug).init();

    let client =
        NetXClient::new(ServerOption::new("127.0.0.1:6666".into(),
                                          "".into(),
                                          "123123".into(),
                                            5000),
                        DefaultSessionStore::default()).await?;


    client.init(TestController::new(client.clone())).await?;
    client.connect_network().await?;
    let server:Box<dyn IServer>=impl_interface!(client=>IServer);

    //call!(@checkrun client=>800;5);
    //client.runcheck1(800,5).await?.check()?;
    server.print(5).await?;

   // call!(@checkrun client=>700;"joy");
    //client.runcheck1(700,"joy").await?.check()?;
     server.run_test("joy").await?;

    //let x:i32=call!(client=>1003;1);
    //let x=client.call_1(1003,1).await?.check()?.deserialize::<i32>()?;
    let x=server.to_client_add_one(1).await?;
    assert_eq!(x,2);

   // call!(@checkrun client=>600;6,"my name is");
    //client.runcheck2(600,6,"my name is").await?.check()?;
    server.print2(6,"my name is").await?;

    let start = Instant::now();

    for i in 0..100000 {
       //call!(@result client=>1000;1,2);
       let _= server.add(1, i).await?;
     //  println!("{}",v);
      //client.call_2(1000,1,2).await?.check()?.deserialize::<i32>()?;
    }

    //let r:i32=call!(client=>1005;10000);
    //let r= server.recursive_test(10000).await?;
    println!("r:{} {}",0,start.elapsed().as_millis());

    let res= server.logon(LogOn{
        username: "username".to_string(),
        password: "password".to_string()
    }).await?;

    assert_eq!(res,LogOnResult{
        success: true,
        msg: Flag::Message("LogOn Ok".into())
    });

    let mut s="".to_string();
    std::io::stdin().read_line(&mut s)?;
    client.close().await?;

    drop(client);
    drop(server);

    let mut s="".to_string();
    std::io::stdin().read_line(&mut s)?;

    Ok(())
}

