mod server;
mod test_controller;

use netxclient::*;
use std::time::Instant;

use log::*;
use server::*;
use std::error::Error;
use structopt::StructOpt;
use test_controller::TestController;

#[derive(StructOpt, Debug, Copy, Clone)]
#[structopt(name = "netx bench client")]
struct Config {
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
    let mut join_array = Vec::with_capacity(config.thread_count as usize);
    let start = Instant::now();
    for id in 0..config.thread_count {
        let join = tokio::spawn(async move {

            let client = {
                cfg_if::cfg_if! {
                if #[cfg(feature = "tls")]{

                        // test tls
                        use openssl::ssl::{SslMethod,SslConnector};
                        let mut connector = SslConnector::builder(SslMethod::tls()).unwrap();
                        connector.set_ca_file("tests/cert.pem").unwrap();
                        let ssl_connector=connector.build();
                        NetXClient::new(ServerOption::new("127.0.0.1:6666".into(),
                                                          "".into(),
                                                          "123123".into(),
                                                          60000),
                                                        DefaultSessionStore::default(),"localhost".to_string(),ssl_connector)

                }else if #[cfg(feature = "tcp")]{

                        // test tcp
                        NetXClient::new(ServerOption::new("127.0.0.1:6666".into(),
                                                          "".into(),
                                                          "123123".into(),
                                                          60000),
                                                        DefaultSessionStore::default())

                }}};

            client
                .init(TestController::new(client.clone()))
                .await
                .unwrap();
            client.connect_network().await.unwrap();
            let server: Box<dyn IServer> = impl_interface!(client=>IServer);

            let start = Instant::now();

            for i in 0..config.count {
                if let Err(er)= server.add(1, i as i32).await{
                    error!("{}",er);
                }
            }

            info!("task:{} use {} ms", id, start.elapsed().as_millis());

            client.close().await.unwrap();
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
