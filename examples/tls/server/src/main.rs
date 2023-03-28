mod controller;

use crate::controller::ImplCreateController;
use lazy_static::lazy_static;
use log::LevelFilter;
use netxserver::prelude::*;

use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;

use rustls_pemfile::{certs, rsa_private_keys};
use tokio_rustls::rustls::{
    server::AllowAnyAuthenticatedClient, Certificate, PrivateKey, RootCertStore, ServerConfig,
};
use tokio_rustls::TlsAcceptor;

use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod, SslVerifyMode};

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
    /// openssl acceptor
    pub static ref SSL: SslAcceptor = {
        let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
        acceptor.set_ca_file("./ca_test/CA.crt").unwrap();
        acceptor
            .set_private_key_file("./ca_test/server-key.pem", SslFiletype::PEM)
            .unwrap();
        acceptor
            .set_certificate_chain_file("./ca_test/server-crt.pem")
            .unwrap();
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
        acceptor.check_private_key().unwrap();
        acceptor.build()
    };

    /// rustls acceptor
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
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::default()
        .filter_level(LevelFilter::Trace)
        .filter_module("mio", LevelFilter::Error)
        .init();

    // use rustls
    #[cfg(feature = "use_rustls")]
    {
        let server = NetXServer::new_tls(
            &TLS,
            ServerOption::new("0.0.0.0:6666", "", "123123"),
            ImplCreateController,
        )
        .await;
        server.start_block().await?;
    }

    // use openssl
    #[cfg(feature = "use_openssl")]
    {
        let server = NetXServer::new_ssl(
            &SSL,
            ServerOption::new("0.0.0.0:6666", "", "123123"),
            ImplCreateController,
        )
        .await;
        server.start_block().await?;
    }

    Ok(())
}
