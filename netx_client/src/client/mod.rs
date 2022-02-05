#[macro_use]
mod impl_client;
pub mod controller;
mod default_session_save;
mod request_manager;
mod result;

use aqueue::Actor;
use std::sync::Arc;

pub use controller::*;
pub use default_session_save::*;
pub use impl_client::*;
pub use result::RetResult;

pub type NetxClientArc<T> = Arc<Actor<NetXClient<T>>>;
pub type NetxClientArcDef = Arc<Actor<NetXClient<DefaultSessionStore>>>;
