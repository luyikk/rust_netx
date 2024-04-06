mod client;
mod test_controller;
mod test_struct;

use crate::test_controller::ImplCreateController;
use anyhow::Result;
use log::LevelFilter;
use netxserver::prelude::*;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

//
// TCP example
//
#[cfg(all(not(feature = "use_rustls"), not(feature = "use_openssl")))]
#[tokio::main]
async fn main() -> Result<()> {
    use anyhow::Context;

    env_logger::Builder::default()
        .filter_level(LevelFilter::Debug)
        .init();
    let server = NetXServer::new(
        ServerOption::new("0.0.0.0:6666", "", "123123"),
        ImplCreateController,
    )
    .await;
    log::info!("start");
    let token_manager = server.get_token_manager().upgrade().context("?")?;
    assert!(token_manager.get_token(1).await.is_none());
    server.start_block().await?;
    Ok(())
}

//
// OpenSSL example
//
#[cfg(all(feature = "use_openssl", not(feature = "use_rustls")))]
#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::default()
        .filter_level(LevelFilter::Debug)
        .init();

    use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod, SslVerifyMode};

    let ssl_acceptor = {
        let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls())?;
        acceptor.set_ca_file("./ca_test/CA.crt")?;
        acceptor.set_private_key_file("./ca_test/server-key.pem", SslFiletype::PEM)?;

        acceptor.set_certificate_chain_file("./ca_test/server-crt.pem")?;

        acceptor.set_verify_callback(
            SslVerifyMode::PEER | SslVerifyMode::FAIL_IF_NO_PEER_CERT,
            |ok, cert| {
                if !ok {
                    if let Some(cert) = cert.current_cert() {
                        println!("subject info {:?}", cert.subject_name());
                        println!("issuer info {:?}", cert.issuer_name());
                    }
                }
                ok
            },
        );
        acceptor.check_private_key()?;
        acceptor.build()
    };
    let ssl_acceptor = Box::leak(Box::new(ssl_acceptor));
    let server = NetXServer::new_ssl(
        ssl_acceptor,
        ServerOption::new("0.0.0.0:6666", "", "123123"),
        ImplCreateController,
    )
    .await;
    log::info!("start");
    server.start_block().await?;
    Ok(())
}

//
// Rustls example
//

#[cfg(all(feature = "use_rustls", not(feature = "use_openssl")))]
#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::default()
        .filter_level(LevelFilter::Debug)
        .init();

    let tls_acceptor = {
        use anyhow::Context;
        use rustls_pemfile::{certs, private_key};
        use std::fs::File;
        use std::io::BufReader;
        use std::sync::Arc;
        use tokio_rustls::rustls::pki_types::CertificateDer;
        use tokio_rustls::rustls::server::WebPkiClientVerifier;
        use tokio_rustls::rustls::{RootCertStore, ServerConfig};
        use tokio_rustls::TlsAcceptor;

        let ca_file = &mut BufReader::new(File::open("./ca_test/CA.crt").unwrap());
        let cert_file = &mut BufReader::new(File::open("./ca_test/server-crt.pem").unwrap());
        let key_file = &mut BufReader::new(File::open("./ca_test/server-key.pem").unwrap());

        let keys = private_key(key_file)?.context("bad private key")?;

        let cert_chain = certs(cert_file).map(|r| r.unwrap()).collect();

        let ca_certs: Vec<CertificateDer<'static>> = certs(ca_file).map(|r| r.unwrap()).collect();

        let mut client_auth_roots = RootCertStore::empty();
        client_auth_roots.add_parsable_certificates(ca_certs);
        let client_auth_roots = Arc::new(client_auth_roots);

        let client_auth = WebPkiClientVerifier::builder(client_auth_roots)
            .build()
            .unwrap();

        let tls_config = ServerConfig::builder()
            .with_client_cert_verifier(client_auth)
            .with_single_cert(cert_chain, keys)?;

        TlsAcceptor::from(Arc::new(tls_config))
    };

    let tls_acceptor = Box::leak(Box::new(tls_acceptor));
    let server = NetXServer::new_tls(
        tls_acceptor,
        ServerOption::new("0.0.0.0:6666", "", "123123"),
        ImplCreateController,
    )
    .await;
    log::info!("start");
    server.start_block().await?;
    Ok(())
}
