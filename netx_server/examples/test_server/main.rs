#![feature(async_closure)]
mod test_controller;
mod client;
mod test_struct;

use std::error::Error;
use netxserver::{NetXServer, ServerOption};
use crate::test_controller::ImplCreateController;
use log::LevelFilter;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

cfg_if::cfg_if! {
if #[cfg(feature = "tls")]{

//
// OpenSSL example
//
//


use lazy_static::lazy_static;
use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod};

lazy_static! {
    pub static ref SSL: SslAcceptor = {
        let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
        acceptor
            .set_private_key_file("tests/key.pem", SslFiletype::PEM)
            .unwrap();
        acceptor
            .set_certificate_chain_file("tests/cert.pem")
            .unwrap();
        acceptor.build()
    };
}

#[tokio::main]
async fn main()->Result<(),Box<dyn Error>> {
    env_logger::Builder::default().filter_level(LevelFilter::Debug).init();
    let server=
        NetXServer::new(&SSL,ServerOption::new("0.0.0.0:6666","","123123"),
                        ImplCreateController).await;
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
    server.start_block().await?;
    Ok(())
}
}}