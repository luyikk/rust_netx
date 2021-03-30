mod server;
mod test_controller;

use std::time::Instant;
use netxclient::*;

use test_controller::TestController;
use server::*;
use std::error::Error;
use log::*;
use structopt::StructOpt;

#[derive(StructOpt, Debug,Copy, Clone)]
#[structopt(name = "netx bench client")]
struct Config{
    thread_count:u32,
    count:u32
}

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main()->Result<(),Box<dyn Error>> {

    let config:Config = Config::from_args();

    env_logger::Builder::default().filter_level(LevelFilter::Info).init();
    let mut join_array =Vec::with_capacity(config.thread_count as usize);
    let start=Instant::now();
    for id in 0..config.thread_count {
        let join = tokio::spawn(async move {
            let client =
                NetXClient::new(ServerOption::new("127.0.0.1:6666".into(),
                                                  "".into(),
                                                  "123123".into(),
                                                  10000),
                                DefaultSessionStore::default()).await.unwrap();


            client.init(TestController::new(client.clone())).await.unwrap();
            client.connect_network().await.unwrap();
            let server: Box<dyn IServer> = impl_interface!(client=>IServer);


            let start = Instant::now();

            for i in 0..config.count {
                let _ = server.add(1, i as i32).await.unwrap();
            }

            info!("task:{} use {} ms", id,start.elapsed().as_millis());

            client.close().await.unwrap();
        });

        join_array.push(join);
    }

    for j in join_array {
        j.await?;
    }

    let ms=start.elapsed().as_millis();
    let all_count =config.thread_count as u128 * config.count as u128;

    info!("all time:{} ms,TPS:{} ", ms, all_count /ms*1000);
    Ok(())
}
