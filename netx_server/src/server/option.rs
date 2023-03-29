use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ServerOption {
    pub addr: String,
    pub service_name: String,
    pub verify_key: String,
    pub request_out_time: u32,
    pub session_save_time: u32,
}

impl ServerOption {
    #[inline]
    pub fn new(addr: &str, service_name: &str, verify_key: &str) -> ServerOption {
        ServerOption {
            addr: addr.to_string(),
            service_name: service_name.to_string(),
            verify_key: verify_key.to_string(),
            request_out_time: 5000,
            session_save_time: 5000,
        }
    }
}
