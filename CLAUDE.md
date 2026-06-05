# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
# Build the entire workspace
cargo build

# Build a specific crate
cargo build -p netxserver
cargo build -p netxclient

# Run all tests (workspace-wide)
cargo test

# Run tests for a specific crate
cargo test -p netxserver

# Run a specific test
cargo test -p netxserver <test_name>

# Check (fast compile without codegen)
cargo check

# Lint
cargo clippy -- -D warnings

# Format check
cargo fmt --all -- --check

# Run an example (chat server)
cargo run -p msgserver
cargo run -p msgclient

# Run TLS examples
cargo run -p server_tls   # examples/tls/server
cargo run -p client_tls   # examples/tls/client

# Build with specific feature flags
cargo build -p netxserver --no-default-features --features use_rustls
cargo build -p netxclient --no-default-features --features use_openssl
```

## Workspace Structure

```
netx/                          (workspace root)
├── Cargo.toml                 — workspace members + dependencies
├── netx_builder/              — proc-macro crate (the codegen engine)
├── netx_server/               — server library
├── netx_client/               — client library
└── examples/
    ├── msg/packer/            — shared message types for the chat example
    ├── msg/msgserver/         — chat server example
    ├── msg/msgclient/         — chat client example (CLI)
    ├── msg/msgclient_tui/     — chat client example (TUI)
    ├── tls/server/            — TLS server example
    └── tls/client/            — TLS client example
```

## Architecture Overview

This is a **bidirectional RPC framework** where both client and server can call each other's methods. A user defines an `async trait` annotated with `#[tag(N)]` and proc-macros (`netxbuilder`) generate:

1. **A proxy struct** (`___impl_<Trait>_call<T>`) that implements the trait — used by the caller side to send RPC packets
2. **An `IController` dispatch impl** (when a controller name is given) that deserializes incoming packets and routes them to the correct method

### The three crates in detail

**`netxbuilder`** (proc-macro):
- Entry points: `#[build_client]`, `#[build_server]`, `#[build_impl]`, `#[build_impl_client]`, `#[tag(N)]`
- `build_client` / `build_server` each walk the trait AST, extract method metadata (`FuncInfo`), and emit:
  - Proxy struct (generic over `T` — owned `Arc` or borrowed `&Arc`) with `async_trait` impl
  - Optionally, an `IController` impl with a `match cmd_tag { ... }` dispatch arm per method
- `build_impl` on the server side is a no-op (parses to validate syntax). On the client side, `build_impl_client` prepends `#[async_trait]`.
- `#[tag(connect)]`, `#[tag(disconnect)]`, `#[tag(closed)]` map to reserved command IDs at the top of `i32` range (i32::MAX, MAX-1, MAX-2)
- Return type determines call type (`tt`): no return → `tt=0` (fire-and-forget), `Result<()>` → `tt=1` (confirm), `Result<T>` → `tt=2` (return value)

**`netxserver`** (server library):
- `NetXServer<T>` — wraps `tcpserver::Builder` (or `tcp-channel-server`), owns a `TokenManager<T>` for session management
- `NetxToken<T>` = `Arc<Actor<AsyncToken<T>>>` — one per connected client; holds the peer handle and the controller instance
- `AsyncTokenManager` — manages token lifecycle (create, get, disconnect, timeout/cleanup)
- `IController` — trait with `async fn call(tt, cmd_tag, data) -> Result<RetResult>`; the generated dispatch impls use this
- `ICreateController` — factory trait: `fn create_controller(token) -> Result<Arc<Controller>>`; called per new connection
- `ServerOption` — addr, service_name, verify_key, request_out_time, session_save_time, max_connections, is_nodelay
- Server-side `RetResult` uses the **same** struct definition as the client side but reads `len` as `i32` (not `u32`) — they are separate copies in `netx_server::server::result` and `netx_client::client::result`

**`netxclient`** (client library):
- `NetXClient<T>` — core struct containing server_info, session store `T: SessionSave`, peer handle, result dictionary, request manager, controller
- `NetxClientArc<T>` = `Arc<Actor<NetXClient<T>>>` (the standard handle type)
- `NetxClientArcDef` = `Arc<Actor<NetXClient<DefaultSessionStore>>>` (most common)
- `SessionSave` trait — `get_session_id()` / `store_session_id()`; `DefaultSessionStore` provides in-memory-only storage
- `RequestManager` — actor that tracks in-flight request serials with timeouts
- `ServerOption` — addr, service_name, verify_key, request_out_time_ms (note: slightly different from server-side `ServerOption`)
- The `call!` macro — provides `@run` (tt=0), `@run_not_err` (tt=0, swallow errors), `@checkrun` (tt=1), and default form (tt=2). Auto-reconnects if disconnected.

### Wire protocol

Messages are length-delimited binary frames using `data-rw` (msgpack-based serialization). Four command types:

| Command | Direction | Purpose |
|---------|-----------|---------|
| `1000` | Client→Server then Server→Client | Connection handshake (service_name, verify_key, session_id) + verification response |
| `2000` | Client→Server then Server→Client | Session ID negotiation (client requests, server assigns) |
| `2400` | Bidirectional | RPC call: `[tt: u8, cmd_tag: i32, serial: i64, args_count: i32, args...]` |
| `2500` | Bidirectional | RPC response: `[serial: i64, is_error: bool, payload...]` |

Two framing modes: mode 0 (no length prefix on 2400/2500 — `tcpserver`/`tcpclient`) and mode 1 (explicit u32 length prefix — `tcp-channel-server`/`tcp-channel-client`).

### Concurrency model

The framework uses **`aqueue::Actor`** everywhere: `NetXClient<T>` itself is wrapped in `Actor`, and each `AsyncToken<T>` on the server side is also wrapped in `Actor`. All mutable state access goes through `inner_call(|inner| async move { ... })`. This eliminates explicit lock contention — the actor serializes mutations.

### Feature flags (`cfg_if!` controlled)

- Transport: `tcpserver` (default server) vs `tcp-channel-server`; `tcpclient` (default client) vs `tcp-channel-client` — mutually exclusive pairs
- TLS: `use_openssl` / `use_rustls` — each crate uses `cfg_if!` blocks to generate `new_ssl`/`new_tls` constructor variants and the `TlsConfig` enum
- Serialization: `dserde` (msgpack), `jserde` (JSON) — passed through to `data-rw`
- `backtrace` — enables `anyhow` backtraces

### Key macros (client side)

```rust
call!(client => cmd_tag; arg1, arg2)         // tt=2, return value
call!(@checkrun client => cmd_tag; arg1)     // tt=1, confirm only
call!(@run client => cmd_tag; arg1)          // tt=0, fire-and-forget
call!(@run_not_err client => cmd_tag; arg1)  // tt=0, log errors

impl_ref!(client => IServer)               // borrow &Arc, no clone
impl_struct!(client => IServer)             // clone Arc, returns impl Trait
impl_interface!(client => IServer)          // clone Arc, returns Box<dyn Trait>
impl_owned_interface!(client => IServer)    // take ownership, returns Box<dyn Trait>
```

### MSRV

Minimum supported Rust version: **1.75+**