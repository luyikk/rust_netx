use serde::{Serialize,Deserialize};


#[derive(Serialize,Deserialize,PartialOrd, PartialEq,Debug)]
pub struct LogOn{
    pub username:String,
    pub password:String
}

#[derive(Serialize,Deserialize,PartialOrd, PartialEq,Debug)]
pub enum Flag{
    Message(String),
    Int(i32)
}

#[derive(Serialize,Deserialize,PartialOrd, PartialEq,Debug)]
pub struct LogOnResult{
    pub success:bool,
    pub msg:Flag
}
