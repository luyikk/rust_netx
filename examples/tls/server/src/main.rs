mod controller;

use crate::controller::ImplCreateController;
use log::LevelFilter;
use netxserver::prelude::*;

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::default()
        .filter_level(LevelFilter::Trace)
        .filter_module("mio", LevelFilter::Error)
        .init();

    // use rustls
    #[cfg(feature = "use_rustls")]
    {
        // rustls acceptor
        let tls_acceptor = {
            use std::fs::File;
            use std::io::BufReader;
            use std::sync::Arc;

            use anyhow::Context;
            use rustls_pemfile::certs;
            use rustls_pemfile::private_key;
            use tokio_rustls::rustls::pki_types::CertificateDer;
            use tokio_rustls::rustls::server::WebPkiClientVerifier;
            use tokio_rustls::rustls::{RootCertStore, ServerConfig};
            use tokio_rustls::TlsAcceptor;

            let ca_file = &mut BufReader::new(File::open("./ca_test/CA.crt")?);
            let cert_file = &mut BufReader::new(File::open("./ca_test/server-crt.pem")?);
            let key_file = &mut BufReader::new(File::open("./ca_test/server-key.pem")?);

            let keys = private_key(key_file)?.context("bad private key")?;

            let cert_chain = certs(cert_file).map(|r| r.unwrap()).collect();

            let ca_certs: Vec<CertificateDer<'static>> =
                certs(ca_file).map(|r| r.unwrap()).collect();

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
        log::info!("start server");
        server.start_block().await?;
    }

    // use openssl
    #[cfg(feature = "use_openssl")]
    {
        // openssl acceptor
        let ssl_acceptor = {
            use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod, SslVerifyMode};
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
        log::info!("start server");
        server.start_block().await?;
    }

    Ok(())
}
