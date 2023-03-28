mod controller;

use crate::controller::*;
use log::LevelFilter;
use netxclient::prelude::*;


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
        use std::convert::TryFrom;
        use tokio_rustls::rustls::{Certificate, PrivateKey,ClientConfig,ServerName};
        use rustls_pemfile::{certs, rsa_private_keys};

        let cert_file = &mut BufReader::new(File::open("./ca_test/client-crt.pem")?);
        let key_file = &mut BufReader::new(File::open("./ca_test/client-key.pem")?);

        let keys = PrivateKey(rsa_private_keys(key_file)?.remove(0));
        let cert_chain = certs(cert_file)
            .unwrap()
            .iter()
            .map(|c| Certificate(c.to_vec()))
            .collect::<Vec<_>>();

        let tls_config = ClientConfig::builder()
            .with_safe_defaults()
            .with_custom_certificate_verifier(Arc::new(RustlsAcceptAnyCertVerifier))
            .with_single_cert(cert_chain, keys)
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
    if let Err(err) = server.test_error().await {
        log::info!("{}", err);
    }
    Ok(())
}
