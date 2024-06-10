mod controller;

use crate::controller::*;

use log::LevelFilter;
use netxclient::prelude::*;
use openssl::ssl::SslVerifyMode;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::default()
        .filter_level(LevelFilter::Trace)
        .filter_module("mio", LevelFilter::Error)
        .init();

    #[cfg(all(feature = "use_openssl", not(feature = "use_rustls")))]
    let client = {
        use openssl::ssl::{SslConnector, SslFiletype, SslMethod};
        let ssl_connector = {
            let mut connector = SslConnector::builder(SslMethod::tls())?;
            connector.set_verify(SslVerifyMode::PEER);
            connector.set_ca_file("./ca_test/CA.crt")?;
            connector.set_private_key_file("./ca_test/client-key.pem", SslFiletype::PEM)?;
            connector.set_certificate_chain_file("./ca_test/client-crt.pem")?;
            connector.check_private_key()?;
            connector.build()
        };

        NetXClient::new_ssl(
            ServerOption::new("127.0.0.1:6666".into(), "".into(), "123123".into(), 5000),
            DefaultSessionStore::default(),
            "localhost".to_string(),
            ssl_connector,
        )
    };

    #[cfg(all(feature = "use_rustls", not(feature = "use_openssl")))]
    let client = {
        use anyhow::Context;
        use rustls_pemfile::{certs, private_key};
        use std::convert::TryFrom;
        use std::fs::File;
        use std::io::BufReader;
        use std::sync::Arc;
        use tokio_rustls::rustls::pki_types::ServerName;
        use tokio_rustls::rustls::ClientConfig;

        let cert_file = &mut BufReader::new(File::open("./ca_test/client-crt.pem")?);
        let key_file = &mut BufReader::new(File::open("./ca_test/client-key.pem")?);

        let keys = private_key(key_file)?.context("bad private key")?;

        let cert_chain = certs(cert_file).map(|r| r.unwrap()).collect();

        let tls_config = ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(RustlsAcceptAnyCertVerifier))
            .with_client_auth_cert(cert_chain, keys)
            .expect("bad certificate/key");

        let connector = tokio_rustls::TlsConnector::from(Arc::new(tls_config));

        NetXClient::new_tls(
            ServerOption::new("127.0.0.1:6666".into(), "".into(), "123123".into(), 5000),
            DefaultSessionStore::default(),
            ServerName::try_from("localhost")?,
            connector,
        )
    };

    let server = impl_ref!(client=>IServer);
    log::info!("{}", server.hello("123").await?);
    log::info!("{}", server.get_static_str().await?);
    log::info!("{:?}", server.get_static_str2().await?);
    let data = vec![1; 0x20000];
    log::info!("{:?}", server.test_buff(&data).await?);
    if let Err(err) = server.test_error().await {
        log::info!("{}", err);
    }
    Ok(())
}
