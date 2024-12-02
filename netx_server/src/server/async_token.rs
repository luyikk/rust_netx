use crate::async_token_manager::IAsyncTokenManager;
use crate::{IController, NetPeer, RetResult};
use anyhow::{anyhow, bail, Result};
use aqueue::Actor;
use data_rw::{Data, DataOwnedReader};
use oneshot::{channel as oneshot, Receiver, Sender};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Weak};
use tokio::time::Instant;

#[cfg(all(feature = "tcpserver", not(feature = "tcp-channel-server")))]
use tcpserver::IPeer;

/// Represents an asynchronous token that manages a session and its associated data.
pub struct AsyncToken<T> {
    /// The session ID associated with this token.
    session_id: i64,
    /// The controller associated with this token, wrapped in an `Arc`.
    controller: Option<Arc<T>>,
    /// The network peer associated with this token, wrapped in an `Arc`.
    peer: Option<Arc<NetPeer>>,
    /// A weak reference to the asynchronous token manager.
    manager: Weak<dyn IAsyncTokenManager<T>>,
    /// A dictionary mapping serial numbers to result senders.
    result_dict: HashMap<i64, Sender<Result<DataOwnedReader>>>,
    /// An atomic counter for generating serial numbers.
    serial_atomic: AtomicI64,
    /// A queue of requests with their timestamps.
    request_queue: VecDeque<(i64, Instant)>,
}

unsafe impl<T: IController> Send for AsyncToken<T> {}
unsafe impl<T: IController> Sync for AsyncToken<T> {}

/// Type alias for a reference-counted actor managing an `AsyncToken`.
pub type NetxToken<T> = Arc<Actor<AsyncToken<T>>>;

impl<T: IController> AsyncToken<T> {
    /// Creates a new `AsyncToken` with the given session ID and manager.
    pub(crate) fn new(session_id: i64, manager: Weak<dyn IAsyncTokenManager<T>>) -> AsyncToken<T> {
        AsyncToken {
            session_id,
            controller: None,
            peer: None,
            manager,
            result_dict: Default::default(),
            serial_atomic: AtomicI64::new(1),
            request_queue: Default::default(),
        }
    }
}

impl<T> Drop for AsyncToken<T> {
    /// Logs a debug message when the `AsyncToken` is dropped.
    fn drop(&mut self) {
        log::debug!("token session id:{} drop", self.session_id);
    }
}

impl<T: IController> AsyncToken<T> {
    /// Calls a special function on the controller with the given command tag.
    #[inline]
    pub(crate) async fn call_special_function(&self, cmd_tag: i32) -> Result<()> {
        if let Some(ref controller) = self.controller {
            controller
                .call(1, cmd_tag, DataOwnedReader::new(vec![0; 4]))
                .await?;
        }
        Ok(())
    }

    /// Executes a controller function with the given parameters.
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

    /// Generates a new serial number.
    #[inline]
    pub(crate) fn new_serial(&self) -> i64 {
        self.serial_atomic.fetch_add(1, Ordering::Acquire)
    }

    /// Sets an error for the given serial number.
    #[inline]
    pub fn set_error(&mut self, serial: i64, err: anyhow::Error) -> Result<()> {
        if let Some(tx) = self.result_dict.remove(&serial) {
            tx.send(Err(err)).map_err(|_| anyhow!("rx is close"))
        } else {
            Ok(())
        }
    }

    /// Checks for request timeouts and sets errors for timed-out requests.
    #[inline]
    pub fn check_request_timeout(&mut self, request_out_time: u32) {
        while let Some(item) = self.request_queue.pop_back() {
            if item.1.elapsed().as_millis() as u32 >= request_out_time {
                if let Err(er) = self.set_error(item.0, anyhow!("time out")) {
                    log::error!("check err:{}", er);
                }
            } else {
                self.request_queue.push_back(item);
                break;
            }
        }
    }
}

/// Trait defining the inner workings of an asynchronous token.
pub(crate) trait IAsyncTokenInner {
    /// The type of the controller.
    type Controller: IController;

    /// Sets the controller for the asynchronous token.
    ///
    /// # Arguments
    ///
    /// * `controller` - An `Arc` reference to the controller.
    async fn set_controller(&self, controller: Arc<Self::Controller>);

    /// Clears all function mappings of the controller.
    async fn clear_controller_fun_maps(&self);

    /// Sets the network peer for the asynchronous token.
    ///
    /// # Arguments
    ///
    /// * `peer` - An optional `Arc` reference to the network peer.
    async fn set_peer(&self, peer: Option<Arc<NetPeer>>);

    /// Calls a special function on the controller, such as disconnect or connect.
    ///
    /// # Arguments
    ///
    /// * `cmd_tag` - The command tag indicating the function to call.
    ///
    /// # Returns
    ///
    /// * `Result<()>` - The result of the function call.
    async fn call_special_function(&self, cmd_tag: i32) -> Result<()>;

    /// Executes a controller function with the given parameters.
    ///
    /// # Arguments
    ///
    /// * `tt` - The type tag.
    /// * `cmd` - The command to execute.
    /// * `data` - The data to pass to the function.
    ///
    /// # Returns
    ///
    /// * `RetResult` - The result of the function execution.
    async fn execute_controller(&self, tt: u8, cmd: i32, data: DataOwnedReader) -> RetResult;

    /// Sets the response result for a given serial number.
    ///
    /// # Arguments
    ///
    /// * `serial` - The serial number.
    /// * `data` - The data to set as the result.
    ///
    /// # Returns
    ///
    /// * `Result<()>` - The result of setting the response.
    async fn set_result(&self, serial: i64, data: DataOwnedReader) -> Result<()>;

    /// Checks for request timeouts and handles them.
    ///
    /// # Arguments
    ///
    /// * `request_out_time` - The timeout duration in milliseconds.
    async fn check_request_timeout(&self, request_out_time: u32);
}

impl<T: IController + 'static> IAsyncTokenInner for Actor<AsyncToken<T>> {
    type Controller = T;

    #[inline]
    async fn set_controller(&self, controller: Arc<T>) {
        self.inner_call(|inner| async move { inner.get_mut().controller = Some(controller) })
            .await
    }

    #[inline]
    async fn clear_controller_fun_maps(&self) {
        self.inner_call(|inner| async move {
            inner.get_mut().controller = None;
        })
        .await
    }

    #[inline]
    async fn set_peer(&self, peer: Option<Arc<NetPeer>>) {
        self.inner_call(|inner| async move {
            inner.get_mut().peer = peer;
        })
        .await
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
                    log::error!(
                        "session id:{} call cmd:{} error:{:?}",
                        self.get_session_id(),
                        cmd,
                        err
                    );
                    RetResult::error(
                        -1,
                        format!(
                            "session id:{} call cmd:{} error:{}",
                            self.get_session_id(),
                            cmd,
                            err
                        ),
                    )
                }
            }
        }
    }

    #[inline]
    async fn set_result(&self, serial: i64, dr: DataOwnedReader) -> Result<()> {
        let have_tx: Option<Sender<Result<DataOwnedReader>>> = self
            .inner_call(|inner| async move { inner.get_mut().result_dict.remove(&serial) })
            .await;

        if let Some(tx) = have_tx {
            return tx.send(Ok(dr)).map_err(|_| anyhow!("close rx"));
        } else {
            match RetResult::from(dr) {
                Ok(res) => match res.check() {
                    Ok(_) => {
                        log::error!("not found 2 {}", serial)
                    }
                    Err(err) => {
                        log::error!("{}", err)
                    }
                },
                Err(er) => {
                    log::error!("not found {} :{}", serial, er)
                }
            }
        }
        Ok(())
    }

    #[inline]
    async fn check_request_timeout(&self, request_out_time: u32) {
        self.inner_call(|inner| async move {
            inner.get_mut().check_request_timeout(request_out_time);
        })
        .await
    }
}

/// Trait defining the interface for an asynchronous token.
pub trait IAsyncToken {
    /// The type of the controller.
    type Controller: IController;

    /// Gets the session ID associated with the token.
    ///
    /// # Returns
    ///
    /// * `i64` - The session ID.
    fn get_session_id(&self) -> i64;

    /// Generates a new serial ID.
    ///
    /// # Returns
    ///
    /// * `i64` - The new serial ID.
    fn new_serial(&self) -> i64;

    /// Gets the TCP socket peer.
    ///
    /// # Returns
    ///
    /// * `impl std::future::Future<Output = Option<Arc<NetPeer>>>` - A future that resolves to an optional `Arc` reference to the network peer.
    fn get_peer(&self) -> impl std::future::Future<Output = Option<Arc<NetPeer>>>;

    /// Sends a buffer.
    ///
    /// # Arguments
    ///
    /// * `buff` - The buffer to send.
    ///
    /// # Returns
    ///
    /// * `impl std::future::Future<Output = Result<()>>` - A future that resolves to a `Result`.
    fn send(&self, buff: Vec<u8>) -> impl std::future::Future<Output = Result<()>>;

    /// Gets the network token by session ID.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session ID.
    ///
    /// # Returns
    ///
    /// * `impl std::future::Future<Output = Result<Option<NetxToken<Self::Controller>>>>` - A future that resolves to an optional `NetxToken`.
    fn get_token(
        &self,
        session_id: i64,
    ) -> impl std::future::Future<Output = Result<Option<NetxToken<Self::Controller>>>>;

    /// Gets all network tokens.
    ///
    /// # Returns
    ///
    /// * `impl std::future::Future<Output = Result<Vec<NetxToken<Self::Controller>>>>` - A future that resolves to a vector of `NetxToken`.
    fn get_all_tokens(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<NetxToken<Self::Controller>>>>;

    /// Calls a function with the given serial and buffer.
    ///
    /// # Arguments
    ///
    /// * `serial` - The serial number.
    /// * `buff` - The buffer to send.
    ///
    /// # Returns
    ///
    /// * `impl std::future::Future<Output = Result<RetResult>>` - A future that resolves to a `RetResult`.
    fn call(&self, serial: i64, buff: Data)
        -> impl std::future::Future<Output = Result<RetResult>>;

    /// Runs a function with the given buffer.
    ///
    /// # Arguments
    ///
    /// * `buff` - The buffer to send.
    ///
    /// # Returns
    ///
    /// * `impl std::future::Future<Output = Result<()>>` - A future that resolves to a `Result`.
    fn run(&self, buff: Data) -> impl std::future::Future<Output = Result<()>>;

    /// Checks if the connection is disconnected.
    ///
    /// # Returns
    ///
    /// * `impl std::future::Future<Output = bool>` - A future that resolves to a boolean indicating if the connection is disconnected.
    fn is_disconnect(&self) -> impl std::future::Future<Output = bool>;
}

impl<T: IController + 'static> IAsyncToken for Actor<AsyncToken<T>> {
    /// controller type
    type Controller = T;

    #[inline]
    fn get_session_id(&self) -> i64 {
        unsafe { self.deref_inner().session_id }
    }

    #[inline]
    fn new_serial(&self) -> i64 {
        unsafe { self.deref_inner().new_serial() }
    }

    #[inline]
    async fn get_peer(&self) -> Option<Arc<NetPeer>> {
        self.inner_call(|inner| async move { inner.get_mut().peer.clone() })
            .await
    }

    #[inline]
    async fn send(&self, buff: Vec<u8>) -> Result<()> {
        unsafe {
            if let Some(peer) = self.deref_inner().peer.clone() {
                return peer.send_all(buff).await;
            }
            bail!("token:{} tcp disconnect", self.get_session_id())
        }
    }

    #[inline]
    async fn get_token(&self, session_id: i64) -> Result<Option<NetxToken<T>>> {
        self.inner_call(|inner| async move {
            let manager = inner
                .get()
                .manager
                .upgrade()
                .ok_or_else(|| anyhow!("manager upgrade fail"))?;
            Ok(manager.get_token(session_id).await)
        })
        .await
    }

    #[inline]
    async fn get_all_tokens(&self) -> Result<Vec<NetxToken<T>>> {
        self.inner_call(|inner| async move {
            let manager = inner
                .get()
                .manager
                .upgrade()
                .ok_or_else(|| anyhow!("manager upgrade fail"))?;
            manager.get_all_tokens().await
        })
        .await
    }

    #[inline]
    async fn call(&self, serial: i64, buff: Data) -> Result<RetResult> {
        let (peer, rx): (Arc<NetPeer>, Receiver<Result<DataOwnedReader>>) = self
            .inner_call(|inner| async move {
                if let Some(peer) = inner.get().peer.clone() {
                    let (tx, rx): (
                        Sender<Result<DataOwnedReader>>,
                        Receiver<Result<DataOwnedReader>>,
                    ) = oneshot();
                    if inner.get_mut().result_dict.contains_key(&serial) {
                        bail!("serial is have")
                    }
                    if inner.get_mut().result_dict.insert(serial, tx).is_none() {
                        inner
                            .get_mut()
                            .request_queue
                            .push_front((serial, Instant::now()));
                    }
                    Ok((peer, rx))
                } else {
                    bail!("call not connect")
                }
            })
            .await?;
        peer.send_all(buff.into_inner()).await?;
        match rx.await {
            Err(_) => Err(anyhow!("tx is Close")),
            Ok(data) => Ok(RetResult::from(data?)?),
        }
    }

    #[inline]
    async fn run(&self, buff: Data) -> Result<()> {
        let peer = self
            .inner_call(|inner| async move {
                if let Some(peer) = inner.get().peer.clone() {
                    Ok(peer)
                } else {
                    bail!("run not connect")
                }
            })
            .await?;
        peer.send_all(buff.into_inner()).await?;
        Ok(())
    }

    #[inline]
    async fn is_disconnect(&self) -> bool {
        self.inner_call(|inner| async move {
            if let Some(ref peer) = inner.get().peer {
                #[cfg(all(feature = "tcpserver", not(feature = "tcp-channel-server")))]
                if let Ok(r) = peer.is_disconnect().await {
                    return r;
                }

                #[cfg(feature = "tcp-channel-server")]
                return peer.is_disconnect();
            }
            true
        })
        .await
    }
}

/// Macro to call a peer with a command and arguments.
#[macro_export]
macro_rules! call_peer {
    (@uint $($x:tt)*)=>(());
    (@count $($rest:expr),*)=>(<[()]>::len(&[$(call_peer!(@uint $rest)),*]));
    ($peer:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call_peer!(@count $($args),*) as i32;
            let serial=$peer.new_serial();
            data.write_fixed(0u32);
            data.write_fixed(2400u32);
            data.write_fixed(2u8);
            data.write_fixed($cmd);
            data.write_fixed(serial);
            data.write_fixed(args_count);
            $(data.pack_serialize($args)?;)*
            let len=data.len();
            (&mut data[0..4]).put_u32_le(len as u32);
            let mut ret= $peer.call(serial,data).await?.check()?;
            ret.deserialize()?
    });
    (@result $peer:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call_peer!(@count $($args),*) as i32;
            let serial=$peer.new_serial();
            data.write_fixed(0u32);
            data.write_fixed(2400u32);
            data.write_fixed(2u8);
            data.write_fixed($cmd);
            data.write_fixed(serial);
            data.write_fixed(args_count);
            $(data.pack_serialize($args)?;)*
            let len=data.len();
            (&mut data[0..4]).put_u32_le(len as u32);
            $peer.call(serial,data).await?
    });
    (@run $peer:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call_peer!(@count $($args),*) as i32;
            let serial=$peer.new_serial();
            data.write_fixed(0u32);
            data.write_fixed(2400u32);
            data.write_fixed(0u8);
            data.write_fixed($cmd);
            data.write_fixed(serial);
            data.write_fixed(args_count);
            $(data.pack_serialize($args)?;)*
            let len=data.len();
            (&mut data[0..4]).put_u32_le(len as u32);
            $peer.run(data).await?;
    });
     (@run_not_err $peer:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call_peer!(@count $($args),*) as i32;
            let serial=$peer.new_serial();
            data.write_fixed(0u32);
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
            let len=data.len();
            (&mut data[0..4]).put_u32_le(len as u32);
            if let Err(err)= $peer.run(data).await{
                 log::warn!{"run {} is error:{}",$cmd,err}
            }
    });
    (@checkrun $peer:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call_peer!(@count $($args),*) as i32;
            let serial=$peer.new_serial();
            data.write_fixed(0u32);
            data.write_fixed(2400u32);
            data.write_fixed(1u8);
            data.write_fixed($cmd);
            data.write_fixed(serial);
            data.write_fixed(args_count);
            $(data.pack_serialize($args)?;)*
            let len=data.len();
            (&mut data[0..4]).put_u32_le(len as u32);
            $peer.call(serial,data).await?.check()?;
    });
}

/// Macro to create a reference to an implementation of a given interface.
///
/// # Arguments
///
/// * `$client` - The client instance.
/// * `$interface` - The interface type.
#[macro_export]
macro_rules! impl_ref {
    ($client:expr=>$interface:ty) => {
        paste::paste! {
            [<___impl_ $interface _call>]::new_ref(&$client)
        }
    };
}
