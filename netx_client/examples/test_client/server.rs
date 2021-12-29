use crate::test_struct::{LogOn, LogOnResult, Foo};
use anyhow::Result;
use netxclient::prelude::*;

#[build]
pub trait IServer: Sync + Send {
    #[tag(1)]
    async fn test_base_type(&self,v:(bool,i8,u8,i16,u16,i32,u32,i64,u64,f32,f64))->Result<(bool,i8,u8,i16,u16,i32,u32,i64,u64,f32,f64)>;
    #[tag(2)]
    async fn test_string(&self,v:&(String,Option<String>,Option<String>))->Result<(String,Option<String>,Option<String>)>;
    #[tag(3)]
    async fn test_buff(&self,v:&(Vec<u8>,Option<Vec<u8>>,Option<Vec<u8>>))->Result<(Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>)>;
    #[tag(4)]
    async fn test_struct(&self,foo:&Foo)->Result<Foo>;

    #[tag(1000)]
    async fn add(&self, a: i32, b: i32) -> Result<i32>;
    #[tag(800)]
    async fn print(&self, a: i32) -> Result<()>;
    #[tag(600)]
    async fn print2(&self, a: i32, b:  &str) -> Result<()>;
    #[tag(700)]
    async fn run_test(&self, a: Option<&str>) -> Result<()>;
    #[tag(5001)]
    async fn test(&self, msg:&str, i: i32);
    #[tag(1003)]
    async fn to_client_add_one(&self, a: i32) -> Result<i32>;
    #[tag(1005)]
    async fn recursive_test(&self, a: i32) -> Result<i32>;
    #[tag(10000)]
    async fn logon(&self, info: LogOn) -> Result<(bool, String)>;

    #[tag(10001)]
    async fn logon2(&self, info: (String, String)) -> Result<LogOnResult>;
}
