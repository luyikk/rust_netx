use crate::async_token::{IAsyncToken, IAsyncTokenInner};
use crate::controller::ICreateController;
use crate::impl_server::SpecialFunctionTag;
use crate::server::async_token::{AsyncToken, NetxToken};
use aqueue::Actor;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Weak};
use tokio::time::{sleep, Duration, Instant};

/// Manages asynchronous tokens, including their creation, timeout checks, and disconnection handling.
pub struct AsyncTokenManager<T: ICreateController + 'static> {
    impl_controller: T,
    dict: HashMap<i64, NetxToken<T::Controller>>,
    request_out_time: u32,
    session_save_time: u32,
    request_disconnect_clear_queue: VecDeque<(i64, Instant)>,
}

unsafe impl<T: ICreateController + 'static> Send for AsyncTokenManager<T> {}
unsafe impl<T: ICreateController + 'static> Sync for AsyncTokenManager<T> {}

/// Type alias for a reference-counted actor managing asynchronous tokens.
pub type TokenManager<T> = Arc<Actor<AsyncTokenManager<T>>>;

impl<T: ICreateController + 'static> AsyncTokenManager<T> {
    /// Creates a new `AsyncTokenManager` instance.
    ///
    /// # Arguments
    ///
    /// * `impl_controller` - The controller implementation.
    /// * `request_out_time` - The timeout duration for requests.
    /// * `session_save_time` - The duration to save sessions.
    ///
    /// # Returns
    ///
    /// An `Arc` wrapped `Actor` managing the `AsyncTokenManager`.
    #[inline]
    pub(crate) fn new(
        impl_controller: T,
        request_out_time: u32,
        session_save_time: u32,
    ) -> TokenManager<T> {
        let ptr = Arc::new(Actor::new(AsyncTokenManager {
            impl_controller,
            dict: HashMap::new(),
            request_out_time,
            session_save_time,
            request_disconnect_clear_queue: Default::default(),
        }));

        Self::start_check(Arc::downgrade(&ptr));
        ptr
    }

    /// Starts the periodic check for token timeouts.
    ///
    /// # Arguments
    ///
    /// * `wk` - A weak reference to the `Actor` managing the `AsyncTokenManager`.
    #[inline]
    fn start_check(wk: Weak<Actor<AsyncTokenManager<T>>>) {
        tokio::spawn(async move {
            while let Some(manager) = wk.upgrade() {
                manager.check_tokens_request_timeout().await;
                manager.check_tokens_disconnect_timeout().await;

                sleep(Duration::from_millis(50)).await
            }
        });
    }

    /// Checks for tokens that have timed out on requests.
    #[inline]
    async fn check_tokens_request_timeout(&self) {
        for token in self.dict.values() {
            token.check_request_timeout(self.request_out_time).await;
        }
    }

    /// Checks for tokens that have timed out on disconnection.
    #[inline]
    async fn check_tokens_disconnect_timeout(&mut self) {
        while let Some(item) = self.request_disconnect_clear_queue.pop_back() {
            if item.1.elapsed().as_millis() as u32 >= self.session_save_time {
                if let Some(token) = self.dict.get(&item.0) {
                    if token.is_disconnect().await {
                        if let Some(token) = self.dict.remove(&item.0) {
                            if let Err(er) = token
                                .call_special_function(SpecialFunctionTag::Closed as i32)
                                .await
                            {
                                log::error!("call token Closed err:{}", er)
                            }
                            token.clear_controller_fun_maps().await;
                            log::debug!("token {} remove", token.get_session_id());
                        } else {
                            log::debug!("remove token {} fail", item.0);
                        }
                    } else {
                        log::debug!("remove token {},but it not disconnect", item.0);
                    }
                } else {
                    log::debug!("remove token not found {}", item.0);
                }
            } else {
                self.request_disconnect_clear_queue.push_back(item);
                break;
            }
        }
    }

    /// Generates a new session ID.
    ///
    /// # Returns
    ///
    /// A new session ID as an `i64`.
    #[inline]
    fn make_new_session_id(&mut self) -> i64 {
        chrono::Local::now().timestamp_nanos_opt().unwrap()
    }

    /// Creates a new token.
    ///
    /// # Arguments
    ///
    /// * `manager` - A weak reference to the `Actor` managing the `AsyncTokenManager`.
    ///
    /// # Returns
    ///
    /// A `Result` containing the new `NetxToken` or an error.
    #[inline]
    async fn create_token(
        &mut self,
        manager: Weak<Actor<AsyncTokenManager<T>>>,
    ) -> anyhow::Result<NetxToken<T::Controller>> {
        let session_id = self.make_new_session_id();
        let token = Arc::new(Actor::new(AsyncToken::new(session_id, manager)));
        let controller = self.impl_controller.create_controller(token.clone())?;
        token.set_controller(controller).await;
        self.dict.insert(session_id, token.clone());
        Ok(token)
    }

    /// Retrieves a token by its session ID.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session ID of the token.
    ///
    /// # Returns
    ///
    /// An `Option` containing the `NetxToken` if found.
    #[inline]
    pub fn get_token(&self, session_id: i64) -> Option<NetxToken<T::Controller>> {
        self.dict.get(&session_id).cloned()
    }

    /// Retrieves all tokens.
    ///
    /// # Returns
    ///
    /// A `Vec` containing all `NetxToken`s.
    #[inline]
    pub fn get_all_tokens(&self) -> Vec<NetxToken<T::Controller>> {
        self.dict.values().cloned().collect()
    }
}

/// Trait for creating tokens asynchronously.
pub(crate) trait IAsyncTokenManagerCreateToken<T> {
    /// Creates a new token.
    ///
    /// # Arguments
    ///
    /// * `manager` - A weak reference to the `Actor` managing the `AsyncTokenManager`.
    ///
    /// # Returns
    ///
    /// A `Result` containing the new `NetxToken` or an error.
    async fn create_token(&self, manager: Weak<Self>) -> anyhow::Result<NetxToken<T>>;
}

impl<T: ICreateController + 'static> IAsyncTokenManagerCreateToken<T::Controller>
    for Actor<AsyncTokenManager<T>>
{
    #[inline]
    async fn create_token(&self, manager: Weak<Self>) -> anyhow::Result<NetxToken<T::Controller>> {
        self.inner_call(|inner| async move { inner.get_mut().create_token(manager).await })
            .await
    }
}

/// Trait for managing tokens.
#[async_trait::async_trait]
pub trait ITokenManager<T>: Send + Sync {
    /// Retrieves a token by its session ID.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session ID of the token.
    ///
    /// # Returns
    ///
    /// An `Option` containing the `NetxToken` if found.
    async fn get_token(&self, session_id: i64) -> Option<NetxToken<T>>;

    /// Retrieves all tokens.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `Vec` of all `NetxToken`s or an error.
    async fn get_all_tokens(&self) -> Vec<NetxToken<T>>;
}

/// Trait for managing tokens asynchronously.
#[async_trait::async_trait]
pub(crate) trait IAsyncTokenManager<T>: ITokenManager<T> {
    /// Checks for tokens that have timed out on requests.
    async fn check_tokens_request_timeout(&self);

    /// Checks for tokens that have timed out on disconnection.
    async fn check_tokens_disconnect_timeout(&self);

    /// Handles peer disconnection.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session ID of the token.
    async fn peer_disconnect(&self, session_id: i64);
}

#[async_trait::async_trait]
impl<T: ICreateController + 'static> ITokenManager<T::Controller> for Actor<AsyncTokenManager<T>> {
    #[inline]
    async fn get_token(&self, session_id: i64) -> Option<NetxToken<T::Controller>> {
        self.inner_call(|inner| async move { inner.get().get_token(session_id) })
            .await
    }

    #[inline]
    async fn get_all_tokens(&self) -> Vec<NetxToken<T::Controller>> {
        self.inner_call(|inner| async move { inner.get().get_all_tokens() })
            .await
    }
}

#[async_trait::async_trait]
impl<T: ICreateController + 'static> IAsyncTokenManager<T::Controller>
    for Actor<AsyncTokenManager<T>>
{
    #[inline]
    async fn check_tokens_request_timeout(&self) {
        unsafe { self.deref_inner().check_tokens_request_timeout().await }
    }

    async fn check_tokens_disconnect_timeout(&self) {
        self.inner_call(
            |inner| async move { inner.get_mut().check_tokens_disconnect_timeout().await },
        )
        .await
    }

    #[inline]
    async fn peer_disconnect(&self, session_id: i64) {
        self.inner_call(|inner| async move {
            log::debug!("token {} start disconnect clear ", session_id);
            inner
                .get_mut()
                .request_disconnect_clear_queue
                .push_front((session_id, Instant::now()));
        })
        .await
    }
}
