mod client;
mod test_controller;
mod test_struct;

use crate::test_controller::ImplCreateController;
use log::LevelFilter;
use netxserver::prelude::*;
use std::error::Error;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

cfg_if::cfg_if! {
if #[cfg(feature = "tls")]{

//
// OpenSSL example
//
//


use lazy_static::lazy_static;
use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod, SslVerifyMode};

lazy_static! {
    pub static ref SSL: SslAcceptor = {
       let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
        acceptor.set_ca_file("../ca_test/CA.crt").unwrap();
        acceptor
            .set_private_key_file("../ca_test/server-key.pem", SslFiletype::PEM)
            .unwrap();
        acceptor
            .set_certificate_chain_file("../ca_test/server-crt.pem")
            .unwrap();
        acceptor.set_verify_callback(SslVerifyMode::PEER|SslVerifyMode::FAIL_IF_NO_PEER_CERT,|ok,cert|{
            if !ok{
                if let Some(cert)= cert.current_cert(){
                   println!("subject info {:?}",cert.subject_name());
                   println!("issuer info {:?}",cert.issuer_name());
                }
            }
            ok
        });
        acceptor.check_private_key().unwrap();
        acceptor.build()
    };
}

#[tokio::main]
async fn main()->Result<(),Box<dyn Error>> {
    env_logger::Builder::default().filter_level(LevelFilter::Debug).init();
    let server=
        NetXServer::new(&SSL,ServerOption::new("0.0.0.0:6666","","123123"),
                        ImplCreateController).await;
    log::info!("start");
    server.start_block().await?;
    Ok(())
}


}else if #[cfg(feature = "tcp")]{
//
// TCP example
//
//

#[tokio::main]
async fn main()->Result<(),Box<dyn Error>> {
    env_logger::Builder::default().filter_level(LevelFilter::Debug).init();
    let server=
        NetXServer::new(ServerOption::new("0.0.0.0:6666","","123123"),
                        ImplCreateController).await;
    log::info!("start");
    server.start_block().await?;
    Ok(())
}
}}
