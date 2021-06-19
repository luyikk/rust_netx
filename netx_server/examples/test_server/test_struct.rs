
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialOrd, PartialEq, Debug)]
pub struct LogOn {
    #[serde(rename = "Username")]
    pub username: String,
    #[serde(rename = "Password")]
    pub password: String,
}

#[derive(Serialize, Deserialize, PartialOrd, PartialEq, Debug)]
pub struct LogOnResult {
    #[serde(rename = "Success")]
    pub success: bool,
    #[serde(rename = "Msg")]
    pub msg: String,
}
