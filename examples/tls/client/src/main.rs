mod controller;

use log::LevelFilter;
use netxclient::prelude::*;
use openssl::ssl::{SslMethod, SslConnector, SslFiletype};
use crate::controller::*;


#[tokio::main]
async fn main()->anyhow::Result<()> {
    env_logger::Builder::default()
        .filter_level(LevelFilter::Trace)
        .filter_module("mio",LevelFilter::Error)
        .init();


    let ssl_connector= {
        let mut connector = SslConnector::builder(SslMethod::tls())?;
        connector.set_ca_file("./ca_test/CA.crt")?;
        connector.set_private_key_file("./ca_test/client-key.pem", SslFiletype::PEM)?;
        connector.set_certificate_chain_file("./ca_test/client-crt.pem")?;
        connector.check_private_key()?;
        connector.build()
    };

    let client= NetXClient::new(ServerOption::new("127.0.0.1:6666".into(),
                                                  "".into(),
                                                  "123123".into(),
                                                  5000),
                                DefaultSessionStore::default(),"localhost".to_string(),ssl_connector);

    client.connect_network().await?;

    let server:Box<dyn IServer>=impl_interface!(client=>IServer);
    log::info!("{}",server.hello("123").await?);

    Ok(())
}
