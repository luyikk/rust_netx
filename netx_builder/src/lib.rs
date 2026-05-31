//! # netxbuilder — proc-macro code generation for the netx RPC framework
//!
//! This crate provides four proc-macro attributes:
//!
//! | Attribute           | Where used   | Purpose                                             |
//! |---------------------|--------------|-----------------------------------------------------|
//! | `#[build_client]`   | `trait`      | Generate client proxy struct + optional dispatcher  |
//! | `#[build_server]`   | `trait`      | Generate server-push proxy + optional dispatcher    |
//! | `#[build_impl]`     | `impl` block | Marker (no-op); validates the impl block parses     |
//! | `#[build_impl_client]` | `impl` block | Same as above + adds `#[async_trait::async_trait]`|
//! | `#[tag(N)]`         | trait method | Assigns a command ID; consumed by the above macros  |
//!
//! ## How it all fits together
//!
//! ```text
//!  User writes:
//!
//!   #[build_client(ClientController)]   ← "I am the server; clients implement this"
//!   pub trait IClientController {
//!       #[tag(2001)]
//!       async fn on_message(&self, from: String, text: String);   // tt=0: fire-and-forget
//!       #[tag(2002)]
//!       async fn on_ping(&self, t: i64) -> Result<i64>;           // tt=2: returns a value
//!   }
//!
//!  Macro generates:
//!   1. The trait itself (with #[async_trait])
//!   2. impl IController for ClientController { fn call(...) { match cmd_tag { ... } } }
//!      — deserialises arguments and dispatches to the correct method
//!   3. pub struct ___impl_IClientController_call<T> { client: T }
//!      — a proxy struct that can be used as `impl IClientController`
//! ```

mod global_info;

extern crate proc_macro;

use global_info::*;
use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::{format_ident, quote};
use syn::{
    // `parse_macro_input!` turns the raw `TokenStream` into a typed syn AST node.
    parse_macro_input,
    // Used when walking function arguments to detect typed parameters vs. `self`.
    FnArg,
    // Used to inspect the single type parameter of `Result<T>`.
    GenericArgument,
    // Used to parse the `impl` block in `build_impl` / `build_impl_client`.
    ItemImpl,
    // Used to parse the `trait` definition in `build_client` / `build_server`.
    ItemTrait,
    // `LitInt` is the syn 2.0 type for integer literals; used to parse `#[tag(1000)]`.
    // (syn 1.0 used the generic `Lit::Int` variant — syn 2.0 prefers the concrete type.)
    LitInt,
    // `Meta` is the enum that represents a parsed attribute (`#[...]`).
    // syn 2.0: attributes now expose `attr.meta` as a direct field instead of
    // requiring a call to `attr.parse_meta()`.
    Meta,
    // Pattern nodes — used to verify that function parameters are plain identifiers.
    Pat,
    // Generic argument brackets: `AngleBracketed` covers `Result<T>`.
    PathArguments,
    // Return type: `Default` = no `->`, `Type(_, box T)` = `-> T`.
    ReturnType,
    // Trait item discriminant.
    // syn 2.0 renamed `TraitItem::Method` → `TraitItem::Fn`.
    TraitItem,
    // syn 2.0 renamed `TraitItemMethod` → `TraitItemFn`.
    TraitItemFn,
    // The Rust type system representation used to inspect `Result<T>`.
    Type,
};

// ─── Protocol command-ID constants ───────────────────────────────────────────
//
// These three values are *reserved* command IDs that the netx runtime uses for
// connection lifecycle events.  They occupy the top of the i32 range so they
// cannot clash with ordinary user-defined tags (which should be small positive
// integers like 1000, 1001, …).
//
// The same three values are hard-coded in `netxserver` and `netxclient` so
// that the generated `match cmd_tag { ... }` arms can be dispatched correctly
// at runtime without any additional serialisation overhead.

/// Fired once after a client successfully authenticates with the server.
const CONNECT: i32 = 2147483647; // i32::MAX

/// Fired when the TCP connection drops (before the session is removed).
const DISCONNECT: i32 = 2147483646; // i32::MAX - 1

/// Fired when the server-side session token is finally garbage-collected.
/// Useful for definitive cleanup (e.g. removing the user from a lobby).
const CLOSED: i32 = 2147483645; // i32::MAX - 2

// ─── Attribute parsing (syn 2.0 API) ─────────────────────────────────────────

/// Scan *one* trait method for a `#[tag(...)]` attribute and return its value.
///
/// Returns `None` if the method has no `#[tag]` attribute (the method is then
/// silently ignored — not every method in a trait needs a tag).
///
/// # Accepted forms
///
/// | Attribute syntax       | Returned value |
/// |------------------------|----------------|
/// | `#[tag(1000)]`         | `1000_i32`     |
/// | `#[tag(connect)]`      | `CONNECT`      |
/// | `#[tag(disconnect)]`   | `DISCONNECT`   |
/// | `#[tag(closed)]`       | `CLOSED`       |
///
/// # syn 2.0 migration notes
///
/// In syn **1.0** attributes were parsed via `attr.parse_meta()` which returned
/// a `Meta` containing a `Punctuated<NestedMeta, Comma>` list.
///
/// In syn **2.0**:
/// - `attr.meta` is a *field* — no method call needed.
/// - `NestedMeta` was **removed**; `Meta::List` now has a `tokens: TokenStream`
///   field.  To decode the tokens we call `syn::parse2::<T>(tokens)`, which
///   tries to parse them as a specific syn type.
fn have_tag(method: &TraitItemFn) -> Option<i32> {
    for attr in &method.attrs {
        // syn 2.0: `attr.path()` is a method (was a field in syn 1.0).
        if !attr.path().is_ident("tag") {
            continue; // skip unrelated attributes like `#[inline]`, `#[allow(...)]`
        }

        // syn 2.0: `attr.meta` is a directly accessible field of type `Meta`.
        // We expect the list form: `#[tag(...)]`.
        let Meta::List(ref list) = attr.meta else {
            panic!(
                "netxbuilder: #[tag] must be written as #[tag(...)], \
                 e.g. #[tag(1000)] or #[tag(connect)]"
            );
        };

        // ── Try `#[tag(1000)]` — integer literal ─────────────────────────────
        //
        // `list.tokens` is the raw token stream *inside* the parentheses.
        // `syn::parse2::<LitInt>` tries to parse those tokens as an integer
        // literal; this covers decimal, hex (0xFF), and other bases.
        if let Ok(lit) = syn::parse2::<LitInt>(list.tokens.clone()) {
            return Some(
                lit.base10_parse::<i32>()
                    .expect("netxbuilder: #[tag] integer value must fit in i32"),
            );
        }

        // ── Try `#[tag(connect)]` — lifecycle keyword ─────────────────────────
        //
        // `syn::parse2::<syn::Ident>` succeeds only when the token stream is a
        // single identifier, which is exactly what `connect` / `disconnect` /
        // `closed` are.
        if let Ok(ident) = syn::parse2::<syn::Ident>(list.tokens.clone()) {
            return Some(match ident.to_string().as_str() {
                "connect" => CONNECT,
                "disconnect" => DISCONNECT,
                "closed" => CLOSED,
                other => panic!(
                    "netxbuilder: unknown lifecycle tag '{}' — \
                     expected one of: connect, disconnect, closed",
                    other
                ),
            });
        }

        // Neither form matched — give a helpful compile-time error.
        panic!(
            "netxbuilder: #[tag] argument must be either an i32 literal \
             (e.g. #[tag(1000)]) or a lifecycle keyword \
             (connect | disconnect | closed)"
        );
    }

    // No `#[tag]` attribute found on this method.
    None
}

// ─── Return-type analysis ─────────────────────────────────────────────────────

/// Classify the return type of a tagged method into a *call type* (`tt`).
///
/// The `tt` value is sent over the wire in the protocol header so the remote
/// end knows whether to:
///   - ignore the result (`tt = 0`)
///   - only check for success/failure (`tt = 1`)
///   - deserialise and return a value (`tt = 2`)
///
/// # Accepted return types
///
/// | Rust return type | `tt` |
/// |------------------|------|
/// | (none / `()`)    |  `0` |
/// | `Result<()>`     |  `1` |
/// | `Result<T>`      |  `2` |
///
/// Any other return type causes a compile-time `panic!`.
fn get_function_tt(tag_id: i32, func_name: &str, rt: &Type) -> u8 {
    // The return type must be a path type (e.g. `anyhow::Result<T>` or
    // `Result<T>`).  Anything else (tuple, reference, impl Trait, …) is
    // rejected immediately.
    let Type::Path(tp) = rt else {
        panic!(
            "netxbuilder: tag {} fn '{}' — return type must be Result<T> or Result<()>, \
             got a non-path type",
            tag_id, func_name
        )
    };

    // Walk every path segment looking for one named `Result`.
    // This handles both `Result<T>` and `anyhow::Result<T>` / `std::result::Result<T, E>`.
    for seg in &tp.path.segments {
        if seg.ident != "Result" {
            continue;
        }

        return match &seg.arguments {
            // `Result<T>` or `Result<()>` — exactly one type argument.
            PathArguments::AngleBracketed(arg) if arg.args.len() == 1 => {
                match &arg.args[0] {
                    // `Result<()>` — the unit tuple means "confirm only".
                    GenericArgument::Type(Type::Tuple(t)) if t.elems.is_empty() => 1,
                    // `Result<T>` for any other T — return value.
                    _ => 2,
                }
            }
            // `Result<A, B>` or `Result<>` — wrong arity.
            PathArguments::AngleBracketed(_) => panic!(
                "netxbuilder: tag {} fn '{}' — Result must have exactly one \
                 type parameter, e.g. Result<T> or Result<()>",
                tag_id, func_name
            ),
            // `Result` with no angle brackets — not a valid return type here.
            _ => panic!(
                "netxbuilder: tag {} fn '{}' — Result must be generic, \
                 e.g. Result<T> or Result<()>",
                tag_id, func_name
            ),
        };
    }

    panic!(
        "netxbuilder: tag {} fn '{}' — could not find a `Result` type in the \
         return type; tagged methods must return Result<T> or Result<()>",
        tag_id, func_name
    )
}

// ─── Code-generation helpers ──────────────────────────────────────────────────

/// Generate the method bodies for the **proxy struct** that implements the trait.
///
/// The proxy struct wraps a client handle (`self.client`) and delegates every
/// call through the appropriate `call!` / `call_peer!` macro variant.
///
/// # Parameters
///
/// - `funcs`      — the list of tagged methods extracted from the trait.
/// - `call_macro` — the macro name to use for remote invocation:
///   - client-side proxy: `call`  (defined in `netxclient`)
///   - server-side proxy: `call_peer` (defined in `netxserver`)
///
/// # Generated code (example for `tt = 2`)
///
/// ```ignore
/// async fn login(&self, name: String, pwd: String) -> Result<String> {
///     Ok(call!(self.client => 1000; name, pwd,))
/// }
/// ```
fn get_impl_func(funcs: &[FuncInfo], call_macro: &Ident) -> Vec<proc_macro2::TokenStream> {
    funcs
        .iter()
        .map(|func| {
            let fn_name = format_ident!("{}", func.func_name);
            let inputs = &func.inputs; // full parameter list (re-emitted verbatim)
            let output = &func.output; // return type        (re-emitted verbatim)
            let input_names = &func.input_names; // bare argument names for forwarding
            let tag = func.tag; // numeric command ID

            match func.tt {
                // ── tt = 0: fire-and-forget ───────────────────────────────────
                // No `Result` return type; we send the message and move on.
                // `@run_not_err` swallows send errors and logs a warning instead
                // of propagating them.
                0 => quote! {
                    async fn #fn_name(#inputs) {
                        #call_macro!(@run_not_err self.client=>#tag;#(#input_names ,)*);
                    }
                },

                // ── tt = 1: confirm only ──────────────────────────────────────
                // Returns `Result<()>`; we wait for the remote `Ok(())` reply
                // but don't need to deserialise any payload.
                // `@checkrun` sends a type-1 packet and awaits confirmation.
                1 => quote! {
                    async fn #fn_name(#inputs) #output {
                        #call_macro!(@checkrun self.client=>#tag;#(#input_names ,)*);
                        Ok(())
                    }
                },

                // ── tt = 2: return value ──────────────────────────────────────
                // Returns `Result<T>`; the default `call!` form sends a type-2
                // packet, awaits the response, and deserialises `T`.
                2 => quote! {
                    async fn #fn_name(#inputs) #output {
                        Ok(#call_macro!(self.client=>#tag;#(#input_names ,)*))
                    }
                },

                // Guard against future `tt` values we haven't handled yet.
                tt => panic!(
                    "netxbuilder: internal error — unexpected call type tt={} \
                     for fn '{}'",
                    tt, func.func_name
                ),
            }
        })
        .collect()
}

/// Generate the `match cmd_tag { … }` arms for the `IController::call` method.
///
/// Each arm corresponds to one `#[tag(N)]`-annotated method and performs:
///
/// 1. **Type check** — `tt` must match what was agreed at compile time.
/// 2. **Arity check** — the argument count in the packet must match the method.
/// 3. **Deserialisation** — each argument is decoded from the binary packet.
/// 4. **Dispatch** — the trait method is called with the decoded arguments.
/// 5. **Response** — a `RetResult` is returned (empty for `tt=0/1`, with a
///    serialised return value for `tt=2`).
///
/// # Generated code (example for `tt = 2`, one String arg)
///
/// ```ignore
/// 1000 => {
///     ::anyhow::ensure!(tt == 2, "cmd:1000 tt:{} !=2", tt);
///     let args_len = data.read_fixed::<u32>()? as usize;
///     ::anyhow::ensure!(args_len == 1, "cmd:1000 args len error");
///     let arg0 = data.pack_deserialize::<String>()?;
///     let ret = IMyTrait::login(self, arg0,).await?;
///     let mut result = RetResult::success();
///     result.add_arg_buff(ret);
///     Ok(result)
/// }
/// ```
fn make_dispatch_arms(
    funcs: &[FuncInfo],
    interface_name: &syn::Ident,
) -> Vec<proc_macro2::TokenStream> {
    funcs
        .iter()
        .map(|func| {
            let func_name = format_ident!("{}", func.func_name);
            let tag = func.tag;

            // Build one `let argN = data.pack_deserialize::<ArgType>()?;` statement
            // and collect one `argN` ident for each non-self parameter.
            let mut arg_names: Vec<proc_macro2::Ident> = Vec::new();
            let mut read_token: Vec<proc_macro2::TokenStream> = Vec::new();

            for (index, token) in func.args_type.iter().enumerate() {
                // Use `arg0`, `arg1`, … as local variable names to avoid any
                // collision with existing identifiers in the generated code.
                let arg_name = format_ident!("arg{}", index);
                read_token.push(quote! {
                    // Deserialise one argument from the binary packet.
                    // `pack_deserialize` uses msgpack under the hood.
                    let #arg_name = data.pack_deserialize::<#token>()?;
                });
                arg_names.push(arg_name);
            }

            let args_len = func.args_type.len(); // known at compile time

            let call = match func.tt {
                // ── Dispatcher arm for tt = 0 (fire-and-forget) ──────────────
                0 => quote! {
                    // Verify the caller sent a type-0 packet for this command.
                    ::anyhow::ensure!(tt == 0, "cmd:{} tt:{} !=0", #tag, tt);
                    let args_len = data.read_fixed::<u32>()? as usize;
                    ::anyhow::ensure!(
                        args_len == #args_len,
                        "cmd:{} args len error: expected {}, got {}",
                        #tag, #args_len, args_len
                    );
                    #(#read_token)*
                    // Call the method; no return value is expected by the caller.
                    #interface_name::#func_name(self, #(#arg_names,)*).await;
                    Ok(RetResult::success())
                },

                // ── Dispatcher arm for tt = 1 (confirm only) ─────────────────
                1 => quote! {
                    ::anyhow::ensure!(tt == 1, "cmd:{} tt:{} !=1", #tag, tt);
                    let args_len = data.read_fixed::<u32>()? as usize;
                    ::anyhow::ensure!(
                        args_len == #args_len,
                        "cmd:{} args len error: expected {}, got {}",
                        #tag, #args_len, args_len
                    );
                    #(#read_token)*
                    // Call the method and propagate any error back to the caller.
                    #interface_name::#func_name(self, #(#arg_names,)*).await?;
                    Ok(RetResult::success())
                },

                // ── Dispatcher arm for tt = 2 (return value) ─────────────────
                2 => quote! {
                    ::anyhow::ensure!(tt == 2, "cmd:{} tt:{} !=2", #tag, tt);
                    let args_len = data.read_fixed::<u32>()? as usize;
                    ::anyhow::ensure!(
                        args_len == #args_len,
                        "cmd:{} args len error: expected {}, got {}",
                        #tag, #args_len, args_len
                    );
                    #(#read_token)*
                    // Call the method and serialise the return value into the response.
                    let ret = #interface_name::#func_name(self, #(#arg_names,)*).await?;
                    let mut result = RetResult::success();
                    result.add_arg_buff(ret);
                    Ok(result)
                },

                // Unreachable in practice; `get_function_tt` only returns 0/1/2.
                _ => quote! { unimplemented!() },
            };

            // Wrap the generated code in the match arm `tag => { ... }`.
            quote! {
                #tag => { #call }
            }
        })
        .collect()
}

/// Generate the `_ => { … }` catch-all arm that handles lifecycle events for
/// methods that the user chose **not** to implement.
///
/// If the user wrote `#[tag(connect)] async fn connect(…)`, that arm is already
/// covered by [`make_dispatch_arms`].  This fallback only fires when the runtime
/// receives a connect/disconnect/closed command for which no user-defined method
/// exists — in that case we silently return success.
///
/// Any other unknown `cmd_tag` returns an error that propagates back to the caller.
fn make_special_fallback() -> proc_macro2::TokenStream {
    quote! {
        _ => match cmd_tag {
            // Lifecycle events without a user-defined handler → succeed silently.
            #CONNECT    => Ok(RetResult::success()),
            #DISCONNECT => Ok(RetResult::success()),
            #CLOSED     => Ok(RetResult::success()),
            // Truly unknown command — surface an error so callers can log it.
            _ => ::anyhow::bail!("not found cmd tag:{}", cmd_tag),
        }
    }
}

// ─── Trait metadata extraction ────────────────────────────────────────────────

/// Walk every item in a `trait` definition and collect [`FuncInfo`] for each
/// method annotated with `#[tag(...)]`.
///
/// Methods without `#[tag]` are **silently skipped** — this lets users add
/// helper methods to their trait without triggering code generation.
///
/// # Panics
///
/// - If a tagged method is not `async`.
/// - If a parameter name is not a plain identifier (e.g. destructured patterns).
/// - If the return type is not `()`, `Result<()>`, or `Result<T>`.
///
/// # syn 2.0 migration note
///
/// In syn 1.0 the match arm was `TraitItem::Method(method)`.
/// In syn 2.0 it became `TraitItem::Fn(method)` and the type is `TraitItemFn`.
fn get_funcs_info(ast: &ItemTrait) -> Vec<FuncInfo> {
    let mut funcs = Vec::new();

    for item in &ast.items {
        // syn 2.0: `TraitItem::Fn` (was `TraitItem::Method` in syn 1.0).
        let TraitItem::Fn(method) = item else {
            continue; // type alias, const, associated type, etc. — skip
        };

        // Only process methods that have a `#[tag(...)]` attribute.
        let Some(tag_id) = have_tag(method) else {
            continue; // no tag → not a remote-callable method
        };

        let sig = &method.sig;

        // Only `async fn` can be remote-called; a non-async tagged method is
        // almost certainly a mistake, so we fail loudly.
        assert!(
            sig.asyncness.is_some(),
            "netxbuilder: method '{}' is tagged with #[tag({})] but is not async — \
             remote-callable methods must be declared `async fn`",
            sig.ident,
            tag_id
        );

        let func_name = sig.ident.to_string();
        let mut args_type = Vec::new();
        let mut input_names = Vec::new();

        for arg in &sig.inputs {
            // Skip `&self` / `&mut self` — only collect typed parameters.
            let FnArg::Typed(pat_type) = arg else {
                continue;
            };

            // Record the type token stream for the dispatcher's deserialisation code.
            let ty = &pat_type.ty;
            args_type.push(quote!(#ty));

            // Record the parameter name for argument forwarding in the proxy impl.
            // Only plain `name: Type` patterns are supported; destructuring is not.
            let Pat::Ident(a) = &*pat_type.pat else {
                panic!(
                    "netxbuilder: method '{}' — all parameters must use plain \
                     identifier patterns, e.g. `name: String` not `(a, b): (i32, i32)`",
                    func_name
                )
            };
            input_names.push(a.ident.clone());
        }

        // Determine `tt` from the declared return type.
        let tt = match &sig.output {
            ReturnType::Default => 0, // no `->` means fire-and-forget
            ReturnType::Type(_, ty) => get_function_tt(tag_id, &func_name, ty),
        };

        funcs.push(FuncInfo {
            tag: tag_id,
            tt,
            func_name,
            args_type,
            inputs: sig.inputs.clone(),
            input_names,
            output: sig.output.clone(),
        });
    }

    funcs
}

// ─── Public proc-macro entry points ──────────────────────────────────────────

/// Generate the **client-side** interface infrastructure from a `trait` definition.
///
/// This macro is re-exported as `build` in `netxclient::prelude`.
///
/// # Usage
///
/// ## Without a controller name (pure interface definition)
///
/// ```ignore
/// // In the CLIENT crate — define what the server exposes.
/// #[build_client]               // or `#[build]` via netxclient::prelude
/// pub trait IServer {
///     #[tag(1000)]
///     async fn login(&self, name: String) -> Result<String>;
/// }
/// ```
///
/// Generates only the **proxy struct** `___impl_IServer_call<T>` together with
/// `impl IServer for ___impl_IServer_call<Arc<Actor<NetXClient<T>>>>`.  Users
/// then obtain an `impl IServer` via `impl_owned_interface!(client => IServer)`.
///
/// ## With a controller name (server receives calls from clients)
///
/// ```ignore
/// // In the CLIENT crate — define what the server can call back on the client.
/// #[build_client(ClientController)]
/// pub trait IClientController {
///     #[tag(2001)]
///     async fn on_message(&self, from: String, text: String);
/// }
/// ```
///
/// Additionally, generates `impl IController for ClientController` which
/// deserialises and dispatches incoming packets.
#[proc_macro_attribute]
pub fn build_client(args: TokenStream, input: TokenStream) -> TokenStream {
    // Parse the trait definition from the input token stream.
    let ast = parse_macro_input!(input as ItemTrait);

    // Collect metadata for every `#[tag(...)]`-annotated method.
    let funcs = get_funcs_info(&ast);

    // The optional argument is the name of the controller struct that will
    // implement `IController`.  An empty string means "no controller".
    let controller_name = args.to_string();
    let interface_name = &ast.ident;

    // The generated proxy struct is given a mangled name to avoid conflicts:
    //   `___impl_IServer_call<T>`
    let proxy_name = format_ident!("___impl_{}_call", interface_name);
    // Client-side proxies use the `call!` macro family from `netxclient`.
    let call_macro = format_ident!("call");
    let impl_func = get_impl_func(&funcs, &call_macro);

    // ── Proxy struct + impls — generated regardless of whether a controller
    //    name was provided. ──────────────────────────────────────────────────
    //
    // The proxy is generic over `T` so it can hold either an owned `Arc` or a
    // borrowed `&Arc` — this avoids unnecessary clones when calling many methods
    // on the same connection object.
    let impl_interface = quote! {
        /// Auto-generated proxy struct for the `#interface_name` interface.
        ///
        /// Wrap a `NetXClient` handle to obtain an `impl #interface_name`:
        /// ```ignore
        /// let server: Box<dyn IServer> = impl_owned_interface!(client => IServer);
        /// ```
        #[allow(non_camel_case_types)]
        pub struct #proxy_name<T> {
            /// The underlying client handle used to send RPC packets.
            client: T,
        }

        impl<T> #proxy_name<T> {
            /// Create a proxy from any handle type `T`.
            ///
            /// Prefer the `impl_owned_interface!` / `impl_struct!` / `impl_ref!`
            /// convenience macros over calling this directly.
            pub fn new(client: T) -> #proxy_name<T> {
                #proxy_name { client }
            }
        }

        // Specialised constructor that returns `impl Trait` (opaque type) for
        // ergonomic use as `Box<dyn InterfaceName>`.
        impl<T: SessionSave + 'static> #proxy_name<std::sync::Arc<Actor<NetXClient<T>>>> {
            /// Wrap an owned `Arc<Actor<NetXClient<T>>>` and return an opaque
            /// `impl #interface_name`.  Used by `impl_struct!`.
            pub fn new_impl(
                client: std::sync::Arc<Actor<NetXClient<T>>>,
            ) -> impl #interface_name {
                #proxy_name { client }
            }
        }

        // Reference variant — avoids cloning the `Arc` when a short-lived proxy
        // is all that is needed (e.g. a single method call in a tight loop).
        impl<'a, T: SessionSave + 'static>
            #proxy_name<&'a std::sync::Arc<Actor<NetXClient<T>>>>
        {
            /// Wrap a reference to an `Arc` without cloning it.  Used by `impl_ref!`.
            pub fn new_impl_ref(
                client: &'a std::sync::Arc<Actor<NetXClient<T>>>,
            ) -> Self {
                #proxy_name { client }
            }
        }

        // The actual `impl Trait` for both owned and borrowed handles.
        // `async_trait` is required because traits with async methods need
        // boxing on stable Rust (until AFIT is stabilised for object-safe traits).
        #[async_trait::async_trait]
        impl<T: SessionSave + 'static> #interface_name
            for #proxy_name<std::sync::Arc<Actor<NetXClient<T>>>>
        {
            #(#impl_func)*
        }

        #[async_trait::async_trait]
        impl<'a, T: SessionSave + 'static> #interface_name
            for #proxy_name<&'a std::sync::Arc<Actor<NetXClient<T>>>>
        {
            #(#impl_func)*
        }
    };

    if !controller_name.is_empty() {
        // ── Controller dispatch ───────────────────────────────────────────────
        // When a controller name is given we additionally emit:
        //
        //   #[async_trait::async_trait]
        //   impl IController for ClientController {
        //       async fn call(&self, tt, cmd_tag, data) -> Result<RetResult> {
        //           match cmd_tag { <arms> <fallback> }
        //       }
        //   }
        let controller = format_ident!("{}", controller_name);
        let make = make_dispatch_arms(&funcs, interface_name);
        let fallback = make_special_fallback();

        let expanded = quote! {
            // Re-emit the original trait (with `async_trait` added).
            #[async_trait::async_trait]
            #ast

            // Implement `IController` so the runtime can dispatch incoming packets.
            #[async_trait::async_trait]
            impl IController for #controller {
                /// Deserialise and dispatch one incoming RPC packet.
                ///
                /// `tt`      — call type (0=fire-and-forget, 1=confirm, 2=return value)
                /// `cmd_tag` — the numeric command ID from the packet header
                /// `data`    — the remaining packet payload (arguments)
                #[inline]
                async fn call(
                    &self,
                    tt: u8,
                    cmd_tag: i32,
                    mut data: data_rw::DataOwnedReader,
                ) -> ::anyhow::Result<RetResult> {
                    match cmd_tag {
                        // One arm per `#[tag(N)]` method.
                        #(#make)*
                        // Lifecycle events and unknown commands.
                        #fallback
                    }
                }
            }
        };
        TokenStream::from(expanded)
    } else {
        // No controller name → emit only the trait + proxy struct.
        let expanded = quote! {
            #[async_trait::async_trait]
            #ast

            #impl_interface
        };
        TokenStream::from(expanded)
    }
}

/// Generate the **server-side** push interface from a `trait` definition.
///
/// This macro is re-exported as `build` in `netxserver::prelude`.
///
/// The generated proxy uses `call_peer!` instead of `call!` because on the
/// server side we push to a `NetxToken` (a connected peer handle) rather than
/// to a `NetXClient`.
///
/// # Usage
///
/// ## Without a controller name (server defines what clients expose)
///
/// ```ignore
/// // In the SERVER crate — define the interface the server can call on clients.
/// #[build_server]               // or `#[build]` via netxserver::prelude
/// pub trait IClient {
///     #[tag(2001)]
///     async fn on_message(&self, from: String, text: String);
/// }
/// // Then push to a client token:
/// let peer = impl_ref!(token => IClient);
/// peer.on_message("admin".into(), "hello!".into()).await;
/// ```
///
/// ## With a controller name (server handles calls from clients)
///
/// ```ignore
/// // In the SERVER crate — implement the server's incoming command handler.
/// #[build_server(ServerController)]
/// pub trait IServerController {
///     #[tag(connect)]
///     async fn connect(&self) -> Result<()>;
///     #[tag(1000)]
///     async fn login(&self, name: String) -> Result<String>;
/// }
/// ```
#[proc_macro_attribute]
pub fn build_server(args: TokenStream, input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as ItemTrait);
    let funcs = get_funcs_info(&ast);

    let controller_name = args.to_string();
    let interface_name = &ast.ident;
    let proxy_name = format_ident!("___impl_{}_call", interface_name);
    // Server-side proxies use `call_peer!` from `netxserver`.
    let call_macro = format_ident!("call_peer");
    let impl_func = get_impl_func(&funcs, &call_macro);

    // ── Proxy struct + impls ──────────────────────────────────────────────────
    //
    // The server-side proxy wraps a `NetxToken<T>` (owned or borrowed) instead
    // of a `NetXClient`.  `NetxToken` represents a single connected client
    // session from the server's perspective.
    let impl_interface = quote! {
        /// Auto-generated server-side proxy for the `#interface_name` interface.
        ///
        /// Use `impl_ref!(token => IClient)` to push to a connected client:
        /// ```ignore
        /// let peer = impl_ref!(token => IClient);
        /// peer.on_message("hello".into()).await;
        /// ```
        #[allow(non_camel_case_types)]
        pub struct #proxy_name<T> {
            /// The `NetxToken` handle for the target client session.
            client: T,
        }

        impl<T> #proxy_name<T> {
            /// Create a proxy from any `T` — typically a `NetxToken` or `&NetxToken`.
            pub fn new(client: T) -> #proxy_name<T> {
                #proxy_name { client }
            }
        }

        // Reference constructor — the most common form when calling from within
        // a server controller method where we already have `&token`.
        impl<'a, T: IController + 'static> #proxy_name<&'a NetxToken<T>> {
            /// Borrow a `NetxToken` reference without cloning the `Arc`.
            /// Used by the `impl_ref!` macro.
            pub fn new_ref(client: &'a NetxToken<T>) -> Self {
                #proxy_name { client }
            }
        }

        // Note: the server-side proxy does NOT add `#[async_trait]` because
        // `IAsyncToken::call` / `IAsyncToken::run` return concrete futures,
        // not `dyn Future`.  The trait methods are generated as `async fn`
        // which is handled by Rust's native RPIT.
        impl<T: IController + 'static> #interface_name for #proxy_name<NetxToken<T>> {
            #(#impl_func)*
        }

        impl<'a, T: IController + 'static> #interface_name
            for #proxy_name<&'a NetxToken<T>>
        {
            #(#impl_func)*
        }
    };

    if !controller_name.is_empty() {
        // ── Controller dispatch ───────────────────────────────────────────────
        let controller = format_ident!("{}", controller_name);
        let make = make_dispatch_arms(&funcs, interface_name);
        let fallback = make_special_fallback();

        let expanded = quote! {
            // Re-emit the original trait as-is (server traits don't need async_trait
            // at the trait level — the impl block adds it via `#[build_impl]`).
            #ast

            impl IController for #controller {
                /// Deserialise and dispatch one incoming RPC packet from a client.
                #[inline]
                async fn call(
                    &self,
                    tt: u8,
                    cmd_tag: i32,
                    mut data: data_rw::DataOwnedReader,
                ) -> ::anyhow::Result<RetResult> {
                    match cmd_tag {
                        #(#make)*
                        #fallback
                    }
                }
            }
        };
        TokenStream::from(expanded)
    } else {
        let expanded = quote! {
            #ast
            #impl_interface
        };
        TokenStream::from(expanded)
    }
}

/// Marker attribute for **server-side** `impl` blocks.
///
/// At the macro level this is currently a **no-op** — it re-emits the `impl`
/// block unchanged.  Its purposes are:
///
/// 1. **Validation** — `parse_macro_input!(input as ItemImpl)` ensures the
///    annotated item is syntactically a valid `impl` block.
/// 2. **Documentation** — signals to readers that this `impl` contains netx
///    remote-callable methods.
/// 3. **Future-proofing** — reserved for potential future transformations
///    (e.g. automatic span injection for distributed tracing).
#[proc_macro_attribute]
pub fn build_impl(_: TokenStream, input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as ItemImpl);
    TokenStream::from(quote! { #ast })
}

/// Marker attribute for **client-side** `impl` blocks.
///
/// Behaves like [`build_impl`] but additionally prepends
/// `#[async_trait::async_trait]` to the `impl` block.
///
/// This is needed because the client-side `IController` trait uses
/// `#[async_trait]`-generated boxing, and the impl must match.
///
/// ```ignore
/// #[build_impl]   // or `#[build_impl_client]` for netxclient::prelude
/// impl IClientController for ClientController {
///     async fn on_message(&self, from: String, text: String) { … }
/// }
/// ```
#[proc_macro_attribute]
pub fn build_impl_client(_: TokenStream, input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as ItemImpl);
    TokenStream::from(quote! {
        #[async_trait::async_trait]
        #ast
    })
}

/// Pass-through attribute that assigns a command ID to a trait method.
///
/// This attribute is consumed by [`build_client`] and [`build_server`] via
/// [`have_tag`].  Its own proc-macro implementation is intentionally a
/// **no-op** so that `rustc` does not reject the attribute when it appears
/// on a method in a regular (non-netx) context.
///
/// # Accepted forms
///
/// ```ignore
/// #[tag(1000)]          // arbitrary i32 command ID
/// #[tag(connect)]       // lifecycle: connection established
/// #[tag(disconnect)]    // lifecycle: TCP connection dropped
/// #[tag(closed)]        // lifecycle: session token garbage-collected
/// ```
#[proc_macro_attribute]
pub fn tag(_: TokenStream, input: TokenStream) -> TokenStream {
    // Simply return the annotated item unchanged; all real work happens in
    // `build_client` / `build_server` when they call `get_funcs_info`.
    input
}
