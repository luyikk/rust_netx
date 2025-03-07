use anyhow::{anyhow, bail, Context, Result};
use aqueue::Actor;
use data_rw::{Data, DataOwnedReader};
use log::warn;
use once_cell::sync::OnceCell;
use oneshot::{channel as oneshot, Receiver, Sender};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, ReadHalf};
use tokio::sync::watch::{channel, Receiver as WReceiver, Sender as WSender};
use tokio::time::{sleep, Duration};

#[cfg(all(feature = "tcpclient", not(feature = "tcp-channel-client")))]
use tcpclient::{SocketClientTrait, TcpClient};

#[cfg(feature = "tcp-channel-client")]
use tcp_channel_client::TcpClient;

use crate::client::controller::IController;
use crate::client::maybe_stream::MaybeStream;
use crate::client::request_manager::{IRequestManager, RequestManager};
use crate::client::result::RetResult;
use crate::client::NetxClientArc;

cfg_if::cfg_if! {
if #[cfg(feature = "use_openssl")]{
   use openssl::ssl::SslConnector;
   use tokio_openssl::SslStream;
   use std::pin::Pin;
}else if #[cfg(feature = "use_rustls")]{
   use tokio_rustls::TlsConnector;
   use tokio_rustls::rustls::pki_types::ServerName;
}}

/// Configuration for TLS (Transport Layer Security).
#[derive(Clone)]
pub enum TlsConfig {
    /// No TLS configuration.
    None,
    /// OpenSSL TLS configuration.
    #[cfg(all(feature = "use_openssl", not(feature = "use_rustls")))]
    OpenSsl {
        /// The domain name for the TLS connection.
        domain: String,
        /// The OpenSSL connector.
        connector: SslConnector,
    },
    /// Rustls TLS configuration.
    #[cfg(all(feature = "use_rustls", not(feature = "use_openssl")))]
    Rustls {
        /// The domain name for the TLS connection.
        domain: ServerName<'static>,
        /// The Rustls connector.
        connector: TlsConnector,
    },
}

#[cfg(all(feature = "tcpclient", not(feature = "tcp-channel-client")))]
pub type NetPeer = Actor<TcpClient<MaybeStream>>;

/// Type alias for `NetPeer` when the `tcp-channel-client` feature is enabled.
#[cfg(all(feature = "tcp-channel-client", not(feature = "tcpclient")))]
pub type NetPeer = TcpClient<MaybeStream>;

/// Type alias for the read half of a network stream.
pub type NetReadHalf = ReadHalf<MaybeStream>;

/// Represents a network client with various configurations and states.
pub struct NetXClient<T> {
    /// Information about the server.
    pub server_info: ServerOption,
    /// Configuration for TLS (Transport Layer Security).
    pub tls_config: TlsConfig,
    /// Mode of the client.
    pub mode: u8,
    /// Session information.
    pub session: T,
    /// Optional network peer.
    net: Option<Arc<NetPeer>>,
    /// Optional receiver for connection status updates.
    connect_stats: Option<WReceiver<(bool, String)>>,
    /// Dictionary to store results with their corresponding serial numbers.
    result_dict: HashMap<i64, Sender<crate::error::Result<DataOwnedReader>>>,
    /// Atomic counter for generating serial numbers.
    serial_atomic: AtomicI64,
    /// Manager for handling requests.
    request_manager: OnceCell<Arc<Actor<RequestManager<T>>>>,
    /// Optional controller for handling special functions.
    controller: Option<Box<dyn IController>>,
}

/// Trait for session management.
///
/// This trait defines methods for getting and storing session IDs,
/// which are used to manage individual sessions within the network client.
pub trait SessionSave {
    /// Gets the current session ID.
    ///
    /// # Returns
    ///
    /// * `i64` - The current session ID.
    fn get_session_id(&self) -> i64;

    /// Stores the given session ID.
    ///
    /// # Parameters
    ///
    /// * `session_id` - The session ID to store.
    fn store_session_id(&mut self, session_id: i64);
}

/// Tags for special functions that can be called by the network client.
enum SpecialFunctionTag {
    /// Tag for the connect function.
    Connect = 2147483647,
    /// Tag for the disconnect function.
    Disconnect = 2147483646,
    /// Tag for the closed function.
    Closed = 2147483645,
}

/// `Send` implementation for `NetXClient`.
unsafe impl<T> Send for NetXClient<T> {}

/// `Sync` implementation for `NetXClient`.
unsafe impl<T> Sync for NetXClient<T> {}

/// Custom `Drop` implementation for `NetXClient`.
impl<T> Drop for NetXClient<T> {
    /// Logs a debug message when the `NetXClient` is dropped.
    fn drop(&mut self) {
        log::debug!("{} is drop", self.server_info)
    }
}

/// Configuration options for the server.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ServerOption {
    /// The address of the server.
    pub addr: String,
    /// The name of the service.
    pub service_name: String,
    /// The key used for verification.
    pub verify_key: String,
    /// The timeout for requests in milliseconds.
    pub request_out_time_ms: u32,
}

/// Implementation of the `Display` trait for `ServerOption`.
///
/// This allows `ServerOption` to be formatted as a string,
/// displaying the service name and address.
impl std::fmt::Display for ServerOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}[{}]", self.service_name, self.addr)
    }
}

/// Implementation of the `ServerOption` struct.
impl ServerOption {
    /// Creates a new `ServerOption`.
    ///
    /// # Parameters
    ///
    /// * `addr` - The address of the server.
    /// * `service_name` - The name of the service.
    /// * `verify_key` - The key used for verification.
    /// * `request_out_time_ms` - The timeout for requests in milliseconds.
    ///
    /// # Returns
    ///
    /// * `ServerOption` - A new instance of `ServerOption`.
    pub fn new(
        addr: String,
        service_name: String,
        verify_key: String,
        request_out_time_ms: u32,
    ) -> ServerOption {
        ServerOption {
            addr,
            service_name,
            verify_key,
            request_out_time_ms,
        }
    }
}

impl<T: SessionSave + 'static> NetXClient<T> {
    cfg_if::cfg_if! {
        if #[cfg(feature = "use_openssl")] {
            /// Creates a new `NetXClient` with OpenSSL TLS configuration.
            ///
            /// # Parameters
            ///
            /// * `server_info` - Configuration options for the server.
            /// * `session` - The session information.
            /// * `domain` - The domain name for the TLS connection.
            /// * `connector` - The OpenSSL connector.
            ///
            /// # Returns
            ///
            /// * `NetxClientArc<T>` - A new instance of `NetXClient` wrapped in an `Arc`.
            pub fn new_ssl(server_info: ServerOption, session:T,domain:String,connector:SslConnector) ->NetxClientArc<T>{
                let request_out_time_ms=server_info.request_out_time_ms;
                let netx_client=Arc::new(Actor::new(NetXClient{
                    tls_config:TlsConfig::OpenSsl {domain,connector},
                    session,
                    server_info,
                    net:None,
                    result_dict:HashMap::new(),
                    connect_stats:None,
                    serial_atomic:AtomicI64::new(1),
                    request_manager:OnceCell::new(),
                    controller:None,
                    mode:0
                }));

                let request_manager=RequestManager::new(request_out_time_ms,Arc::downgrade(&netx_client));
                unsafe {
                    if netx_client.deref_inner().request_manager.set(request_manager).is_err(){
                        log::error!("not set request_manager,request_manager may not be none")
                    }
                }
                netx_client
            }
        } else if #[cfg(feature = "use_rustls")] {
             /// Creates a new `NetXClient` with Rustls TLS configuration.
             ///
             /// # Parameters
             ///
             /// * `server_info` - Configuration options for the server.
             /// * `session` - The session information.
             /// * `domain` - The domain name for the TLS connection.
             /// * `connector` - The Rustls connector.
             ///
             /// # Returns
             ///
             /// * `NetxClientArc<T>` - A new instance of `NetXClient` wrapped in an `Arc`.
             pub fn new_tls(server_info: ServerOption, session:T,domain:ServerName<'static>,connector:TlsConnector) ->NetxClientArc<T>{
                let request_out_time_ms=server_info.request_out_time_ms;
                let netx_client=Arc::new(Actor::new(NetXClient{
                    tls_config:TlsConfig::Rustls {domain,connector},
                    session,
                    server_info,
                    net:None,
                    result_dict:HashMap::new(),
                    connect_stats:None,
                    serial_atomic:AtomicI64::new(1),
                    request_manager:OnceCell::new(),
                    controller:None,
                    mode:0
                }));

                let request_manager=RequestManager::new(request_out_time_ms,Arc::downgrade(&netx_client));
                unsafe {
                    if netx_client.deref_inner().request_manager.set(request_manager).is_err(){
                        log::error!("not set request_manager,request_manager may not be none")
                    }
                }
                netx_client
            }
        }
    }

    /// Creates a new `NetXClient` without TLS configuration.
    ///
    /// # Parameters
    ///
    /// * `server_info` - Configuration options for the server.
    /// * `session` - The session information.
    ///
    /// # Returns
    ///
    /// * `NetxClientArc<T>` - A new instance of `NetXClient` wrapped in an `Arc`.
    pub fn new(server_info: ServerOption, session: T) -> NetxClientArc<T> {
        let request_out_time_ms = server_info.request_out_time_ms;
        let netx_client = Arc::new(Actor::new(NetXClient {
            tls_config: TlsConfig::None,
            session,
            server_info,
            net: None,
            result_dict: HashMap::new(),
            connect_stats: None,
            serial_atomic: AtomicI64::new(1),
            request_manager: OnceCell::new(),
            controller: None,
            mode: 0,
        }));

        let request_manager =
            RequestManager::new(request_out_time_ms, Arc::downgrade(&netx_client));
        unsafe {
            if netx_client
                .deref_inner()
                .request_manager
                .set(request_manager)
                .is_err()
            {
                log::error!("not set request_manager,request_manager may not be none")
            }
        }
        netx_client
    }

    /// Initializes the `NetXClient` with a given controller.
    ///
    /// # Parameters
    ///
    /// * `controller` - The controller to initialize the client with. It must implement the `IController` trait and be `Sync`, `Send`, and `'static`.
    #[inline]
    pub fn init<C: IController + Sync + Send + 'static>(&mut self, controller: C) {
        self.controller = Some(Box::new(controller));
    }

    /// Reads data from the network stream into the buffer and processes it.
    ///
    /// This function reads data from the provided `reader` and processes it using
    /// the `read_buffer` method. If an error occurs during reading, it logs the error.
    /// After reading, it cleans up the connection and calls the special disconnect function.
    ///
    /// # Parameters
    ///
    /// * `(netx_client, set_connect)` - A tuple containing the `NetxClientArc` and a `WSender`
    ///   for connection status updates.
    /// * `client` - An `Arc` containing the network peer.
    /// * `reader` - The read half of the network stream.
    ///
    /// # Returns
    ///
    /// * `Result<bool>` - Returns `Ok(true)` if the operation is successful, otherwise returns an error.
    #[allow(clippy::type_complexity)]
    async fn input_buffer(
        (netx_client, set_connect): (NetxClientArc<T>, WSender<(bool, String)>),
        client: Arc<NetPeer>,
        mut reader: NetReadHalf,
    ) -> Result<bool> {
        if let Err(er) = Self::read_buffer(&netx_client, set_connect, client, &mut reader).await {
            log::error!("read buffer err:{}", er);
        }
        netx_client.clean_connect().await?;
        log::debug!("disconnect to {}", netx_client.get_service_info());
        netx_client
            .call_special_function(SpecialFunctionTag::Disconnect as i32)
            .await?;
        Ok(true)
    }

    /// Reads data from the network stream into the buffer and processes it.
    ///
    /// This function reads data from the provided `reader` and processes it using
    /// the `read_buffer` method. If an error occurs during reading, it logs the error.
    /// After reading, it cleans up the connection and calls the special disconnect function.
    ///
    /// # Parameters
    ///
    /// * `netx_client` - A reference to the `NetxClientArc`.
    /// * `set_connect` - A `WSender` for connection status updates.
    /// * `client` - An `Arc` containing the network peer.
    /// * `reader` - The read half of the network stream.
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Returns `Ok(())` if the operation is successful, otherwise returns an error.
    async fn read_buffer(
        netx_client: &NetxClientArc<T>,
        set_connect: WSender<(bool, String)>,
        client: Arc<NetPeer>,
        reader: &mut NetReadHalf,
    ) -> Result<()> {
        let server_info = netx_client.get_service_info();
        let mut session_id = netx_client.get_session_id();
        client
            .send_all(
                Self::get_verify_buff(
                    &server_info.service_name,
                    &server_info.verify_key,
                    &session_id,
                )
                .into_inner(),
            )
            .await?;
        let mut option_connect = Some(set_connect);
        while let Ok(len) = reader.read_u32_le().await {
            let len = (len - 4) as usize;
            let mut buff = vec![0; len];
            reader.read_exact(&mut buff).await?;
            let mut dr = DataOwnedReader::new(buff);
            let cmd = dr.read_fixed::<i32>()?;
            match cmd {
                1000 => match dr.read_fixed::<bool>()? {
                    false => {
                        let msg = dr.read_fixed_str()?;
                        log::debug!("{server_info} {msg}");
                        if (dr.len() - dr.get_offset()) == 1 && dr.read_fixed::<u8>()? == 1 {
                            log::debug!("mode 1");
                            netx_client.set_mode(1).await;
                        }
                        client
                            .send_all(
                                Self::get_session_id_buff(netx_client.get_mode()).into_inner(),
                            )
                            .await?;

                        // call connect if error disconnect
                        if let Some(set_connect) = option_connect.take() {
                            let client = client.clone();
                            let netx_client = netx_client.clone();
                            tokio::spawn(async move {
                                if let Err(err) = netx_client
                                    .call_special_function(SpecialFunctionTag::Connect as i32)
                                    .await
                                {
                                    log::error!("call connect error:{}", err);
                                    let _ = client.disconnect().await;
                                    if set_connect.send((false, err.to_string())).is_err() {
                                        log::error!("talk connect rx is close");
                                    }
                                    drop(set_connect);
                                } else {
                                    if set_connect.send((true, "success".into())).is_err() {
                                        log::error!("talk connect rx is close");
                                    }
                                    drop(set_connect);
                                }
                            });
                        }
                    }
                    true => {
                        let err = dr.read_fixed_str()?;
                        log::error!("connect {} error:{}", server_info, err);
                        if let Some(set_connect) = option_connect.take() {
                            if set_connect.send((false, err.to_string())).is_err() {
                                log::error!("talk connect rx is close");
                            }
                            drop(set_connect);
                        }
                        break;
                    }
                },
                2000 => {
                    session_id = dr.read_fixed::<i64>()?;
                    log::debug!("{} save session id:{}", server_info, session_id);
                    netx_client.store_session_id(session_id).await;
                }
                2400 => {
                    let tt = dr.read_fixed::<u8>()?;
                    let cmd = dr.read_fixed::<i32>()?;
                    let session_id = dr.read_fixed::<i64>()?;
                    match tt {
                        0 => {
                            let run_netx_client = netx_client.clone();
                            tokio::spawn(async move {
                                let _ = run_netx_client.execute_controller(tt, cmd, dr).await;
                            });
                        }
                        1 => {
                            let run_netx_client = netx_client.clone();
                            let send_client = client.clone();
                            tokio::spawn(async move {
                                let res = run_netx_client.execute_controller(tt, cmd, dr).await;
                                if let Err(er) = send_client
                                    .send_all(
                                        Self::get_result_buff(
                                            session_id,
                                            res,
                                            run_netx_client.get_mode(),
                                        )
                                        .into_inner(),
                                    )
                                    .await
                                {
                                    log::error!("send buff 1 error:{}", er);
                                }
                            });
                        }
                        2 => {
                            let run_netx_client = netx_client.clone();
                            let send_client = client.clone();
                            tokio::spawn(async move {
                                let res = run_netx_client.execute_controller(tt, cmd, dr).await;
                                if let Err(er) = send_client
                                    .send_all(
                                        Self::get_result_buff(
                                            session_id,
                                            res,
                                            run_netx_client.get_mode(),
                                        )
                                        .into_inner(),
                                    )
                                    .await
                                {
                                    log::error!("send buff 2 error:{}", er);
                                }
                            });
                        }
                        _ => {
                            panic!("not found call type:{}", tt);
                        }
                    }
                }
                2500 => {
                    let serial = dr.read_fixed::<i64>()?;
                    netx_client.set_result(serial, dr).await;
                }
                _ => {
                    log::error!("{} Unknown command:{}->{:?}", server_info, cmd, dr);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Calls a special function on the controller if it exists.
    ///
    /// # Parameters
    ///
    /// * `cmd_tag` - The command tag to be sent to the controller.
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Returns `Ok(())` if the operation is successful, otherwise returns an error.
    #[inline]
    pub(crate) async fn call_special_function(&self, cmd_tag: i32) -> Result<()> {
        if let Some(ref controller) = self.controller {
            controller
                .call(1, cmd_tag, DataOwnedReader::new(vec![0; 4]))
                .await?;
        }
        Ok(())
    }

    /// Executes a command on the controller if it exists.
    ///
    /// # Parameters
    ///
    /// * `tt` - The type tag for the command.
    /// * `cmd` - The command to be executed.
    /// * `dr` - The data reader containing the command data.
    ///
    /// # Returns
    ///
    /// * `Result<RetResult>` - Returns the result of the command execution if successful, otherwise returns an error.
    #[inline]
    pub(crate) async fn execute_controller(
        &self,
        tt: u8,
        cmd: i32,
        dr: DataOwnedReader,
    ) -> Result<RetResult> {
        if let Some(ref controller) = self.controller {
            return controller.call(tt, cmd, dr).await;
        }
        bail!("controller is none")
    }

    /// Generates a verification buffer.
    ///
    /// # Parameters
    ///
    /// * `service_name` - The name of the service.
    /// * `verify_key` - The key used for verification.
    /// * `session_id` - The session ID.
    ///
    /// # Returns
    ///
    /// * `Data` - The verification buffer.
    #[inline]
    fn get_verify_buff(service_name: &str, verify_key: &str, session_id: &i64) -> Data {
        let mut data = Data::with_capacity(128);
        data.write_fixed(1000);
        data.write_fixed(service_name);
        data.write_fixed(verify_key);
        data.write_fixed(session_id);
        data
    }

    /// Generates a session ID buffer.
    ///
    /// # Parameters
    ///
    /// * `mode` - The mode of the client.
    ///
    /// # Returns
    ///
    /// * `Data` - The session ID buffer.
    fn get_session_id_buff(mode: u8) -> Data {
        let mut buff = Data::with_capacity(32);
        buff.write_fixed(2000);
        if mode == 0 {
            buff
        } else {
            let len = buff.len() + 4;
            let mut data = Data::with_capacity(len);
            data.write_fixed(len as u32);
            data.write_buf(&buff);
            data
        }
    }

    /// Generates a result buffer.
    ///
    /// # Parameters
    ///
    /// * `session_id` - The session ID.
    /// * `result` - The result of the operation.
    /// * `mode` - The mode of the client.
    ///
    /// # Returns
    ///
    /// * `Data` - The result buffer.
    #[inline]
    fn get_result_buff(session_id: i64, result: RetResult, mode: u8) -> Data {
        let mut data = Data::with_capacity(1024);
        data.write_fixed(2500u32);
        data.write_fixed(session_id);
        if result.is_error {
            data.write_fixed(true);
            data.write_fixed(result.error_id);
            data.write_fixed(result.msg);
        } else {
            data.write_fixed(false);
            data.write_fixed(result.arguments.len() as u32);
            for argument in result.arguments {
                data.write_fixed(argument.into_inner());
            }
        }

        if mode == 0 {
            data
        } else {
            let len = data.len() + 4usize;
            let mut buff = Data::with_capacity(len);
            buff.write_fixed(len as u32);
            buff.write_buf(&data);
            buff
        }
    }

    /// Sets the mode of the client.
    ///
    /// # Parameters
    ///
    /// * `mode` - The mode to set.
    #[inline]
    fn set_mode(&mut self, mode: u8) {
        self.mode = mode
    }

    /// Gets the current mode of the client.
    ///
    /// # Returns
    ///
    /// * `u8` - The current mode.
    #[inline]
    pub fn get_mode(&self) -> u8 {
        self.mode
    }

    /// Gets the address of the server as a string.
    ///
    /// # Returns
    ///
    /// * `String` - The server address.
    #[inline]
    pub fn get_addr_string(&self) -> String {
        self.server_info.addr.clone()
    }

    /// Gets the service information of the server.
    ///
    /// # Returns
    ///
    /// * `ServerOption` - The service information.
    #[inline]
    pub fn get_service_info(&self) -> ServerOption {
        self.server_info.clone()
    }

    /// Gets the current session ID.
    ///
    /// # Returns
    ///
    /// * `i64` - The current session ID.
    #[inline]
    pub fn get_session_id(&self) -> i64 {
        self.session.get_session_id()
    }

    /// Stores the given session ID.
    ///
    /// # Parameters
    ///
    /// * `session_id` - The session ID to store.
    #[inline]
    pub fn store_session_id(&mut self, session_id: i64) {
        self.session.store_session_id(session_id)
    }

    /// Sets the network client.
    ///
    /// # Parameters
    ///
    /// * `client` - The network client to set.
    #[inline]
    pub fn set_network_client(&mut self, client: Arc<NetPeer>) {
        self.net = Some(client);
    }

    /// Sets the connection status receiver.
    ///
    /// # Parameters
    ///
    /// * `stats` - The connection status receiver to set.
    #[inline]
    pub fn set_connect_stats(&mut self, stats: Option<WReceiver<(bool, String)>>) {
        self.connect_stats = stats;
    }

    /// Checks if the client is connected.
    ///
    /// # Returns
    ///
    /// * `bool` - `true` if connected, `false` otherwise.
    #[inline]
    pub fn is_connect(&self) -> bool {
        self.net.is_some()
    }

    /// Generates a new serial number.
    ///
    /// # Returns
    ///
    /// * `i64` - The new serial number.
    #[inline]
    pub fn new_serial(&self) -> i64 {
        self.serial_atomic.fetch_add(1, Ordering::Acquire)
    }

    /// Gets the length of the callback dictionary.
    ///
    /// # Returns
    ///
    /// * `usize` - The length of the callback dictionary.
    #[inline]
    pub fn get_callback_len(&mut self) -> usize {
        self.result_dict.len()
    }

    /// Sets the request session ID.
    ///
    /// # Parameters
    ///
    /// * `session_id` - The session ID to set.
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Returns `Ok(())` if successful, otherwise returns an error.
    #[inline]
    pub(crate) async fn set_request_session_id(&self, session_id: i64) -> crate::error::Result<()> {
        if let Some(request) = self.request_manager.get() {
            return request.set(session_id).await;
        }
        Ok(())
    }
}

/// Trait defining the internal operations for the `NetXClient`.
///
/// This trait includes methods for handling timeouts, setting results and errors,
/// calling special functions, executing controllers, cleaning connections, and resetting connection statuses.
pub(crate) trait INextClientInner {
    /// Gets the request or connect timeout time in milliseconds.
    fn get_timeout_ms(&self) -> u32;

    /// Sets the response result.
    ///
    /// # Parameters
    /// - `serial`: The serial number of the request.
    /// - `data`: The data to be set as the result.
    async fn set_result(&self, serial: i64, data: DataOwnedReader);

    /// Sets the response error.
    ///
    /// # Parameters
    /// - `serial`: The serial number of the request.
    /// - `err`: The error to be set as the result.
    async fn set_error(&self, serial: i64, err: crate::error::Error);

    /// Calls a special function (disconnect or connect command).
    ///
    /// # Parameters
    /// - `cmd_tag`: The command tag to be sent.
    ///
    /// # Returns
    /// - `Result<()>`: Returns `Ok(())` if the operation is successful, otherwise returns an error.
    async fn call_special_function(&self, cmd_tag: i32) -> Result<()>;

    /// Calls the request controller.
    ///
    /// # Parameters
    /// - `tt`: The type tag for the command.
    /// - `cmd`: The command to be executed.
    /// - `data`: The data reader containing the command data.
    ///
    /// # Returns
    /// - `RetResult`: Returns the result of the command execution if successful, otherwise returns an error.
    async fn execute_controller(&self, tt: u8, cmd: i32, data: DataOwnedReader) -> RetResult;

    /// Cleans the current connection.
    ///
    /// # Returns
    /// - `Result<()>`: Returns `Ok(())` if the operation is successful, otherwise returns an error.
    async fn clean_connect(&self) -> crate::error::Result<()>;

    /// Resets the network connection status.
    async fn reset_connect_stats(&self);

    /// Sets the mode of the client.
    ///
    /// # Parameters
    /// - `mode`: The mode to set.
    async fn set_mode(&self, mode: u8);

    /// Stores the session ID.
    ///
    /// # Parameters
    /// - `session_id`: The session ID to store.
    async fn store_session_id(&self, session_id: i64);
}

/// Implementation of the `INextClientInner` trait for `Actor<NetXClient<T>>`.
///
/// This implementation provides the internal operations for the `NetXClient`,
/// including methods for handling timeouts, setting results and errors,
/// calling special functions, executing controllers, cleaning connections,
/// and resetting connection statuses.
impl<T: SessionSave + 'static> INextClientInner for Actor<NetXClient<T>> {
    #[inline]
    fn get_timeout_ms(&self) -> u32 {
        unsafe { self.deref_inner().server_info.request_out_time_ms }
    }

    #[inline]
    async fn set_result(&self, serial: i64, data: DataOwnedReader) {
        let have_tx: Option<Sender<crate::error::Result<DataOwnedReader>>> = self
            .inner_call(|inner| async move { inner.get_mut().result_dict.remove(&serial) })
            .await;

        if let Some(tx) = have_tx {
            if tx.send(Ok(data)).is_err() {
                warn!("rx is close 1");
            }
        } else {
            match RetResult::from(data) {
                Ok(res) => match res.check() {
                    Ok(_) => log::error!("not found 2 {}", serial),
                    Err(err) => log::error!("{}", err),
                },
                Err(er) => log::error!("not found {} :{}", serial, er),
            }
        }
    }

    #[inline]
    async fn set_error(&self, serial: i64, err: crate::error::Error) {
        let have_tx: Option<Sender<crate::error::Result<DataOwnedReader>>> = self
            .inner_call(|inner| async move { inner.get_mut().result_dict.remove(&serial) })
            .await;
        if let Some(tx) = have_tx {
            if tx.send(Err(err)).is_err() {
                warn!("rx is close 2");
            }
        }
    }

    #[inline]
    async fn call_special_function(&self, cmd_tag: i32) -> Result<()> {
        unsafe { self.deref_inner().call_special_function(cmd_tag).await }
    }

    #[inline]
    async fn execute_controller(&self, tt: u8, cmd: i32, dr: DataOwnedReader) -> RetResult {
        unsafe {
            match self.deref_inner().execute_controller(tt, cmd, dr).await {
                Ok(res) => res,
                Err(err) => {
                    log::error!("call controller error:{:?}", err);
                    RetResult::error(1, format!("call controller err:{}", err))
                }
            }
        }
    }

    #[inline]
    async fn clean_connect(&self) -> crate::error::Result<()> {
        let net: Result<Arc<NetPeer>> = self
            .inner_call(|inner| async move { inner.get_mut().net.take().context("not connect") })
            .await;
        match net {
            Err(_) => Ok(()),
            Ok(net) => {
                net.disconnect().await?;
                sleep(Duration::from_millis(100)).await;
                Ok(())
            }
        }
    }

    #[inline]
    async fn reset_connect_stats(&self) {
        self.inner_call(|inner| async move {
            inner.get_mut().set_connect_stats(None);
        })
        .await
    }

    #[inline]
    async fn set_mode(&self, mode: u8) {
        self.inner_call(|inner| async move {
            inner.get_mut().set_mode(mode);
        })
        .await
    }

    #[inline]
    async fn store_session_id(&self, session_id: i64) {
        self.inner_call(|inner| async move {
            inner.get_mut().store_session_id(session_id);
        })
        .await
    }
}

#[allow(clippy::too_many_arguments)]
pub trait INetXClient {
    /// Initializes the NetX client controller.
    ///
    /// # Parameters
    /// - `controller`: The controller to initialize, which must implement the `IController` trait and be `Sync`, `Send`, and `'static`.
    ///
    /// # Returns
    /// A future that resolves to a `Result<()>`.
    fn init<C: IController + Sync + Send + 'static>(
        &self,
        controller: C,
    ) -> impl std::future::Future<Output = ()>;

    /// Connects to the network.
    ///
    /// # Returns
    /// A future that resolves to a `Result<()>`.
    fn connect_network(
        self: &Arc<Self>,
    ) -> impl std::future::Future<Output = crate::error::Result<()>> + Send;

    /// Gets the TLS configuration.
    ///
    /// # Returns
    /// The TLS configuration.
    fn get_tls_config(&self) -> TlsConfig;

    /// Gets the NetX server address.
    ///
    /// # Returns
    /// The server address as a `String`.
    fn get_address(&self) -> String;

    /// Gets the NetX client service configuration.
    ///
    /// # Returns
    /// The service configuration as a `ServerOption`.
    fn get_service_info(&self) -> ServerOption;

    /// Gets the NetX session ID.
    ///
    /// # Returns
    /// The session ID as an `i64`.
    fn get_session_id(&self) -> i64;

    /// Gets the NetX mode.
    ///
    /// # Returns
    /// The mode as a `u8`.
    fn get_mode(&self) -> u8;

    /// Generates a new serial ID.
    ///
    /// # Returns
    /// The new serial ID as an `i64`.
    fn new_serial(&self) -> i64;

    /// Checks if the client is connected to the server.
    ///
    /// # Returns
    /// `true` if connected, `false` otherwise.
    fn is_connect(&self) -> bool;

    /// Gets the TCP socket peer.
    ///
    /// # Returns
    /// A future that resolves to an `Option<Arc<NetPeer>>`.
    fn get_peer(&self) -> impl std::future::Future<Output = Option<Arc<NetPeer>>>;

    /// Sets the TCP socket peer.
    ///
    /// # Parameters
    /// - `client`: The network client to set.
    ///
    /// # Returns
    /// A future that resolves to `()`.
    fn set_network_client(&self, client: Arc<NetPeer>) -> impl std::future::Future<Output = ()>;

    /// Gets the length of the request wait callback.
    ///
    /// # Returns
    /// A future that resolves to the length as a `usize`.
    fn get_callback_len(&self) -> impl std::future::Future<Output = usize>;

    /// Closes the NetX client.
    ///
    /// # Returns
    /// A future that resolves to a `Result<()>`.
    fn close(&self) -> impl std::future::Future<Output = crate::error::Result<()>>;

    /// Calls a function with the given serial and buffer.
    ///
    /// # Parameters
    /// - `serial`: The serial ID.
    /// - `buff`: The data buffer.
    ///
    /// # Returns
    /// A future that resolves to a `Result<RetResult>`.
    fn call(
        &self,
        serial: i64,
        buff: Data,
    ) -> impl std::future::Future<Output = crate::error::Result<RetResult>>;

    /// Runs the client with the given buffer.
    ///
    /// # Parameters
    /// - `buff`: The data buffer.
    ///
    /// # Returns
    /// A future that resolves to a `Result<()>`.
    fn run(&self, buff: Data) -> impl std::future::Future<Output = crate::error::Result<()>>;
}

/// Implementation of the `INetXClient` trait for `Actor<NetXClient<T>>`.
///
/// This implementation provides the external operations for the `NetXClient`,
/// including methods for initializing the controller, connecting to the network,
/// getting configuration and session information, generating serial IDs, and
/// checking connection status.
#[allow(clippy::too_many_arguments)]
impl<T: SessionSave + 'static> INetXClient for Actor<NetXClient<T>> {
    #[inline]
    async fn init<C: IController + Sync + Send + 'static>(&self, controller: C) {
        self.inner_call(|inner| async move {
            inner.get_mut().init(controller);
        })
        .await
    }

    #[inline]
    async fn connect_network(self: &Arc<Self>) -> crate::error::Result<()> {
        let netx_client = self.clone();
        let wait_handler: crate::error::Result<Option<WReceiver<(bool, String)>>> = self.inner_call(|inner|async move  {
            if inner.get().is_connect() {
                return match inner.get().connect_stats {
                    Some(ref stats) => {
                        Ok(Some(stats.clone()))
                    },
                    None => {
                        warn!("inner is connect,but not get stats");
                        Ok(None)
                    }
                }
            }

            let (set_connect, wait_connect) = channel((false, "not connect".to_string()));

            let client={
            cfg_if::cfg_if! {
            if #[cfg(feature = "use_openssl")]{
                if let TlsConfig::OpenSsl{domain,connector}=netx_client.get_tls_config(){
                      let ssl=connector.configure()?.into_ssl(&domain)?;
                      tokio::time::timeout(Duration::from_millis(self.get_timeout_ms() as u64),TcpClient::connect_stream_type(netx_client.get_address(),|tcp_stream| async move{
                         let mut stream = SslStream::new(ssl, tcp_stream)?;
                         Pin::new(&mut stream).connect().await?;
                         Ok(MaybeStream::ServerSsl(stream))
                      },NetXClient::input_buffer, (netx_client, set_connect))).await.map_err(|_|anyhow!("connect timeout"))??
                }else{
                      tokio::time::timeout(Duration::from_millis(self.get_timeout_ms() as u64),TcpClient::connect_stream_type(netx_client.get_address(), |tcp_stream| async move{
                        Ok(MaybeStream::Plain(tcp_stream))
                      },NetXClient::input_buffer, (netx_client, set_connect))).await.map_err(|_|anyhow!("connect timeout"))??
                }
            }else if #[cfg(feature = "use_rustls")]{
                if let TlsConfig::Rustls{domain,connector}=netx_client.get_tls_config(){
                      tokio::time::timeout(Duration::from_millis(self.get_timeout_ms() as u64),TcpClient::connect_stream_type(netx_client.get_address(),|tcp_stream| async move{
                         let stream =connector.connect(domain,tcp_stream).await?;
                         Ok(MaybeStream::ServerTls(stream))
                      },NetXClient::input_buffer, (netx_client, set_connect))).await.map_err(|_|anyhow!("connect timeout"))??
                }else{
                      tokio::time::timeout(Duration::from_millis(self.get_timeout_ms() as u64),TcpClient::connect_stream_type(netx_client.get_address(), |tcp_stream| async move{
                        Ok(MaybeStream::Plain(tcp_stream))
                      },NetXClient::input_buffer, (netx_client, set_connect))).await.map_err(|_|anyhow!("connect timeout"))??
                }
            }else{
                    tokio::time::timeout(Duration::from_millis(self.get_timeout_ms() as u64),TcpClient::connect_stream_type(netx_client.get_address(), |tcp_stream| async move{
                        Ok(MaybeStream::Plain(tcp_stream))
                    },NetXClient::input_buffer, (netx_client, set_connect))).await.map_err(|_|anyhow!("connect timeout"))??
            }}};

            let ref_inner = inner.get_mut();
            ref_inner.set_network_client(client);
            ref_inner.connect_stats = Some(wait_connect.clone());
            Ok(Some(wait_connect))

        }).await;

        if let Some(mut wait_handler) = wait_handler? {
            match wait_handler.changed().await {
                Err(err) => {
                    self.reset_connect_stats().await;
                    return Err(err.into());
                }
                Ok(_) => {
                    self.reset_connect_stats().await;
                    let (is_connect, msg) = &(*wait_handler.borrow());
                    if !is_connect {
                        return Err(crate::error::Error::ConnectError(msg.clone()));
                    }
                }
            }
        }

        Ok(())
    }

    #[inline]
    fn get_tls_config(&self) -> TlsConfig {
        unsafe { self.deref_inner().tls_config.clone() }
    }

    #[inline]
    fn get_address(&self) -> String {
        unsafe { self.deref_inner().get_addr_string() }
    }

    #[inline]
    fn get_service_info(&self) -> ServerOption {
        unsafe { self.deref_inner().get_service_info() }
    }

    #[inline]
    fn get_session_id(&self) -> i64 {
        unsafe { self.deref_inner().get_session_id() }
    }

    #[inline]
    fn get_mode(&self) -> u8 {
        unsafe { self.deref_inner().get_mode() }
    }

    #[inline]
    fn new_serial(&self) -> i64 {
        unsafe { self.deref_inner().new_serial() }
    }

    #[inline]
    fn is_connect(&self) -> bool {
        unsafe { self.deref_inner().is_connect() }
    }

    #[inline]
    async fn get_peer(&self) -> Option<Arc<NetPeer>> {
        self.inner_call(|inner| async move { inner.get().net.clone() })
            .await
    }

    #[inline]
    async fn set_network_client(&self, client: Arc<NetPeer>) {
        self.inner_call(|inner| async move {
            inner.get_mut().set_network_client(client);
        })
        .await
    }

    #[inline]
    async fn get_callback_len(&self) -> usize {
        self.inner_call(|inner| async move { inner.get_mut().get_callback_len() })
            .await
    }

    #[inline]
    async fn close(&self) -> crate::error::Result<()> {
        let net: Result<Arc<NetPeer>> = self
            .inner_call(|inner| async move {
                if let Err(er) = inner
                    .get()
                    .call_special_function(SpecialFunctionTag::Closed as i32)
                    .await
                {
                    log::error!("call controller Closed err:{}", er)
                }
                inner.get_mut().controller = None;
                inner.get_mut().net.take().context("not connect")
            })
            .await;
        match net {
            Err(_) => Ok(()),
            Ok(net) => {
                net.disconnect().await?;
                sleep(Duration::from_millis(100)).await;
                Ok(())
            }
        }
    }

    #[inline]
    async fn call(&self, serial: i64, buff: Data) -> crate::error::Result<RetResult> {
        let (net, rx): (
            Arc<NetPeer>,
            Receiver<crate::error::Result<DataOwnedReader>>,
        ) = self
            .inner_call(|inner| async move {
                if let Some(ref net) = inner.get().net {
                    if inner.get_mut().result_dict.contains_key(&serial) {
                        bail!("serial is have")
                    }
                    let (tx, rx): (
                        Sender<crate::error::Result<DataOwnedReader>>,
                        Receiver<crate::error::Result<DataOwnedReader>>,
                    ) = oneshot();
                    inner.get_mut().result_dict.insert(serial, tx);
                    Ok((net.clone(), rx))
                } else {
                    bail!("not connect")
                }
            })
            .await?;
        unsafe {
            self.deref_inner().set_request_session_id(serial).await?;
        }
        if self.get_mode() == 0 {
            net.send_all(buff.into_inner()).await?;
        } else {
            let len = buff.len() + 4;
            let mut data = Data::with_capacity(len);
            data.write_fixed(len as u32);
            data.write_buf(&buff);

            net.send_all(data.into_inner()).await?;
        }
        match rx.await {
            Err(_) => Err(crate::error::Error::SerialClose(serial)),
            Ok(data) => Ok(RetResult::from(data?)?),
        }
    }

    #[inline]
    async fn run(&self, buff: Data) -> crate::error::Result<()> {
        let net = self
            .inner_call(|inner| async move {
                if let Some(ref net) = inner.get().net {
                    Ok(net.clone())
                } else {
                    bail!("not connect")
                }
            })
            .await?;
        if self.get_mode() == 0 {
            net.send_all(buff.into_inner()).await?;
        } else {
            let len = buff.len() + 4;
            let mut data = Data::with_capacity(len);
            data.write_fixed(len as u32);
            data.write_buf(&buff);
            net.send_all(data.into_inner()).await?;
        }

        Ok(())
    }
}

#[macro_export]
macro_rules! call {
    // Helper macro to count the number of arguments
    (@uint $($x:tt)*)=>(());
    (@count $($rest:expr),*)=>(<[()]>::len(&[$(call!(@uint $rest)),*]));

    // Macro to call a command and deserialize the result
    ($client:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            if $client.is_connect() ==false{
                $client.connect_network().await?;
            }
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call!(@count $($args),*) as i32;
            let serial=$client.new_serial();
            data.write_fixed(2400u32);
            data.write_fixed(2u8);
            data.write_fixed($cmd);
            data.write_fixed(serial);
            data.write_fixed(args_count);
            $(data.pack_serialize($args)?;)*
            let mut ret= $client.call(serial,data).await?.check()?;
            ret.deserialize()?
    });

    // Macro to call a command and return the result
    (@result $client:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            if $client.is_connect() ==false{
               $client.connect_network().await?;
            }
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call!(@count $($args),*) as i32;
            let serial=$client.new_serial();
            data.write_fixed(2400u32);
            data.write_fixed(2u8);
            data.write_fixed($cmd);
            data.write_fixed(serial);
            data.write_fixed(args_count);
            $(data.pack_serialize($args)?;)*
            $client.call(serial,data).await?
    });

    // Macro to run a command without returning a result
    (@run $client:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            if $client.is_connect() ==false{
                $client.connect_network().await?;
            }
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call!(@count $($args),*) as i32;
            let serial=$client.new_serial();
            data.write_fixed(2400u32);
            data.write_fixed(0u8);
            data.write_fixed($cmd);
            data.write_fixed(serial);
            data.write_fixed(args_count);
            $(data.pack_serialize($args)?;)*
            $client.run(data).await?;
    });

    // Macro to run a command without returning a result and log errors
     (@run_not_err $client:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            if $client.is_connect() ==false{
               if let Err(err)= $client.connect_network().await{
                    log::error!{"run connect {} is error:{}",$cmd,err}
               }
            }
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call!(@count $($args),*) as i32;
            let serial=$client.new_serial();
            data.write_fixed(2400u32);
            data.write_fixed(0u8);
            data.write_fixed($cmd);
            data.write_fixed(serial);
            data.write_fixed(args_count);
            $(
              if let Err(err)=  data.pack_serialize($args){
                 log::error!{"pack_serialize {} is error:{}",$cmd,err};
              }
            )*
            if let Err(err)= $client.run(data).await{
                 log::warn!{"run {} is error:{}",$cmd,err}
            }
    });

    // Macro to call a command, check the result, and return an error if the check fails
    (@checkrun $client:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            if $client.is_connect() ==false{
                $client.connect_network().await?;
            }
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call!(@count $($args),*) as i32;
            let serial=$client.new_serial();
            data.write_fixed(2400u32);
            data.write_fixed(1u8);
            data.write_fixed($cmd);
            data.write_fixed(serial);
            data.write_fixed(args_count);
            $(data.pack_serialize($args)?;)*
            $client.call(serial,data).await?.check()?;

    });

}

/// Macro to create a `Box<dyn $interface>` that clones `$client`.
#[macro_export]
macro_rules! impl_interface {
    ($client:expr=>$interface:ty) => {
        paste::paste! {
              Box::new([<___impl_ $interface _call>]::new($client.clone()))  as  Box<dyn $interface>
        }
    };
}

/// Macro to create an implementation of `$interface` that clones `$client`.
#[macro_export]
macro_rules! impl_struct {
    ($client:expr=>$interface:ty) => {
        paste::paste! {
            [<___impl_ $interface _call>]::new_impl($client.clone())
        }
    };
}

/// Macro to create a struct for `$interface` that references `$client`.
#[macro_export]
macro_rules! impl_ref {
    ($client:expr=>$interface:ty) => {
        paste::paste! {
            [<___impl_ $interface _call>]::new_impl_ref(&$client)
        }
    };
}

/// Macro to create a `Box<dyn $interface>` without cloning `$client`.
#[macro_export]
macro_rules! impl_owned_interface {
    ($client:expr=>$interface:ty) => {
        paste::paste! {
              Box::new([<___impl_ $interface _call>]::new($client))  as  Box<dyn $interface>
        }
    };
}
