#[macro_use]
mod impl_client;
pub mod controller;
mod default_session_save;
mod maybe_stream;
mod request_manager;
mod result;
#[cfg(feature = "use_rustls")]
mod rustls_accept_any_cert_verifier;

use aqueue::Actor;
use std::sync::Arc;

pub use controller::*;
pub use default_session_save::*;
pub use impl_client::*;
pub use result::RetResult;

#[cfg(feature = "use_rustls")]
pub use rustls_accept_any_cert_verifier::RustlsAcceptAnyCertVerifier;

pub type NetxClientArc<T> = Arc<Actor<NetXClient<T>>>;
pub type NetxClientArcDef = Arc<Actor<NetXClient<DefaultSessionStore>>>;
