use serde::{Serialize,Deserialize};


#[derive(Deserialize,Serialize)]
pub struct LogOn{
    pub nickname:String
}

#[derive(Deserialize,Serialize)]
pub struct LogOnRes{
    pub success:bool,
    pub msg:String
}

#[derive(Deserialize,Serialize,Clone)]
pub struct User{
    pub nickname:String,
    pub sessionid:i64
}

