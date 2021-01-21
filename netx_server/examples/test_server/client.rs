use netxserver::*;

#[build]
pub trait IClient:Sync+Send{
    #[tag(2000)]
    async fn add_one(&self,i:i32)->Result<i32,Box<dyn Error>>;
    #[tag(2002)]
    async fn recursive_test(&self,a:i32)->Result<i32,Box<dyn Error>>;
    #[tag(4000)]
    async fn run(&self,name:String)->Result<(),Box<dyn Error>>;
    #[tag(5000)]
    async fn print2(&self,i:i32,s:String)->Result<(),Box<dyn Error>>;

}