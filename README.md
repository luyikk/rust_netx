# NETX RUST
[![Rust Report Card](https://rust-reportcard.xuri.me/badge/github.com/luyikk/rust_netx)](https://rust-reportcard.xuri.me/report/github.com/luyikk/rust_netx)
[![Rust CI](https://github.com/luyikk/rust_netx/actions/workflows/rust.yml/badge.svg)](https://github.com/luyikk/rust_netx/actions/workflows/rust.yml)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE)
**[English](#english) | [中文](#chinese)**
---
<a name="english"></a>
# NETX RUST — English
> **A high-performance bidirectional RPC framework for Rust** — interface-model driven, easy to code, maintain, and use.
- Minimum supported Rust version: **1.75+**
- Supports **bidirectional method calls** between client and server
- Proc-macro driven: automatically generates serialization and routing code — zero boilerplate
- Optional **TLS** encryption via OpenSSL or Rustls
- .NET implementation: [github.com/luyikk/NetX](https://github.com/luyikk/NetX)
---
## Table of Contents (English)
- [Features](#features)
- [Project Structure](#project-structure)
- [Quick Start](#quick-start)
  - [Add Dependencies](#add-dependencies)
  - [Define Interfaces](#define-interfaces)
  - [Server Implementation](#server-implementation)
  - [Client Implementation](#client-implementation)
- [Macro Reference — Interface](#macro-reference--interface)
- [Macro Reference — Calling](#macro-reference--calling)
- [TLS Support](#tls-support)
- [Configuration](#configuration)
- [Examples](#examples)
- [Related Projects](#related-projects)
---
## Features
| Feature | Description |
|---------|-------------|
| 🔁 Bidirectional RPC | Server can actively call client methods (Push); client can call server methods |
| 🔖 Interface-driven | A single `trait` definition generates both caller proxy code and router dispatch code via proc-macros |
| ♻️ Session Recovery | Clients reconnect with their old `session_id`; the server seamlessly restores context |
| ⚡ High Performance | Built on `tokio` async runtime + `Actor` concurrency model — no explicit lock contention |
| 🔒 TLS Encryption | Optional `use_openssl` or `use_rustls` feature for encrypted transport |
| 🔌 Auto-reconnect | Detects disconnection and reconnects automatically on next call |
| ⏱️ Request Timeout | Built-in timeout management prevents calls from blocking indefinitely |
---
## Project Structure
```
netx (Workspace)
├── netx_builder/     — Proc-macro crate (code generation)
├── netx_server/      — Server library
├── netx_client/      — Client library
└── examples/
    ├── msg/          — Chat room example
    │   ├── packer/       — Shared data structures
    │   ├── msgserver/    — Message server
    │   ├── msgclient/    — Message client (CLI)
    │   └── msgclient_tui/ — Message client (TUI)
    └── tls/          — TLS encrypted communication example
        ├── server/
        └── client/
```
---
## Quick Start
### Add Dependencies
**Server `Cargo.toml`:**
```toml
[dependencies]
netxserver = "3"
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"
anyhow = "1"
```
**Client `Cargo.toml`:**
```toml
[dependencies]
netxclient = "3"
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"
anyhow = "1"
```
### Optional Features
Both `netxserver` and `netxclient` expose the following optional Cargo features:
| Feature | Description |
|---------|-------------|
| `use_openssl` | Enable OpenSSL-based TLS transport |
| `use_rustls` | Enable Rustls-based TLS transport |
| `dserde` | Enable binary (msgpack) serialization via `data-rw` |
| `jserde` | Enable JSON serialization via `data-rw` |
| `backtrace` | Enable `anyhow` backtraces in error output |
---
### Define Interfaces
Both sides define RPC interfaces using `trait`. Each method is annotated with a unique command ID via `#[tag(N)]`.
**Server interface (called by the client):**
```rust
use netxserver::prelude::*;
#[build(ServerController)]  // Bind to ServerController implementation
pub trait IServerController {
    #[tag(connect)]          // Called when a client connects
    async fn connect(&self) -> Result<()>;
    #[tag(disconnect)]       // Called when a client disconnects
    async fn disconnect(&self) -> Result<()>;
    #[tag(closed)]           // Called when the token is cleaned up
    async fn close(&self) -> Result<()>;
    #[tag(1000)]
    async fn login(&self, username: String, password: String) -> Result<String>;
    #[tag(1001)]
    async fn get_data(&self) -> Result<Vec<u8>>;
    #[tag(1002)]
    async fn notify(&self, msg: String) -> Result<()>;  // Result<()>: confirm only
    #[tag(1003)]
    async fn fire_and_forget(&self, data: String);      // No return value
}
```
**Client interface (called/pushed by the server):**
```rust
use netxclient::prelude::*;
#[build(ClientController)]
pub trait IClientController {
    #[tag(2001)]
    async fn on_message(&self, from: String, content: String);
    #[tag(2002)]
    async fn on_ping(&self, time: i64) -> Result<i64>;
}
```
> **Three return types, three call modes:**
> - No return value (`async fn foo()`) → fire-and-forget, no response awaited
> - `Result<()>` → wait for success/failure confirmation
> - `Result<T>` → wait for response and deserialize return value
---
### Server Implementation
```rust
use netxserver::prelude::*;
use std::sync::Arc;
use anyhow::Result;
pub struct ServerController {
    token: NetxToken<Self>,
}
#[build_impl]
impl IServerController for ServerController {
    async fn connect(&self) -> Result<()> {
        println!("client {} connected", self.token.get_session_id());
        Ok(())
    }
    async fn disconnect(&self) -> Result<()> {
        println!("client {} disconnected", self.token.get_session_id());
        Ok(())
    }
    async fn close(&self) -> Result<()> { Ok(()) }
    async fn login(&self, username: String, _password: String) -> Result<String> {
        Ok(format!("welcome, {}!", username))
    }
    async fn get_data(&self) -> Result<Vec<u8>> { Ok(vec![1, 2, 3]) }
    async fn notify(&self, msg: String) -> Result<()> {
        println!("notify: {}", msg);
        Ok(())
    }
    async fn fire_and_forget(&self, data: String) {
        println!("received: {}", data);
    }
}
// Create a controller instance for each connection
pub struct ImplCreateController;
impl ICreateController for ImplCreateController {
    type Controller = ServerController;
    fn create_controller(&self, token: NetxToken<Self::Controller>) -> Result<Arc<Self::Controller>> {
        Ok(Arc::new(ServerController { token }))
    }
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let option = ServerOption::new("0.0.0.0:8000", "myservice", "secret_key");
    let server = NetXServer::new(option, ImplCreateController).await;
    server.start_block().await?;
    Ok(())
}
```
**Server pushing to a client:**
```rust
// Inside a server controller method — get another client token and call its interface
async fn broadcast(&self, target_session_id: i64, msg: String) -> Result<()> {
    if let Some(token) = self.token.get_token(target_session_id).await? {
        let client = impl_ref!(token => IClient);  // IClient defined on the server side
        client.on_message("server".to_string(), msg).await;
    }
    Ok(())
}
```
---
### Client Implementation
```rust
use netxclient::prelude::*;
use anyhow::Result;
pub struct ClientController {
    // NetxClientArcDef is a type alias for Arc<Actor<NetXClient<DefaultSessionStore>>>
    server: Box<dyn IServer>,
}
impl ClientController {
    pub fn new(client: NetxClientArcDef) -> Self {
        ClientController {
            server: impl_owned_interface!(client => IServer),
        }
    }
}
#[build_impl]
impl IClientController for ClientController {
    async fn on_message(&self, from: String, content: String) {
        println!("[{}] {}", from, content);
    }
    async fn on_ping(&self, time: i64) -> Result<i64> {
        Ok(time)  // Echo back; server calculates RTT
    }
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let server_info = ServerOption::new(
        "127.0.0.1:8000".to_string(),
        "myservice".to_string(),
        "secret_key".to_string(),
        5000,  // request timeout ms
    );
    // DefaultSessionStore does not persist the session across process restarts
    // NetXClient::new returns NetxClientArcDef (Arc<Actor<NetXClient<DefaultSessionStore>>>)
    let client = NetXClient::new(server_info, DefaultSessionStore::default());
    // Register the controller (server will call methods on this)
    client.init(ClientController::new(client.clone())).await;
    // Connect to the server
    client.connect_network().await?;
    // Obtain a server interface proxy
    let server = impl_ref!(client => IServer);
    // Call a server method
    let msg = server.login("alice".to_string(), "password".to_string()).await?;
    println!("{}", msg);
    Ok(())
}
```
---
## Macro Reference — Interface
### `#[build(ControllerName)]` / `#[build]`
Applied to a **`trait`**. Automatically generates routing dispatch and caller proxy code.
| Usage | Description |
|-------|-------------|
| `#[build(ControllerName)]` | Generates `IController` impl for `ControllerName` (dispatch) + caller proxy struct |
| `#[build]` | Generates only the caller proxy struct (pure interface, no dispatch) |
> In the server `prelude`, `build` = `build_server`. In the client `prelude`, `build` = `build_client`.
### `#[build_impl]`
Applied to an **`impl`** block.
| Context | Behavior |
|---------|----------|
| Server (`netxserver::prelude`) | No-op — validates that the impl block parses correctly; native `async fn` in impl blocks is supported directly (no `async_trait` needed) |
| Client (`netxclient::prelude`) | Prepends `#[async_trait::async_trait]` to the impl block (required for client-side trait dispatch) |
### `#[tag(N)]`
Annotates a trait method with a command ID (`i32`). Client and server must use the same IDs.
| Special Tag | Triggered When |
|-------------|----------------|
| `#[tag(connect)]` | After client TCP connection and authentication succeed |
| `#[tag(disconnect)]` | After client TCP connection drops |
| `#[tag(closed)]` | When the server-side token is garbage-collected |
---
## Macro Reference — Calling
### Client → Server
```rust
// Call and deserialize result T
call!(client => 1000; arg1, arg2)
// Call and confirm success/failure only, returns Result<()>
call!(@checkrun client => 1001; arg1)
// Fire-and-forget, no response awaited
call!(@run client => 1002; arg1)
// Fire-and-forget, errors are only logged
call!(@run_not_err client => 1002; arg1)
```
### Generate Interface Proxy
```rust
// Takes ownership, returns Box<dyn IServer>
let server: Box<dyn IServer> = impl_owned_interface!(client => IServer);
// Clones the Arc, returns impl IServer
let server = impl_struct!(client => IServer);
// Reference version, no clone (most efficient for single calls)
let server = impl_ref!(client => IServer);
// Clones the Arc, returns Box<dyn IServer>
let server: Box<dyn IServer> = impl_interface!(client => IServer);
```
### Server → Client (Push)
```rust
// Obtain a client interface proxy via token
let peer = impl_ref!(token => IClient);
peer.on_message("hello".to_string()).await;
```
### Useful Type Aliases (Client)
```rust
// Arc<Actor<NetXClient<T>>> — generic over session store type
type NetxClientArc<T> = Arc<Actor<NetXClient<T>>>;
// Arc<Actor<NetXClient<DefaultSessionStore>>> — most common usage
type NetxClientArcDef = Arc<Actor<NetXClient<DefaultSessionStore>>>;
```
---
## TLS Support
### Using Rustls
```toml
# Server
netxserver = { version = "3", default-features = false, features = ["use_rustls"] }
# Client
netxclient = { version = "3", default-features = false, features = ["use_rustls"] }
```
```rust
// Server
let acceptor: &'static TlsAcceptor = /* build TlsAcceptor */;
let server = NetXServer::new_tls(acceptor, option, ImplCreateController).await;
// Client (RustlsAcceptAnyCertVerifier is re-exported from netxclient::prelude)
use netxclient::prelude::RustlsAcceptAnyCertVerifier;
let client = NetXClient::new_tls(server_info, session, domain, connector);
```
### Using OpenSSL
```toml
netxserver = { version = "3", default-features = false, features = ["use_openssl"] }
netxclient = { version = "3", default-features = false, features = ["use_openssl"] }
```
```rust
// Server
let server = NetXServer::new_ssl(ssl_acceptor, option, ImplCreateController).await;
// Client
let client = NetXClient::new_ssl(server_info, session, domain, connector);
```
See [`ca_test/`](ca_test/) for test certificate generation scripts.
---
## Configuration
### Server (`ServerOption`)
```json
{
  "addr": "0.0.0.0:8000",
  "service_name": "myservice",
  "verify_key": "secret_key",
  "request_out_time": 5000,
  "session_save_time": 5000
}
```
| Field | Type | Description |
|-------|------|-------------|
| `addr` | `String` | Listen address and port |
| `service_name` | `String` | Service name; clients must match (empty = no check) |
| `verify_key` | `String` | Connection key; clients must match (empty = no check) |
| `request_out_time` | `u32` | Request timeout in milliseconds (default 5000) |
| `session_save_time` | `u32` | How long to retain an offline session in milliseconds (default 5000) |
### Client (`ServerOption`)
```json
{
  "addr": "127.0.0.1:8000",
  "service_name": "myservice",
  "verify_key": "secret_key",
  "request_out_time_ms": 5000
}
```
| Field | Type | Description |
|-------|------|-------------|
| `addr` | `String` | Server address and port |
| `service_name` | `String` | Service name |
| `verify_key` | `String` | Connection key |
| `request_out_time_ms` | `u32` | Request timeout in milliseconds |
---
## Examples
### Chat Room Example
**Start the server:**
```bash
cd examples/msg/msgserver
cargo run
```
**Start the client:**
```bash
cd examples/msg/msgclient
cargo run
```
**Client commands:**
```
--help                        Show all commands
--online                      List all online users
--to [nickname] [msg]         Send a private message to a user
--ping [nickname] [count]     Ping a user and display RTT
[message]                     Broadcast a message to all users
```
### TLS Example
```bash
# Generate test certificates first
cd ca_test && bash tls_gen.sh
# Start TLS server
cd examples/tls/server && cargo run
# Start TLS client
cd examples/tls/client && cargo run
```
---
## Related Projects
- [**rust_netx examples**](https://github.com/luyikk/rust_netx/tree/master/examples) — More complete examples
- [**file-store-server**](https://github.com/luyikk/file-store-server) — File storage server built on netx
- [**file-store-client**](https://github.com/luyikk/file-store-client) — File storage client built on netx
- [**NetX (.NET)**](https://github.com/luyikk/NetX) — .NET implementation
---
## License
Licensed under either [MIT](LICENSE) or [Apache-2.0](LICENSE) at your option.
---
---
<a name="chinese"></a>
# NETX RUST — 中文
> **Rust 高性能双向 RPC 框架** — 接口模型驱动，易于编码、维护和使用。
- 最低支持 Rust 版本：**1.75+**
- 支持客户端与服务端**双向互相调用**对方方法
- 基于过程宏自动生成序列化/路由代码，无样板代码
- 支持 **TLS**（OpenSSL / Rustls）可选加密传输
- .NET 版本实现：[github.com/luyikk/NetX](https://github.com/luyikk/NetX)
---
## 目录（中文）
- [功能特性](#功能特性)
- [项目结构](#项目结构)
- [快速开始](#快速开始)
  - [添加依赖](#添加依赖)
  - [定义共享接口](#定义共享接口)
  - [实现服务端](#实现服务端)
  - [实现客户端](#实现客户端)
- [接口宏说明](#接口宏说明)
- [调用宏说明](#调用宏说明)
- [TLS 支持](#tls-支持)
- [配置说明](#配置说明)
- [示例](#示例)
- [相关项目](#相关项目)
---
## 功能特性
| 特性 | 说明 |
|------|------|
| 🔁 双向 RPC | 服务端可主动调用客户端方法（Push），客户端也可调用服务端方法 |
| 🔖 接口驱动 | 通过过程宏从同一 `trait` 定义自动生成调用方代码与路由分发代码 |
| ♻️ Session 恢复 | 客户端断线重连后携带旧 `session_id`，服务端可无缝恢复上下文 |
| ⚡ 高性能 | 基于 `tokio` 异步运行时 + `Actor` 并发模型，无显式锁竞争 |
| 🔒 TLS 加密 | 可选 `use_openssl` 或 `use_rustls` feature 启用加密传输 |
| 🔌 自动重连 | 客户端调用时检测到断线会自动触发重连 |
| ⏱️ 请求超时 | 内置请求超时管理，防止调用永久阻塞 |
---
## 项目结构
```
netx (Workspace)
├── netx_builder/     — 过程宏库（代码生成）
├── netx_server/      — 服务端库
├── netx_client/      — 客户端库
└── examples/
    ├── msg/          — 聊天室示例
    │   ├── packer/       — 共享数据结构
    │   ├── msgserver/    — 消息服务端
    │   ├── msgclient/    — 消息客户端（命令行）
    │   └── msgclient_tui/ — 消息客户端（TUI 界面）
    └── tls/          — TLS 加密通信示例
        ├── server/
        └── client/
```
---
## 快速开始
### 添加依赖
**服务端 `Cargo.toml`：**
```toml
[dependencies]
netxserver = "3"
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"
anyhow = "1"
```
**客户端 `Cargo.toml`：**
```toml
[dependencies]
netxclient = "3"
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"
anyhow = "1"
```
### 可选 Feature
`netxserver` 和 `netxclient` 均提供以下可选 Cargo feature：
| Feature | 说明 |
|---------|------|
| `use_openssl` | 启用基于 OpenSSL 的 TLS 传输 |
| `use_rustls` | 启用基于 Rustls 的 TLS 传输 |
| `dserde` | 通过 `data-rw` 启用二进制（msgpack）序列化 |
| `jserde` | 通过 `data-rw` 启用 JSON 序列化 |
| `backtrace` | 在错误输出中启用 `anyhow` 回溯 |
---
### 定义共享接口
客户端和服务端通过共享的 `trait` 定义 RPC 接口，每个方法用 `#[tag(N)]` 标注唯一命令 ID。
**服务端接口（供客户端调用）：**
```rust
use netxserver::prelude::*;
#[build(ServerController)]  // 绑定到 ServerController 实现
pub trait IServerController {
    #[tag(connect)]          // 连接时回调
    async fn connect(&self) -> Result<()>;
    #[tag(disconnect)]       // 断开时回调
    async fn disconnect(&self) -> Result<()>;
    #[tag(closed)]           // Token 被清理时回调
    async fn close(&self) -> Result<()>;
    #[tag(1000)]
    async fn login(&self, username: String, password: String) -> Result<String>;
    #[tag(1001)]
    async fn get_data(&self) -> Result<Vec<u8>>;
    #[tag(1002)]
    async fn notify(&self, msg: String) -> Result<()>;  // Result<()> 只确认
    #[tag(1003)]
    async fn fire_and_forget(&self, data: String);      // 无返回值
}
```
**客户端接口（供服务端主动推送）：**
```rust
use netxclient::prelude::*;
#[build(ClientController)]
pub trait IClientController {
    #[tag(2001)]
    async fn on_message(&self, from: String, content: String);
    #[tag(2002)]
    async fn on_ping(&self, time: i64) -> Result<i64>;
}
```
> **三种返回类型对应三种调用模式：**
> - 无返回值（`async fn foo()`）→ fire-and-forget，不等待响应
> - `Result<()>` → 等待确认成功/失败
> - `Result<T>` → 等待响应并反序列化返回值
---
### 实现服务端
```rust
use netxserver::prelude::*;
use std::sync::Arc;
use anyhow::Result;
pub struct ServerController {
    token: NetxToken<Self>,
}
#[build_impl]
impl IServerController for ServerController {
    async fn connect(&self) -> Result<()> {
        println!("client {} connected", self.token.get_session_id());
        Ok(())
    }
    async fn disconnect(&self) -> Result<()> {
        println!("client {} disconnected", self.token.get_session_id());
        Ok(())
    }
    async fn close(&self) -> Result<()> { Ok(()) }
    async fn login(&self, username: String, _password: String) -> Result<String> {
        Ok(format!("welcome, {}!", username))
    }
    async fn get_data(&self) -> Result<Vec<u8>> { Ok(vec![1, 2, 3]) }
    async fn notify(&self, msg: String) -> Result<()> {
        println!("notify: {}", msg);
        Ok(())
    }
    async fn fire_and_forget(&self, data: String) {
        println!("received: {}", data);
    }
}
// 为每个连接创建控制器实例
pub struct ImplCreateController;
impl ICreateController for ImplCreateController {
    type Controller = ServerController;
    fn create_controller(&self, token: NetxToken<Self::Controller>) -> Result<Arc<Self::Controller>> {
        Ok(Arc::new(ServerController { token }))
    }
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let option = ServerOption::new("0.0.0.0:8000", "myservice", "secret_key");
    let server = NetXServer::new(option, ImplCreateController).await;
    server.start_block().await?;
    Ok(())
}
```
**服务端主动推送到客户端：**
```rust
// 在服务端控制器中，通过 token 获取其他客户端并调用其接口
async fn broadcast(&self, target_session_id: i64, msg: String) -> Result<()> {
    if let Some(token) = self.token.get_token(target_session_id).await? {
        let client = impl_ref!(token => IClient);  // IClient 为服务端定义的客户端接口
        client.on_message("server".to_string(), msg).await;
    }
    Ok(())
}
```
---
### 实现客户端
```rust
use netxclient::prelude::*;
use anyhow::Result;
pub struct ClientController {
    // NetxClientArcDef 是 Arc<Actor<NetXClient<DefaultSessionStore>>> 的类型别名
    server: Box<dyn IServer>,
}
impl ClientController {
    pub fn new(client: NetxClientArcDef) -> Self {
        ClientController {
            server: impl_owned_interface!(client => IServer),
        }
    }
}
#[build_impl]
impl IClientController for ClientController {
    async fn on_message(&self, from: String, content: String) {
        println!("[{}] {}", from, content);
    }
    async fn on_ping(&self, time: i64) -> Result<i64> {
        Ok(time)  // 原样返回，服务端计算 RTT
    }
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let server_info = ServerOption::new(
        "127.0.0.1:8000".to_string(),
        "myservice".to_string(),
        "secret_key".to_string(),
        5000,
    );
    // 创建客户端（DefaultSessionStore 不持久化 session）
    // NetXClient::new 返回 NetxClientArcDef
    let client = NetXClient::new(server_info, DefaultSessionStore::default());
    // 注册控制器（服务端会调用此控制器中的方法）
    client.init(ClientController::new(client.clone())).await;
    // 连接到服务端
    client.connect_network().await?;
    // 获取服务端接口代理
    let server = impl_ref!(client => IServer);
    // 调用服务端方法
    let msg = server.login("alice".to_string(), "password".to_string()).await?;
    println!("{}", msg);
    Ok(())
}
```
---
## 接口宏说明
### `#[build(ControllerName)]` / `#[build]`
用于 **`trait`** 上，自动生成路由分发和调用代理代码。
| 用法 | 说明 |
|------|------|
| `#[build(ControllerName)]` | 生成 `ControllerName` 的路由分发实现 + 调用代理结构体 |
| `#[build]` | 只生成调用代理结构体，不生成路由分发（纯接口定义） |
> - 服务端 `prelude` 中 `build` = `build_server`，客户端 `prelude` 中 `build` = `build_client`
### `#[build_impl]`
用于 **`impl`** 块上。
| 使用场景 | 行为 |
|----------|------|
| 服务端（`netxserver::prelude`） | 空操作 — 仅验证 impl 块语法正确；服务端 impl 块原生支持 `async fn`，无需 `async_trait` |
| 客户端（`netxclient::prelude`） | 自动为 impl 块添加 `#[async_trait::async_trait]`（客户端 trait 分发所必须） |
### `#[tag(N)]`
为 trait 方法标注命令 ID（`i32`），客户端和服务端必须对应一致。
| 特殊标签 | 触发时机 |
|----------|----------|
| `#[tag(connect)]` | 客户端 TCP 连接并验证成功后 |
| `#[tag(disconnect)]` | 客户端 TCP 连接断开后 |
| `#[tag(closed)]` | 服务端 Token 被回收清理时 |
---
## 调用宏说明
### 客户端调用服务端
```rust
// 调用并返回反序列化结果 T
call!(client => 1000; arg1, arg2)
// 只确认成功/失败，返回 Result<()>
call!(@checkrun client => 1001; arg1)
// fire-and-forget，不等待响应
call!(@run client => 1002; arg1)
// fire-and-forget，出错只记录日志不 panic
call!(@run_not_err client => 1002; arg1)
```
### 生成接口代理对象
```rust
// 获取所有权，生成 Box<dyn IServer>
let server: Box<dyn IServer> = impl_owned_interface!(client => IServer);
// 克隆 Arc，生成 impl IServer
let server = impl_struct!(client => IServer);
// 引用版本，不克隆（单次调用最高效）
let server = impl_ref!(client => IServer);
// 克隆 Arc，生成 Box<dyn IServer>
let server: Box<dyn IServer> = impl_interface!(client => IServer);
```
### 服务端调用客户端（Push）
```rust
// 通过 token 获取客户端接口代理
let peer = impl_ref!(token => IClient);
peer.on_message("hello".to_string()).await;
```
### 常用类型别名（客户端）
```rust
// Arc<Actor<NetXClient<T>>>，泛型于 session store 类型
type NetxClientArc<T> = Arc<Actor<NetXClient<T>>>;
// Arc<Actor<NetXClient<DefaultSessionStore>>>，最常用形式
type NetxClientArcDef = Arc<Actor<NetXClient<DefaultSessionStore>>>;
```
---
## TLS 支持
### 使用 Rustls
```toml
# 服务端
netxserver = { version = "3", default-features = false, features = ["use_rustls"] }
# 客户端
netxclient = { version = "3", default-features = false, features = ["use_rustls"] }
```
```rust
// 服务端
let acceptor: &'static TlsAcceptor = /* 初始化 TlsAcceptor */;
let server = NetXServer::new_tls(acceptor, option, ImplCreateController).await;
// 客户端（RustlsAcceptAnyCertVerifier 已从 netxclient::prelude 重导出）
use netxclient::prelude::RustlsAcceptAnyCertVerifier;
let client = NetXClient::new_tls(server_info, session, domain, connector);
```
### 使用 OpenSSL
```toml
netxserver = { version = "3", default-features = false, features = ["use_openssl"] }
netxclient = { version = "3", default-features = false, features = ["use_openssl"] }
```
```rust
// 服务端
let server = NetXServer::new_ssl(ssl_acceptor, option, ImplCreateController).await;
// 客户端
let client = NetXClient::new_ssl(server_info, session, domain, connector);
```
测试证书生成脚本参见 [`ca_test/`](ca_test/)。
---
## 配置说明
### 服务端配置（`ServerOption`）
```json
{
  "addr": "0.0.0.0:8000",
  "service_name": "myservice",
  "verify_key": "secret_key",
  "request_out_time": 5000,
  "session_save_time": 5000
}
```
| 字段 | 类型 | 说明 |
|------|------|------|
| `addr` | `String` | 监听地址和端口 |
| `service_name` | `String` | 服务名称，客户端连接时需匹配（为空则不校验） |
| `verify_key` | `String` | 连接密钥，客户端连接时需匹配（为空则不校验） |
| `request_out_time` | `u32` | 请求超时时间（毫秒），默认 5000 |
| `session_save_time` | `u32` | Session 离线保留时间（毫秒），默认 5000 |
### 客户端配置（`ServerOption`）
```json
{
  "addr": "127.0.0.1:8000",
  "service_name": "myservice",
  "verify_key": "secret_key",
  "request_out_time_ms": 5000
}
```
| 字段 | 类型 | 说明 |
|------|------|------|
| `addr` | `String` | 服务端地址和端口 |
| `service_name` | `String` | 服务名称 |
| `verify_key` | `String` | 连接密钥 |
| `request_out_time_ms` | `u32` | 请求超时时间（毫秒） |
---
## 示例
### 运行聊天室示例
**启动服务端：**
```bash
cd examples/msg/msgserver
cargo run
```
**启动客户端：**
```bash
cd examples/msg/msgclient
cargo run
```
**客户端命令：**
```
--help                        显示帮助
--online                      显示所有在线用户
--to [nickname] [msg]         发送私信给指定用户
--ping [nickname] [count]     向指定用户发送 ping，显示往返时延
[message]                     向所有用户广播消息
```
### 运行 TLS 示例
```bash
# 先生成测试证书
cd ca_test && bash tls_gen.sh
# 启动 TLS 服务端
cd examples/tls/server && cargo run
# 启动 TLS 客户端
cd examples/tls/client && cargo run
```
---
## 相关项目
- [**rust_netx examples**](https://github.com/luyikk/rust_netx/tree/master/examples) — 更多完整示例
- [**file-store-server**](https://github.com/luyikk/file-store-server) — 基于 netx 的文件存储服务端
- [**file-store-client**](https://github.com/luyikk/file-store-client) — 基于 netx 的文件存储客户端
- [**NetX (.NET)**](https://github.com/luyikk/NetX) — .NET 版本实现
---
## License
本项目采用 [MIT](LICENSE) 或 [Apache-2.0](LICENSE) 双重许可证。