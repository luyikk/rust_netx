mod controller;

use lazy_static::lazy_static;
use log::LevelFilter;
use netxserver::prelude::*;
use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod, SslVerifyMode};
use crate::controller::ImplCreateController;

// ```
//  windows install openssl:
//  git clone https://github.com/microsoft/vcpkg
//  .\vcpkg\bootstrap-vcpkg.bat
//  .\vcpkg\vcpkg integrate install
//  setx VCPKGRS_DYNAMIC 1
//  .\vcpkg\vcpkg install openssl:x64-windows-static-md
//  .\vcpkg\vcpkg install openssl:x64-windows
//  */
// ```


lazy_static! {
    pub static ref SSL: SslAcceptor = {
       let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
        acceptor.set_ca_file("./ca_test/CA.crt").unwrap();
        acceptor
            .set_private_key_file("./ca_test/server-key.pem", SslFiletype::PEM)
            .unwrap();
        acceptor
            .set_certificate_chain_file("./ca_test/server-crt.pem")
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
async fn main()->anyhow::Result<()> {
    env_logger::Builder::default()
        .filter_level(LevelFilter::Trace)
        .filter_module("mio",LevelFilter::Error)
        .init();

    let server=
        NetXServer::new(&SSL,ServerOption::new("0.0.0.0:6666","","123123"),
                        ImplCreateController).await;
    server.start_block().await?;

    Ok(())
}



