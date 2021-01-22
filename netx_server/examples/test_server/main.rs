mod test_controller;
mod client;
mod test_struct;

use std::error::Error;
use netxserver::{NetXServer, ServerOption};
use netxserver::log::*;
use crate::test_controller::ImplCreateController;


#[tokio::main]
async fn main()->Result<(),Box<dyn Error>> {
    env_logger::Builder::default().filter_level(LevelFilter::Debug).init();
    let server=
        NetXServer::new(ServerOption::new("0.0.0.0:6666","","123123"),
                        ImplCreateController).await;
    server.start_block().await?;
    Ok(())
}
