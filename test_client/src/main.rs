mod server;
mod test_controller;

use std::time::Instant;
use netxclient::*;
use netxclient::log::*;

use test_controller::TestController;
use server::*;


#[tokio::main]
async fn main()->Result<(),Box<dyn Error>> {
    env_logger::Builder::default().filter_level(LevelFilter::Debug).init();



    let client =
        NetXClient::new(ServerInfo::new("192.168.1.196:1006".into(),
                                        "".into(),
                                        "123123".into()),
                        DefaultSessionStore::default(),20000).await?;


    client.init(TestController::new(client.clone())).await?;
    NetXClient::connect_network(client.clone()).await?;





   let server:Box<dyn IServer>=impl_interface!(client=>IServer);
    //call!(@checkrun client=>800;5);
    // client.runcheck1(800,5).await?.check()?;
    server.print(5).await?;

    // call!(@checkrun client=>700;"joy");
    //client.runcheck1(700,"joy").await?.check()?;
    server.run_test("joy").await?;

    //let x:i32=call!(client=>1003;1);
    //let x=client.call_1(1003,1).await?.check()?.deserialize::<i32>()?;
    let x=server.to_client_add_one(1).await?;
    println!("{}",x);

    //call!(@checkrun client=>600;6,"my name is");
    //client.runcheck2(600,6,"my name is").await?.check()?;
    server.print2(6,"my name is").await?;
    let start = Instant::now();
    for _ in 0..10000 {
       //call!(@result client=>1000;1,2);
       server.add(1,2).await?;
       //client.call_2(1000,1,2).await?.check()?.deserialize::<i32>()?;

    }

    //let _:i32=call!(client=>1005;10000);

    server.recursive_test(10000).await?;

    println!("{}",start.elapsed().as_millis());
    let mut s="".to_string();
    std::io::stdin().read_line(&mut s)?;

    Ok(())
}

