#![feature(async_closure)]

mod controller;
mod user_manager;
mod interface_client;

use netxserver::*;
use netxserver::log::LevelFilter;
use crate::controller::ImplCreateController;
use std::error::Error;

#[tokio::main]
async fn main()->Result<(),Box<dyn Error>> {
    //新建日记输出
    //create logger
    env_logger::Builder::default().filter_level(LevelFilter::Debug).init();
    // 从option.json加载配置
    // load serverinfo from option.json
    let option=
        serde_json::from_str::<ServerOption>(&std::fs::read_to_string("./option.json")?)?;

    //新建服务器,需要设置和接口实现
    //create server,need to option and impl trait [ICreateController]
    let server=
        NetXServer::new(option,
                        ImplCreateController).await;

    // 开始服务器,堵塞模式
    // start block thread
    server.start_block().await?;


    Ok(())
}
