use anyhow::Result;
use netxserver::*;

#[build]
pub trait IClient: Sync + Send {
    #[tag(2000)]
    async fn add_one(&self, i: i32) -> Result<i32>;
    #[tag(2002)]
    async fn recursive_test(&self, a: i32) -> Result<i32>;
    #[tag(4000)]
    async fn run(&self, name: Option<String>) -> Result<Option<String>>;
    #[tag(5000)]
    async fn print2(&self, i: i32, s: &str) -> Result<()>;
}
