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
if #[cfg(feature = "use_openssl")]{

    //
    // OpenSSL example
    //

    use lazy_static::lazy_static;
    use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod, SslVerifyMode};

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
    async fn main()->Result<(),Box<dyn Error>> {
        env_logger::Builder::default().filter_level(LevelFilter::Debug).init();
        let server=
            NetXServer::new_ssl(&SSL,ServerOption::new("0.0.0.0:6666","","123123"),
                            ImplCreateController).await;
        log::info!("start");
        server.start_block().await?;
        Ok(())
    }

}else if #[cfg(feature = "use_rustls")]{
    //
    // Rustls example
    //
    use lazy_static::lazy_static;
    use std::fs::File;
    use std::io::BufReader;
    use std::sync::Arc;
    use rustls_pemfile::{certs, rsa_private_keys};
    use tokio_rustls::rustls::{server::AllowAnyAuthenticatedClient,Certificate, PrivateKey, ServerConfig,RootCertStore};
    use tokio_rustls::TlsAcceptor;

    lazy_static! {
         pub static ref TLS: TlsAcceptor = {
            let ca_file= &mut BufReader::new(File::open("./ca_test/CA.crt").unwrap());
            let cert_file = &mut BufReader::new(File::open("./ca_test/server-crt.pem").unwrap());
            let key_file = &mut BufReader::new(File::open("./ca_test/server-key.pem").unwrap());

            let keys = PrivateKey(rsa_private_keys(key_file).unwrap().remove(0));
            let cert_chain = certs(cert_file)
                .unwrap()
                .iter()
                .map(|c| Certificate(c.to_vec()))
                .collect::<Vec<_>>();

            let ca_certs= certs(ca_file).unwrap();
            let mut client_auth_roots = RootCertStore::empty();
            client_auth_roots.add_parsable_certificates(&ca_certs);

            let client_auth = AllowAnyAuthenticatedClient::new(client_auth_roots);
            let tls_config = ServerConfig::builder()
                .with_safe_defaults()
                .with_client_cert_verifier(client_auth)
                .with_single_cert(cert_chain, keys)
                .unwrap();

            TlsAcceptor::from(Arc::new(tls_config))
        };
    }

    #[tokio::main]
    async fn main()->Result<(),Box<dyn Error>> {
        env_logger::Builder::default().filter_level(LevelFilter::Debug).init();
        let server=
            NetXServer::new_tls(&TLS,ServerOption::new("0.0.0.0:6666","","123123"),
                            ImplCreateController).await;
        log::info!("start");
        server.start_block().await?;
        Ok(())
    }
}else{
    //
    // TCP example
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
