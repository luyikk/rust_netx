mod server;
mod test_controller;

use netxclient::prelude::*;
use std::time::Instant;

use log::*;
use server::{IServer, *};

use structopt::StructOpt;

use anyhow::Result;
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
async fn main() -> Result<()> {
    let config: Config = Config::from_args();

    env_logger::Builder::default()
        .filter_level(LevelFilter::Info)
        .init();

    let mut join_array = Vec::with_capacity(config.thread_count as usize);
    let start = Instant::now();
    let count = config.count;
    for id in 0..config.thread_count {
        let ipaddress = config.ipaddress.clone();
        let join = tokio::spawn(async move {
            let client = {
                cfg_if::cfg_if! {
                if #[cfg(feature = "use_openssl")]{
                    // test tls
                    use openssl::ssl::{SslMethod,SslConnector,SslFiletype};
                    let mut connector = SslConnector::builder(SslMethod::tls()).unwrap();
                    connector.set_ca_file("./ca_test/CA.crt").unwrap();
                    connector.set_private_key_file("./ca_test/client-key.pem", SslFiletype::PEM).unwrap();
                    connector.set_certificate_chain_file("./ca_test/client-crt.pem").unwrap();
                    connector.check_private_key().unwrap();
                    let ssl_connector=connector.build();
                    NetXClient::new_ssl(ServerOption::new(format!("{}:6666",ipaddress),
                                                      "".into(),
                                                      "123123".into(),
                                                      6000),
                                                    DefaultSessionStore::default(),"localhost".to_string(),ssl_connector)

                }else if #[cfg(feature = "use_rustls")]{
                    use std::sync::Arc;
                    use std::io::BufReader;
                    use std::fs::File;
                    use std::convert::TryFrom;
                    use tokio_rustls::rustls::ClientConfig;
                    use rustls_pemfile::{certs, private_key};
                    use tokio_rustls::rustls::pki_types::ServerName;



                    let cert_file = &mut BufReader::new(File::open("./ca_test/client-crt.pem").unwrap());
                    let key_file = &mut BufReader::new(File::open("./ca_test/client-key.pem").unwrap());

                    let keys = private_key(key_file).unwrap().unwrap();
                    let cert_chain = certs(cert_file).map(|r| r.unwrap()).collect();

                    let tls_config = ClientConfig::builder()
                        .dangerous()
                        .with_custom_certificate_verifier(Arc::new(RustlsAcceptAnyCertVerifier))
                        .with_client_auth_cert(cert_chain, keys)
                        .expect("bad certificate/key");

                    let connector=tokio_rustls::TlsConnector::from(Arc::new(tls_config));

                    NetXClient::new_tls(ServerOption::new(format!("{}:6666",ipaddress),
                                                          "".into(),
                                                          "123123".into(),
                                                          5000),
                                                        DefaultSessionStore::default(),ServerName::try_from("localhost").unwrap(),connector)
                }else{
                    // test tcp
                    NetXClient::new(ServerOption::new(format!("{}:6666",ipaddress),
                                                      "".into(),
                                                      "123123".into(),
                                                      60000),
                                                    DefaultSessionStore::default())
                }}
            };

            client.init(TestController::new(client.clone())).await;

            client.connect_network().await.unwrap();
            let server: Box<dyn IServer> = impl_interface!(client=>IServer);
            let start = Instant::now();
            for i in 0..count {
                if let Err(er) = server.add(1, i as i32).await {
                    error!("send error:{}", er);
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

    let ms = start.elapsed().as_secs_f32();
    let all_count = config.thread_count as f32 * config.count as f32;
    info!("all time:{} s,TPS:{} ", ms, (all_count / ms) as i64);
    Ok(())
}
