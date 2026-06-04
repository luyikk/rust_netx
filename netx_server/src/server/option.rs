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
    /// Set the maximum number of concurrent connections. When the limit is reached,
    /// accept will block until a slot is freed. Set to 0 (default) for unlimited connections.
    #[serde(default)]
    pub max_connections: usize,
    /// Enable or disable TCP_NODELAY (Nagle's algorithm). When enabled,
    /// small packets are sent immediately without delay. Default: false (Nagle's algorithm enabled).
    #[serde(default)]
    pub is_nodelay: bool,
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
            max_connections: 0,
            is_nodelay: false,
        }
    }

    /// Sets the request timeout duration.
    #[inline]
    pub fn set_session_save_time(mut self, session_save_time: u32) -> Self {
        self.session_save_time = session_save_time;
        self
    }

    /// Sets the request timeout duration.
    #[inline]
    pub fn set_request_out_time(mut self, request_out_time: u32) -> Self {
        self.request_out_time = request_out_time;
        self
    }

    /// Sets the maximum number of concurrent connections.
    ///
    /// When the limit is reached, accept will block until a slot is freed. Set to 0 (default) for unlimited connections.
    #[inline]
    pub fn set_max_connections(mut self, max_connections: usize) -> Self {
        self.max_connections = max_connections;
        self
    }

    /// Sets whether to enable TCP_NODELAY (Nagle's algorithm).
    pub fn set_nodelay(mut self, is_nodelay: bool) -> Self {
        self.is_nodelay = is_nodelay;
        self
    }
}
