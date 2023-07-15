mod server;
mod test_controller;

use netxclient::prelude::*;
use std::time::Instant;

use log::*;
use server::{IServer, *};
use std::error::Error;
use structopt::StructOpt;

use test_controller::TestController;

#[derive(StructOpt, Debug, Clone)]
#[structopt(name = "netx bench client")]
struct Config {
    ipaddress: String,
    thread_count: u32,
    count: u32,
}

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config: Config = Config::from_args();

    env_logger::Builder::default()
        .filter_level(LevelFilter::Info)
        .init();

    let client = NetXClient::new(
        ServerOption::new(
            format!("{}:6666", config.ipaddress),
            "".into(),
            "123123".into(),
            60000,
        ),
        DefaultSessionStore::default(),
    );

    client
        .init(TestController::new(client.clone()))
        .await
        .unwrap();
    client.connect_network().await.unwrap();

    let mut join_array = Vec::with_capacity(config.thread_count as usize);
    let start = Instant::now();
    let count = config.count;
    for id in 0..config.thread_count {
        let client = client.clone();
        let join = tokio::spawn(async move {
            let server: Box<dyn IServer> = impl_interface!(client=>IServer);
            let start = Instant::now();
            for i in 0..count {
                if let Err(er) = server.add(1, i as i32).await {
                    error!("send error:{}", er);
                }
            }
            info!("task:{} use {} ms", id, start.elapsed().as_millis());
        });

        join_array.push(join);
    }

    for j in join_array {
        j.await?;
    }

    let ms = start.elapsed().as_millis();
    let all_count = config.thread_count as u128 * config.count as u128;
    info!("all time:{} ms,TPS:{} ", ms, all_count / ms * 1000);
    Ok(())
}
