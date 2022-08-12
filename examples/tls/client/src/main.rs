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

    let server=impl_struct!(client=>IServer);
    log::info!("{}",server.hello("123").await?);
    log::info!("{}",server.get_static_str().await?);
    log::info!("{:?}",server.get_static_str2().await?);
    if let Err(err)=server.test_error().await {
        log::info!("{}",err);
    }
    Ok(())
}
