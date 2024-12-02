use crate::async_token::NetxToken;
use crate::result::RetResult;
use anyhow::Result;
use data_rw::DataOwnedReader;
use std::sync::Arc;

/// Trait representing a controller that can handle calls.
pub trait IController: Send + Sync {
    /// Handles a call with the given parameters.
    ///
    /// # Parameters
    /// - `tt`: A `u8` representing the type.
    /// - `cmd_tag`: An `i32` representing the command tag.
    /// - `dr`: A `DataOwnedReader` instance.
    ///
    /// # Returns
    /// A future that resolves to a `Result` containing a `RetResult`.
    fn call(
        &self,
        tt: u8,
        cmd_tag: i32,
        dr: DataOwnedReader,
    ) -> impl std::future::Future<Output = Result<RetResult>> + Send;
}

/// Trait for creating controllers.
pub trait ICreateController {
    type Controller: IController;

    /// Creates a new controller.
    ///
    /// # Parameters
    /// - `token`: A `NetxToken` for the controller.
    ///
    /// # Returns
    /// A `Result` containing an `Arc` to the created controller.
    fn create_controller(
        &self,
        token: NetxToken<Self::Controller>,
    ) -> Result<Arc<Self::Controller>>;
}
