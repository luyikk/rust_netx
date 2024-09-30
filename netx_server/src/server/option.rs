use serde::{Deserialize, Serialize};

/// Represents the configuration options for the server.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ServerOption {
    /// The address of the server.
    pub addr: String,
    /// The name of the service.
    pub service_name: String,
    /// The verification key.
    pub verify_key: String,
    /// The timeout for requests in milliseconds.
    pub request_out_time: u32,
    /// The time to save the session in milliseconds.
    pub session_save_time: u32,
}

impl ServerOption {
    /// Creates a new `ServerOption` with the given address, service name, and verify key.
    ///
    /// # Arguments
    ///
    /// * `addr` - A string slice that holds the address of the server.
    /// * `service_name` - A string slice that holds the name of the service.
    /// * `verify_key` - A string slice that holds the verification key.
    ///
    /// # Returns
    ///
    /// A `ServerOption` instance with default values for `request_out_time` and `session_save_time`.
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
