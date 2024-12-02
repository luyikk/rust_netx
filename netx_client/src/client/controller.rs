use crate::client::RetResult;
use anyhow::{bail, Result};
use data_rw::DataOwnedReader;

/// An asynchronous trait for controllers that can handle commands.
#[async_trait::async_trait]
pub trait IController: Send + Sync {
    /// Asynchronously handles a command.
    ///
    /// # Arguments
    ///
    /// * `tt` - A type tag.
    /// * `cmd_tag` - A command tag.
    /// * `dr` - A data reader.
    ///
    /// # Returns
    ///
    /// A result containing either a `RetResult` or an error.
    async fn call(&self, tt: u8, cmd_tag: i32, dr: DataOwnedReader) -> Result<RetResult>;
}

/// A default implementation of the `IController` trait.
#[derive(Default)]
pub struct DefaultController;

#[async_trait::async_trait]
impl IController for DefaultController {
    /// Asynchronously handles a command.
    ///
    /// This implementation always returns an error indicating that the command tag was not found.
    ///
    /// # Arguments
    ///
    /// * `_tt` - A type tag (unused).
    /// * `cmd_tag` - A command tag.
    /// * `_dr` - A data reader (unused).
    ///
    /// # Returns
    ///
    /// An error indicating that the command tag was not found.
    async fn call(&self, _tt: u8, cmd_tag: i32, _dr: DataOwnedReader) -> Result<RetResult> {
        bail!("not found cmd tag:{} for DefaultController", cmd_tag)
    }
}
