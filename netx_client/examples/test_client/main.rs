mod server;
mod test_controller;
mod test_struct;

use crate::test_struct::{Foo, LogOn, LogOnResult};
use log::LevelFilter;
use netxclient::impl_ref;
use netxclient::prelude::*;
use server::*;
use std::error::Error;
use std::time::Instant;
use test_controller::TestController;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::default()
        .filter_level(LevelFilter::Debug)
        .init();

    let client = {
        cfg_if::cfg_if! {
        if #[cfg(feature = "tls")]{

                // test tls
                use openssl::ssl::{SslMethod,SslConnector,SslFiletype};
                let mut connector = SslConnector::builder(SslMethod::tls())?;
                connector.set_ca_file("../ca_test/CA.crt")?;
                connector.set_private_key_file("../ca_test/client-key.pem", SslFiletype::PEM)?;
                connector.set_certificate_chain_file("../ca_test/client-crt.pem")?;
                connector.check_private_key()?;
                let ssl_connector=connector.build();
                NetXClient::new(ServerOption::new("127.0.0.1:6666".into(),
                                                  "".into(),
                                                  "123123".into(),
                                                  5000),
                                                DefaultSessionStore::default(),"localhost".to_string(),ssl_connector)

        }else if #[cfg(feature = "tcp")]{

                // test tcp
                NetXClient::new(ServerOption::new("127.0.0.1:6666".into(),
                                                  "".into(),
                                                  "123123".into(),
                                                  5000),
                                                DefaultSessionStore::default())

        }}
    };

    client.init(TestController::new(client.clone())).await?;
    client.connect_network().await?;
    client.connect_network().await?;

    let server = impl_ref!(client=>IServer);

    //test base type
    {
        let value = (true, 2, 3, 4, 5, 6, 7, 8, 9, 1.1f32, 2.2222f64);
        let r = server.test_base_type(value).await?;
        assert_eq!(value, r);
    }

    //test string
    {
        let value = ("test".to_string(), Some("test".to_string()), None);
        //test base type
        let r = server.test_string(&value).await?;
        assert_eq!(value, r);
    }
    //test buff
    {
        let value = (
            "test".as_bytes().to_vec(),
            Some("test".as_bytes().to_vec()),
            None,
        );
        //test base type
        let r = server.test_buff(&value).await?;
        assert_eq!(value, r);
    }
    //test struct
    {
        let value = Foo::default();
        //test base type
        let r = server.test_struct(&value).await?;
        assert_eq!(value, r);
    }

    //test call
    {
        //call!(@checkrun client=>800;5);
        server.print(5).await?;

        // call!(@checkrun client=>700;"joy");
        server.run_test(Some("joy")).await?;

        //let x:i32=call!(client=>1003;1);
        let x = server.to_client_add_one(1).await?;
        assert_eq!(x, 2);

        // call!(@checkrun client=>600;6,"my name is");
        server.print2(6, "my name is").await?;
    }

    // test cow
    {
        assert_eq!(server.test_cow(true).await?,"is static str");
        assert_eq!(server.test_cow(false).await?,"is string owned");
    }

    //test bench and recursive
    {
        let start = Instant::now();

        for i in 0..10000 {
            //call!(@result client=>1000;1,2);
            //let v =
            server.add(1, i).await?;
            //println!("{}",v);
        }

        //let r:i32=call!(client=>1005;10000);
        let r = server.recursive_test(10000).await?;
        println!("r:{} {}", r, start.elapsed().as_millis());
    }

    // simulation logon
    {
        let res = server
            .logon(LogOn {
                username: "username".to_string(),
                password: "password".to_string(),
            })
            .await?;

        assert_eq!(res, (true, "1 Ok".to_string()));
        println!("{:?}", res);

        let res = server
            .logon2(("username".into(), "password".into()))
            .await?;
        assert_eq!(
            res,
            LogOnResult {
                success: true,
                msg: "2 Ok".to_string()
            }
        );

        println!("{:?}", res);
    }

    let mut s = "".to_string();
    std::io::stdin().read_line(&mut s)?;
    client.close().await?;

    drop(server);
    drop(client);

    let mut s = "".to_string();
    std::io::stdin().read_line(&mut s)?;

    Ok(())
}
