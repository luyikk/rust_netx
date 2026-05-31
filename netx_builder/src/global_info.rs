//! Internal data structures used during proc-macro code generation.
//!
//! [`FuncInfo`] is populated by [`crate::get_funcs_info`] and then consumed
//! by the code-generation helpers ([`crate::get_impl_func`],
//! [`crate::make_dispatch_arms`]).

use proc_macro2::{Ident, TokenStream};
use syn::punctuated::Punctuated;
use syn::{FnArg, ReturnType, Token};

/// Metadata collected from a single `#[tag(...)]`-annotated trait method.
///
/// Each field is used by the code-generation helpers to emit the correct Rust
/// source code for both the **proxy caller** (client or server push) and the
/// **dispatcher** (`IController::call` match arm).
#[derive(Clone)]
pub struct FuncInfo {
    /// The numeric command ID assigned by `#[tag(N)]`.
    ///
    /// Special values (defined as constants in `lib.rs`):
    /// - `2147483647` → `connect`
    /// - `2147483646` → `disconnect`
    /// - `2147483645` → `closed`
    pub tag: i32,

    /// The *call type* that controls how the remote end responds.
    ///
    /// | value | meaning                     | generated call macro form   |
    /// |-------|-----------------------------|-----------------------------|
    /// | `0`   | fire-and-forget             | `@run_not_err`              |
    /// | `1`   | confirm (wait for `Ok(())`) | `@checkrun`                 |
    /// | `2`   | return value (wait for `T`) | default form                |
    pub tt: u8,

    /// The Rust identifier of the trait method, e.g. `"login"`.
    pub func_name: String,

    /// The type tokens for each non-`self` parameter, used to generate
    /// `data.pack_deserialize::<TYPE>()` calls in the dispatcher.
    ///
    /// # Example
    /// For `async fn login(&self, name: String, age: u32)` this would be
    /// `[quote!(String), quote!(u32)]`.
    pub args_type: Vec<TokenStream>,

    /// The full parameter list (including `&self`) taken directly from the
    /// trait method signature — re-emitted verbatim in the proxy impl.
    pub inputs: Punctuated<FnArg, Token![,]>,

    /// The identifier of every non-`self` parameter, used to forward
    /// arguments in the proxy impl (`call!(self.client => tag; name, age,)`).
    pub input_names: Vec<Ident>,

    /// The return type of the method as written in the trait, re-emitted
    /// verbatim in the proxy impl (`-> Result<String>`).
    pub output: ReturnType,
}
