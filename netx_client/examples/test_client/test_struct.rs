use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialOrd, PartialEq, Debug)]
pub struct LogOn {
    pub username: String,
    pub password: String,
}

#[derive(Serialize, Deserialize, PartialOrd, PartialEq, Debug)]
pub struct LogOnResult {
    pub success: bool,
    pub msg: String,
}
