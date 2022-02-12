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

#[derive(Serialize, Deserialize, PartialOrd, PartialEq, Debug)]
pub struct Foo {
    pub v1: i32,
    pub v2: String,
    pub v3: Vec<i32>,
    pub v4: (f32, f64, String),
}

impl Default for Foo {
    fn default() -> Self {
        Foo {
            v1: 1,
            v2: "2".to_string(),
            v3: vec![3, 4],
            v4: (5.0f32, 6.111f64, "6".to_string()),
        }
    }
}
