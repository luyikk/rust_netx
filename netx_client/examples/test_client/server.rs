use crate::test_struct::{LogOn, LogOnResult};
use anyhow::*;
use netxclient::*;

#[build]
pub trait IServer: Sync + Send {
    #[tag(1000)]
    async fn add(&self, a: i32, b: i32) -> Result<i32>;
    #[tag(800)]
    async fn print(&self, a: i32) -> Result<()>;
    #[tag(600)]
    async fn print2(&self, a: i32, b:  String) -> Result<()>;
    #[tag(700)]
    async fn run_test(&self, a: String) -> Result<()>;
    #[tag(5001)]
    async fn test(&self, msg:String, i: i32);
    #[tag(1003)]
    async fn to_client_add_one(&self, a: i32) -> Result<i32>;
    #[tag(1005)]
    async fn recursive_test(&self, a: i32) -> Result<i32>;
    #[tag(10000)]
    async fn logon(&self, info: LogOn) -> Result<(bool, String)>;

    #[tag(10001)]
    async fn logon2(&self, info: (String, String)) -> Result<LogOnResult>;
}
