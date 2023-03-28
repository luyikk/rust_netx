mod controller;

use crate::controller::*;
use log::LevelFilter;
use netxclient::prelude::*;
use openssl::ssl::{SslConnector, SslFiletype, SslMethod};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::default()
        .filter_level(LevelFilter::Trace)
        .filter_module("mio", LevelFilter::Error)
        .init();

    #[cfg(all(feature = "use_openssl", not(feature = "use_rustls")))]
    let client = {
        let ssl_connector = {
            let mut connector = SslConnector::builder(SslMethod::tls())?;
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
        use std::fs::File;
        use std::io::BufReader;
        use std::sync::Arc;

        let cert_file = &mut BufReader::new(File::open("./ca_test/client-crt.pem")?);
        let key_file = &mut BufReader::new(File::open("./ca_test/client-key.pem")?);

        let root_store = RootCertStore::empty();
        let keys = PrivateKey(rustls_pemfile::rsa_private_keys(key_file)?.remove(0));
        let cert_chain = rustls_pemfile::certs(cert_file)
            .unwrap()
            .iter()
            .map(|c| Certificate(c.to_vec()))
            .collect::<Vec<_>>();

        let mut tls_config = ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_store)
            .with_single_cert(cert_chain, keys)
            .expect("bad certificate/key");

        tls_config
            .dangerous()
            .set_certificate_verifier(Arc::new(RustlsAcceptAnyCertVerifier));
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
    if let Err(err) = server.test_error().await {
        log::info!("{}", err);
    }
    Ok(())
}
