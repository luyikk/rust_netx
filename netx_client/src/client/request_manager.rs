use crate::client::{INextClientInner, NetXClient, SessionSave};
use aqueue::Actor;
use std::collections::VecDeque;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// `RequestManager` manages the requests, including their timeouts and the associated client.
pub struct RequestManager<T> {
    queue: VecDeque<(i64, Instant)>,
    request_out_time: u32,
    netx_client: Weak<Actor<NetXClient<T>>>,
}

unsafe impl<T> Send for RequestManager<T> {}
unsafe impl<T> Sync for RequestManager<T> {}

impl<T> Drop for RequestManager<T> {
    /// Custom drop implementation for `RequestManager` to log when it is dropped.
    fn drop(&mut self) {
        log::debug!("request manager is drop");
    }
}

impl<T: SessionSave + 'static> RequestManager<T> {
    /// Creates a new `RequestManager` instance.
    ///
    /// # Arguments
    ///
    /// * `request_out_time` - The timeout duration for requests.
    /// * `netx_client` - A weak reference to the `NetXClient`.
    ///
    /// # Returns
    ///
    /// An `Arc` wrapped `Actor` of `RequestManager`.
    pub fn new(
        request_out_time: u32,
        netx_client: Weak<Actor<NetXClient<T>>>,
    ) -> Arc<Actor<RequestManager<T>>> {
        let ptr = Arc::new(Actor::new(RequestManager {
            queue: VecDeque::new(),
            request_out_time,
            netx_client,
        }));

        Self::start_check(Arc::downgrade(&ptr));
        ptr
    }

    /// Starts the periodic check for request timeouts.
    ///
    /// # Arguments
    ///
    /// * `request_manager` - A weak reference to the `RequestManager`.
    fn start_check(request_manager: Weak<Actor<RequestManager<T>>>) {
        tokio::spawn(async move {
            while let Some(req) = request_manager.upgrade() {
                if let Err(er) = req.check().await {
                    log::error!("check request error:{}", er);
                }
                sleep(Duration::from_millis(500)).await
            }
        });
    }

    /// Checks the queue for timed-out requests and handles them.
    #[inline]
    pub async fn check(&mut self) {
        while let Some(item) = self.queue.pop_back() {
            if item.1.elapsed().as_millis() as u32 >= self.request_out_time {
                if let Some(client) = self.netx_client.upgrade() {
                    client
                        .set_error(item.0, crate::error::Error::SerialTimeOut(item.0))
                        .await;
                }
            } else {
                self.queue.push_back(item);
                break;
            }
        }
    }

    /// Adds a new request to the queue with the current timestamp.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The ID of the session to be added.
    #[inline]
    pub fn set(&mut self, session_id: i64) {
        self.queue.push_front((session_id, Instant::now()));
    }
}

/// Trait defining the interface for `RequestManager`.
pub(crate) trait IRequestManager {
    /// Asynchronously checks the requests.
    async fn check(&self) -> crate::error::Result<()>;
    /// Asynchronously sets a new request.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The ID of the session to be set.
    async fn set(&self, session_id: i64) -> crate::error::Result<()>;
}

impl<T: SessionSave + 'static> IRequestManager for Actor<RequestManager<T>> {
    /// Asynchronously checks the requests by calling the inner `check` method.
    #[inline]
    async fn check(&self) -> crate::error::Result<()> {
        self.inner_call(|inner| async move {
            inner.get_mut().check().await;
            Ok(())
        })
        .await
    }

    /// Asynchronously sets a new request by calling the inner `set` method.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The ID of the session to be set.
    #[inline]
    async fn set(&self, session_id: i64) -> crate::error::Result<()> {
        self.inner_call(|inner| async move {
            inner.get_mut().set(session_id);
            Ok(())
        })
        .await
    }
}
