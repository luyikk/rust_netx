use anyhow::{anyhow, bail, Context, Result};
use aqueue::Actor;
use async_oneshot::{oneshot, Receiver, Sender};
use data_rw::{Data, DataOwnedReader};
use log::*;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tcpclient::{SocketClientTrait, TcpClient};
use tokio::io::{AsyncReadExt, ReadHalf};
use tokio::sync::watch::{channel, Receiver as WReceiver, Sender as WSender};
use tokio::time::{sleep, Duration};

use crate::client::controller::{FunctionInfo, IController};
use crate::client::request_manager::{IRequestManager, RequestManager};
use crate::client::result::RetResult;
use crate::client::NetxClientArc;

cfg_if::cfg_if! {
if #[cfg(feature = "tls")]{
   use openssl::ssl::{SslConnector,Ssl};
   use tokio::net::TcpStream;
   use tokio_openssl::SslStream;
   use std::pin::Pin;
   pub type NetPeer=Actor<TcpClient<SslStream<TcpStream>>>;
   pub type NetReadHalf=ReadHalf<SslStream<TcpStream>>;

   pub struct NetXClient<T>{
        ssl_domain:String,
        ssl_connector:SslConnector,
        session:T,
        serverinfo: ServerOption,
        net:Option<Arc<NetPeer>>,
        connect_stats:Option<WReceiver<(bool,String)>>,
        result_dict:HashMap<i64,Sender<Result<DataOwnedReader>>>,
        serial_atomic:AtomicI64,
        request_manager:OnceCell<Arc<Actor<RequestManager<T>>>>,
        controller_fun_register_dict:HashMap<i32,Box<dyn FunctionInfo>>,
        mode:u8
   }

}else if #[cfg(feature = "tcp")]{
   use tokio::net::TcpStream;
   pub type NetPeer=Actor<TcpClient<TcpStream>>;
   pub type NetReadHalf=ReadHalf<TcpStream>;

   pub struct NetXClient<T>{
        session:T,
        serverinfo: ServerOption,
        net:Option<Arc<NetPeer>>,
        connect_stats:Option<WReceiver<(bool,String)>>,
        result_dict:HashMap<i64,Sender<Result<DataOwnedReader>>>,
        serial_atomic:AtomicI64,
        request_manager:OnceCell<Arc<Actor<RequestManager<T>>>>,
        controller_fun_register_dict:HashMap<i32,Box<dyn FunctionInfo>>,
        mode:u8
   }
}}

pub trait SessionSave {
    fn get_sessionid(&self) -> i64;
    fn store_sessionid(&mut self, sessionid: i64);
}

enum SpecialFunctionTag {
    Connect = 2147483647,
    Disconnect = 2147483646,
    Closed = 2147483645,
}

unsafe impl<T> Send for NetXClient<T> {}
unsafe impl<T> Sync for NetXClient<T> {}

impl<T> Drop for NetXClient<T> {
    fn drop(&mut self) {
        debug!("{} is drop", self.serverinfo)
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct ServerOption {
    addr: String,
    service_name: String,
    verify_key: String,
    request_out_time_ms: u32,
}

impl std::fmt::Display for ServerOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}[{}]", self.service_name, self.addr)
    }
}

impl ServerOption {
    pub fn new(
        addr: String,
        service_name: String,
        verify_key: String,
        request_out_time_ms: u32,
    ) -> ServerOption {
        ServerOption {
            addr,
            service_name,
            verify_key,
            request_out_time_ms,
        }
    }
}

impl<T: SessionSave + 'static> NetXClient<T> {
    cfg_if::cfg_if! {
        if #[cfg(feature = "tls")] {

        pub fn new(serverinfo: ServerOption, session:T,ssl_domain:String,ssl_connector:SslConnector) ->NetxClientArc<T>{
            let request_out_time_ms=serverinfo.request_out_time_ms;
            let netx_client=Arc::new(Actor::new(NetXClient{
                ssl_domain,
                ssl_connector,
                session,
                serverinfo,
                net:None,
                result_dict:HashMap::new(),
                connect_stats:None,
                serial_atomic:AtomicI64::new(1),
                request_manager:OnceCell::new(),
                controller_fun_register_dict:HashMap::new(),
                mode:0
            }));

            let request_manager=RequestManager::new(request_out_time_ms,Arc::downgrade(&netx_client));
            unsafe {
                if netx_client.deref_inner().request_manager.set(request_manager).is_err(){
                    error!("not set request_manager,request_manager may not be none")
                }
            }
            netx_client
        }} else if #[cfg(feature = "tcp")]{

        pub fn new(serverinfo: ServerOption, session:T) ->NetxClientArc<T>{
            let request_out_time_ms=serverinfo.request_out_time_ms;
            let netx_client=Arc::new(Actor::new(NetXClient{
                session,
                serverinfo,
                net:None,
                result_dict:HashMap::new(),
                connect_stats:None,
                serial_atomic:AtomicI64::new(1),
                request_manager:OnceCell::new(),
                controller_fun_register_dict:HashMap::new(),
                mode:0
            }));

            let request_manager=RequestManager::new(request_out_time_ms,Arc::downgrade(&netx_client));
            unsafe {
                if netx_client.deref_inner().request_manager.set(request_manager).is_err(){
                    error!("not set request_manager,request_manager may not be none")
                }
            }
            netx_client
        }}
    }

    #[inline]
    pub fn init<C: IController + Sync + Send + 'static>(&mut self, controller: C) {
        self.controller_fun_register_dict =
            IController::register(Arc::new(controller)).expect("init error");
    }

    #[allow(clippy::type_complexity)]
    async fn input_buffer(
        (netx_client, set_connect): (NetxClientArc<T>, WSender<(bool, String)>),
        client: Arc<NetPeer>,
        mut reader: NetReadHalf,
    ) -> Result<bool> {
        if let Err(er) = Self::read_buffer(&netx_client, set_connect, client, &mut reader).await {
            error!("read buffer err:{}", er);
        }
        netx_client.clean_connect().await?;
        info! {"disconnect to {}", netx_client.get_serviceinfo()};
        netx_client
            .call_special_function(SpecialFunctionTag::Disconnect as i32)
            .await?;
        Ok(true)
    }

    async fn read_buffer(
        netx_client: &NetxClientArc<T>,
        set_connect: WSender<(bool, String)>,
        client: Arc<NetPeer>,
        reader: &mut NetReadHalf,
    ) -> Result<()> {
        let serverinfo = netx_client.get_serviceinfo();
        let mut sessionid = netx_client.get_sessionid();
        client
            .send(
                Self::get_verify_buff(&serverinfo.service_name, &serverinfo.verify_key, &sessionid)
                    .into_inner(),
            )
            .await?;
        let mut option_connect = Some(set_connect);
        while let Ok(len) = reader.read_u32_le().await {
            let len = (len - 4) as usize;
            let mut buff = vec![0; len];
            reader.read_exact(&mut buff).await?;
            let mut dr = DataOwnedReader::new(buff);
            let cmd = dr.read_fixed::<i32>()?;
            match cmd {
                1000 => match dr.read_fixed::<bool>()? {
                    false => {
                        info!("{} {}", serverinfo, dr.read_fixed_str()?);
                        if (dr.len() - dr.get_offset()) == 1 && dr.read_fixed::<u8>()? == 1 {
                            debug!("mode 1");
                            netx_client.set_mode(1).await?;
                        }
                        client
                            .send(Self::get_sessionid_buff(netx_client.get_mode()).into_inner())
                            .await?;
                        netx_client
                            .call_special_function(SpecialFunctionTag::Connect as i32)
                            .await?;
                        if let Some(set_connect) = option_connect.take() {
                            if set_connect.send((true, "success".into())).is_err() {
                                error!("talk connect rx is close");
                            }
                            drop(set_connect);
                        }
                    }
                    true => {
                        let err = dr.read_fixed_str()?;
                        error!("connect {} error:{}", serverinfo, err);
                        if let Some(set_connect) = option_connect.take() {
                            if set_connect.send((false, err.to_string())).is_err() {
                                error!("talk connect rx is close");
                            }
                            drop(set_connect);
                        }
                        break;
                    }
                },
                2000 => {
                    sessionid = dr.read_fixed::<i64>()?;
                    info!("{} save sessionid:{}", serverinfo, sessionid);
                    netx_client.store_sessionid(sessionid).await?;
                }
                2400 => {
                    let tt = dr.read_fixed::<u8>()?;
                    let cmd = dr.read_fixed::<i32>()?;
                    let sessionid = dr.read_fixed::<i64>()?;
                    match tt {
                        0 => {
                            let run_netx_client = netx_client.clone();
                            tokio::spawn(async move {
                                let _ = run_netx_client.call_controller(tt, cmd, dr).await;
                            });
                        }
                        1 => {
                            let run_netx_client = netx_client.clone();
                            let send_client = client.clone();
                            tokio::spawn(async move {
                                let res = run_netx_client.call_controller(tt, cmd, dr).await;
                                if let Err(er) = send_client
                                    .send(
                                        Self::get_result_buff(
                                            sessionid,
                                            res,
                                            run_netx_client.get_mode(),
                                        )
                                        .into_inner(),
                                    )
                                    .await
                                {
                                    error!("send buff 1 error:{}", er);
                                }
                            });
                        }
                        2 => {
                            let run_netx_client = netx_client.clone();
                            let send_client = client.clone();
                            tokio::spawn(async move {
                                let res = run_netx_client.call_controller(tt, cmd, dr).await;
                                if let Err(er) = send_client
                                    .send(
                                        Self::get_result_buff(
                                            sessionid,
                                            res,
                                            run_netx_client.get_mode(),
                                        )
                                        .into_inner(),
                                    )
                                    .await
                                {
                                    error!("send buff 2 error:{}", er);
                                }
                            });
                        }
                        _ => {
                            panic!("not found call type:{}", tt);
                        }
                    }
                }
                2500 => {
                    let serial = dr.read_fixed::<i64>()?;
                    netx_client.set_result(serial, dr).await?;
                }
                _ => {
                    error!("{} Unknown command:{}->{:?}", serverinfo, cmd, dr);
                    break;
                }
            }
        }

        Ok(())
    }

    #[inline]
    pub(crate) async fn call_special_function(&self, cmd: i32) -> Result<()> {
        if let Some(func) = self.controller_fun_register_dict.get(&cmd) {
            func.call(DataOwnedReader::new(vec![0; 4])).await?;
        }
        Ok(())
    }

    #[inline]
    pub(crate) async fn run_controller(
        &self,
        tt: u8,
        cmd: i32,
        dr: DataOwnedReader,
    ) -> Result<RetResult> {
        return if let Some(func) = self.controller_fun_register_dict.get(&cmd) {
            if func.function_type() != tt {
                bail!(" cmd:{} function type error:{}", cmd, tt)
            } else {
                func.call(dr).await
            }
        } else {
            bail!("not found cmd:{}", cmd)
        };
    }

    #[inline]
    fn get_verify_buff(service_name: &str, verify_key: &str, sessionid: &i64) -> Data {
        let mut data = Data::with_capacity(128);
        data.write_fixed(1000);
        data.write_fixed(service_name);
        data.write_fixed(verify_key);
        data.write_fixed(sessionid);
        data
    }

    fn get_sessionid_buff(mode: u8) -> Data {
        let mut buff = Data::with_capacity(32);
        buff.write_fixed(2000);
        if mode == 0 {
            buff
        } else {
            let len = buff.len() + 4;
            let mut data = Data::with_capacity(len);
            data.write_fixed(len as u32);
            data.write_buf(&buff);
            data
        }
    }

    #[inline]
    fn get_result_buff(sessionid: i64, result: RetResult, mode: u8) -> Data {
        let mut data = Data::with_capacity(1024);
        data.write_fixed(2500u32);
        data.write_fixed(sessionid);
        if result.is_error {
            data.write_fixed(true);
            data.write_fixed(result.error_id);
            data.write_fixed(result.msg);
        } else {
            data.write_fixed(false);
            data.write_fixed(result.arguments.len() as u32);
            for argument in result.arguments {
                data.write_fixed(argument.into_inner());
            }
        }

        if mode == 0 {
            data
        } else {
            let len = data.len() + 4usize;
            let mut buff = Data::with_capacity(len);
            buff.write_fixed(len as u32);
            buff.write_buf(&data);
            buff
        }
    }

    #[inline]
    pub fn set_mode(&mut self, mode: u8) {
        self.mode = mode
    }

    #[inline]
    pub fn get_mode(&self) -> u8 {
        self.mode
    }

    #[inline]
    pub fn get_addr_string(&self) -> String {
        self.serverinfo.addr.clone()
    }

    #[inline]
    pub fn get_service_info(&self) -> ServerOption {
        self.serverinfo.clone()
    }

    #[cfg(feature = "tls")]
    #[inline]
    pub fn get_ssl(&self) -> Result<Ssl> {
        Ok(self.ssl_connector.configure()?.into_ssl(&self.ssl_domain)?)
    }

    #[inline]
    pub fn get_sessionid(&self) -> i64 {
        self.session.get_sessionid()
    }

    #[inline]
    pub fn store_sessionid(&mut self, sessionid: i64) {
        self.session.store_sessionid(sessionid)
    }

    #[inline]
    pub fn set_network_client(&mut self, client: Arc<NetPeer>) {
        self.net = Some(client);
    }

    #[inline]
    pub fn set_connect_stats(&mut self, stats: Option<WReceiver<(bool, String)>>) {
        self.connect_stats = stats;
    }

    #[inline]
    pub fn is_connect(&self) -> bool {
        self.net.is_some()
    }

    #[inline]
    pub fn new_serial(&self) -> i64 {
        self.serial_atomic.fetch_add(1, Ordering::Acquire)
    }

    #[inline]
    pub fn get_callback_len(&mut self) -> usize {
        self.result_dict.len()
    }

    #[inline]
    pub(crate) async fn set_request_sessionid(&self, sessionid: i64) -> Result<()> {
        if let Some(request) = self.request_manager.get() {
            return request.set(sessionid).await;
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
#[async_trait::async_trait]
pub trait INetXClient<T> {
    async fn init<C: IController + Sync + Send + 'static>(&self, controller: C) -> Result<()>;
    async fn connect_network(self: &Arc<Self>) -> Result<()>;
    #[cfg(feature = "tls")]
    fn get_ssl(&self) -> Result<Ssl>;
    fn get_address(&self) -> String;
    fn get_serviceinfo(&self) -> ServerOption;
    fn get_sessionid(&self) -> i64;
    fn get_mode(&self) -> u8;
    fn new_serial(&self) -> i64;
    fn is_connect(&self) -> bool;
    async fn get_peer(&self) -> Result<Option<Arc<NetPeer>>>;
    async fn store_sessionid(&self, sessionid: i64) -> Result<()>;
    async fn set_mode(&self, mode: u8) -> Result<()>;
    async fn set_network_client(&self, client: Arc<NetPeer>) -> Result<()>;
    async fn reset_connect_stats(&self) -> Result<()>;
    async fn get_callback_len(&self) -> Result<usize>;
    async fn set_result(&self, serial: i64, data: DataOwnedReader) -> Result<()>;
    async fn set_error(&self, serial: i64, err: anyhow::Error) -> Result<()>;
    async fn call_special_function(&self, cmd: i32) -> Result<()>;
    async fn call_controller(&self, tt: u8, cmd: i32, data: DataOwnedReader) -> RetResult;
    async fn clean_connect(&self) -> Result<()>;
    async fn close(&self) -> Result<()>;
    async fn call(&self, serial: i64, buff: Data) -> Result<RetResult>;
    async fn run(&self, buff: Data) -> Result<()>;
}

#[allow(clippy::too_many_arguments)]
#[async_trait::async_trait]
impl<T: SessionSave + 'static> INetXClient<T> for Actor<NetXClient<T>> {
    #[inline]
    async fn init<C: IController + Sync + Send + 'static>(&self, controller: C) -> Result<()> {
        self.inner_call(|inner| async move {
            inner.get_mut().init(controller);
            Ok(())
        })
        .await
    }

    #[inline]
    async fn connect_network(self: &Arc<Self>) -> Result<()> {
        let netx_client = self.clone();
        let mut wait_handler: WReceiver<(bool, String)> = self.inner_call(|inner|async move  {
            if inner.get().is_connect() {
                return match inner.get().connect_stats {
                    Some(ref stats) => {
                        Ok(stats.clone())
                    },
                    None => {
                        bail!("inner is connect,but not get stats")
                    }
                }
            }

            let (set_connect, wait_connect) = channel((false, "not connect".to_string()));

            let client={
            cfg_if::cfg_if! {
            if#[cfg(feature = "tls")]{
                let ssl=netx_client.get_ssl()?;
                TcpClient::connect_stream_type(netx_client.get_address(),|tcp_stream| async move{
                     let mut stream = SslStream::new(ssl, tcp_stream)?;
                     Pin::new(&mut stream).connect().await?;
                     Ok(stream)
                },NetXClient::input_buffer, (netx_client, set_connect)).await?
            }else if #[cfg(feature = "tcp")]{
                TcpClient::connect(netx_client.get_address(), NetXClient::input_buffer, (netx_client, set_connect)).await?
            }}};

            let ref_inner = inner.get_mut();
            ref_inner.set_network_client(client);
            ref_inner.connect_stats = Some(wait_connect.clone());
            Ok(wait_connect)

        }).await?;

        match wait_handler.changed().await {
            Err(err) => {
                self.reset_connect_stats().await?;
                bail!("connect err:{}", err)
            }
            Ok(_) => {
                self.reset_connect_stats().await?;
                let (is_connect, msg) = &(*wait_handler.borrow());
                if !is_connect {
                    bail!("connect err:{}", msg);
                }
            }
        }

        Ok(())
    }

    #[cfg(feature = "tls")]
    #[inline]
    fn get_ssl(&self) -> Result<Ssl> {
        unsafe { self.deref_inner().get_ssl() }
    }

    #[inline]
    fn get_address(&self) -> String {
        unsafe { self.deref_inner().get_addr_string() }
    }

    #[inline]
    fn get_serviceinfo(&self) -> ServerOption {
        unsafe { self.deref_inner().get_service_info() }
    }

    #[inline]
    fn get_sessionid(&self) -> i64 {
        unsafe { self.deref_inner().get_sessionid() }
    }

    #[inline]
    fn get_mode(&self) -> u8 {
        unsafe { self.deref_inner().get_mode() }
    }

    #[inline]
    fn new_serial(&self) -> i64 {
        unsafe { self.deref_inner().new_serial() }
    }

    #[inline]
    fn is_connect(&self) -> bool {
        unsafe { self.deref_inner().is_connect() }
    }

    #[inline]
    async fn get_peer(&self) -> Result<Option<Arc<NetPeer>>> {
        self.inner_call(|inner| async move { Ok(inner.get().net.clone()) })
            .await
    }

    #[inline]
    async fn store_sessionid(&self, sessionid: i64) -> Result<()> {
        self.inner_call(|inner| async move {
            inner.get_mut().store_sessionid(sessionid);
            Ok(())
        })
        .await
    }

    #[inline]
    async fn set_mode(&self, mode: u8) -> Result<()> {
        self.inner_call(|inner| async move {
            inner.get_mut().set_mode(mode);
            Ok(())
        })
        .await
    }

    #[inline]
    async fn set_network_client(&self, client: Arc<NetPeer>) -> Result<()> {
        self.inner_call(|inner| async move {
            inner.get_mut().set_network_client(client);
            Ok(())
        })
        .await
    }

    #[inline]
    async fn reset_connect_stats(&self) -> Result<()> {
        self.inner_call(|inner| async move {
            inner.get_mut().set_connect_stats(None);
            Ok(())
        })
        .await
    }

    #[inline]
    async fn get_callback_len(&self) -> Result<usize> {
        self.inner_call(|inner| async move { Ok(inner.get_mut().get_callback_len()) })
            .await
    }

    #[inline]
    async fn set_result(&self, serial: i64, data: DataOwnedReader) -> Result<()> {
        let have_tx: Option<Sender<Result<DataOwnedReader>>> = self
            .inner_call(|inner| async move { Ok(inner.get_mut().result_dict.remove(&serial)) })
            .await?;

        if let Some(mut tx) = have_tx {
            tx.send(Ok(data)).map_err(|_| anyhow!("rx is close"))?;
        } else {
            match RetResult::from(data) {
                Ok(res) => match res.check() {
                    Ok(_) => error!("not found 2 {}", serial),
                    Err(err) => error!("{}", err),
                },
                Err(er) => error!("not found {} :{}", serial, er),
            }
        }
        Ok(())
    }
    #[inline]
    async fn set_error(&self, serial: i64, err: anyhow::Error) -> Result<()> {
        let have_tx: Option<Sender<Result<DataOwnedReader>>> = self
            .inner_call(|inner| async move { Ok(inner.get_mut().result_dict.remove(&serial)) })
            .await?;
        if let Some(mut tx) = have_tx {
            tx.send(Err(err)).map_err(|_| anyhow!("rx is close"))?;
            Ok(())
        } else {
            Ok(())
        }
    }

    #[inline]
    async fn call_special_function(&self, cmd: i32) -> Result<()> {
        unsafe { self.deref_inner().call_special_function(cmd).await }
    }

    #[inline]
    async fn call_controller(&self, tt: u8, cmd: i32, dr: DataOwnedReader) -> RetResult {
        unsafe {
            match self.deref_inner().run_controller(tt, cmd, dr).await {
                Ok(res) => res,
                Err(err) => {
                    error!("call controller error:{}", err);
                    RetResult::error(1, format!("call controller err:{}", err))
                }
            }
        }
    }

    #[inline]
    async fn clean_connect(&self) -> Result<()> {
        let net: Result<Arc<NetPeer>> = self
            .inner_call(|inner| async move { inner.get_mut().net.take().context("not connect") })
            .await;
        match net {
            Err(_) => Ok(()),
            Ok(net) => {
                net.disconnect().await?;
                sleep(Duration::from_millis(100)).await;
                Ok(())
            }
        }
    }

    #[inline]
    async fn close(&self) -> Result<()> {
        let net: Result<Arc<NetPeer>> = self
            .inner_call(|inner| async move {
                if let Err(er) = inner
                    .get()
                    .call_special_function(SpecialFunctionTag::Closed as i32)
                    .await
                {
                    error!("call controller Closed err:{}", er)
                }
                inner.get_mut().controller_fun_register_dict.clear();
                inner.get_mut().net.take().context("not connect")
            })
            .await;
        match net {
            Err(_) => Ok(()),
            Ok(net) => {
                net.disconnect().await?;
                sleep(Duration::from_millis(100)).await;
                Ok(())
            }
        }
    }

    #[inline]
    async fn call(&self, serial: i64, buff: Data) -> Result<RetResult> {
        let (net, rx): (Arc<NetPeer>, Receiver<Result<DataOwnedReader>>) = self
            .inner_call(|inner| async move {
                if let Some(ref net) = inner.get().net {
                    let (tx, rx): (
                        Sender<Result<DataOwnedReader>>,
                        Receiver<Result<DataOwnedReader>>,
                    ) = oneshot();
                    if inner.get_mut().result_dict.contains_key(&serial) {
                        bail!("serial is have")
                    }
                    inner.get_mut().result_dict.insert(serial, tx);
                    Ok((net.clone(), rx))
                } else {
                    bail!("not connect")
                }
            })
            .await?;
        unsafe {
            self.deref_inner().set_request_sessionid(serial).await?;
        }
        if self.get_mode() == 0 {
            net.send(buff.into_inner()).await?;
        } else {
            let len = buff.len() + 4;
            let mut data = Data::with_capacity(len);
            data.write_fixed(len as u32);
            data.write_buf(&buff);
            net.send(data.into_inner()).await?;
        }
        match rx.await {
            Err(_) => {
                bail!("tx is Close")
            }
            Ok(data) => Ok(RetResult::from(data?)?),
        }
    }

    #[inline]
    async fn run(&self, buff: Data) -> Result<()> {
        let net = self
            .inner_call(|inner| async move {
                if let Some(ref net) = inner.get().net {
                    Ok(net.clone())
                } else {
                    bail!("not connect")
                }
            })
            .await?;
        if self.get_mode() == 0 {
            net.send(buff.into_inner()).await?;
        } else {
            let len = buff.len() + 4;
            let mut data = Data::with_capacity(len);
            data.write_fixed(len as u32);
            data.write_buf(&buff);
            net.send(data.into_inner()).await?;
        }

        Ok(())
    }
}

#[macro_export]
macro_rules! call {
    (@uint $($x:tt)*)=>(());
    (@count $($rest:expr),*)=>(<[()]>::len(&[$(call!(@uint $rest)),*]));
    ($client:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            if $client.is_connect() ==false{
                $client.connect_network().await?;
            }
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call!(@count $($args),*) as i32;
            let serial=$client.new_serial();
            data.write_fixed(2400u32);
            data.write_fixed(2u8);
            data.write_fixed($cmd);
            data.write_fixed(serial);
            data.write_fixed(args_count);
            $(data.pack_serialize($args)?;)*
            let mut ret= $client.call(serial,data).await?.check()?;
            ret.deserialize()?
    });
    (@result $client:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            if $client.is_connect() ==false{
               $client.connect_network().await?;
            }
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call!(@count $($args),*) as i32;
            let serial=$client.new_serial();
            data.write_fixed(2400u32);
            data.write_fixed(2u8);
            data.write_fixed($cmd);
            data.write_fixed(serial);
            data.write_fixed(args_count);
            $(data.pack_serialize($args)?;)*
            $client.call(serial,data).await?
    });
    (@run $client:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            if $client.is_connect() ==false{
                $client.connect_network().await?;
            }
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call!(@count $($args),*) as i32;
            let serial=$client.new_serial();
            data.write_fixed(2400u32);
            data.write_fixed(0u8);
            data.write_fixed($cmd);
            data.write_fixed(serial);
            data.write_fixed(args_count);
            $(data.pack_serialize($args)?;)*
            $client.run(data).await?;
    });
     (@run_not_err $client:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            if $client.is_connect() ==false{
               if let Err(err)= $client.connect_network().await{
                    log::error!{"run connect {} is error:{}",$cmd,err}
               }
            }
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call!(@count $($args),*) as i32;
            let serial=$client.new_serial();
            data.write_fixed(2400u32);
            data.write_fixed(0u8);
            data.write_fixed($cmd);
            data.write_fixed(serial);
            data.write_fixed(args_count);
            $(
              if let Err(err)=  data.pack_serialize($args){
                 log::error!{"pack_serialize {} is error:{}",$cmd,err};
              }
            )*
            if let Err(err)= $client.run(data).await{
                 log::warn!{"run {} is error:{}",$cmd,err}
            }
    });
    (@checkrun $client:expr=>$cmd:expr;$($args:expr), *$(,)*) => ({
            if $client.is_connect() ==false{
                $client.connect_network().await?;
            }
            use data_rw::Data;
            let mut data=Data::with_capacity(128);
            let args_count=call!(@count $($args),*) as i32;
            let serial=$client.new_serial();
            data.write_fixed(2400u32);
            data.write_fixed(1u8);
            data.write_fixed($cmd);
            data.write_fixed(serial);
            data.write_fixed(args_count);
            $(data.pack_serialize($args)?;)*
            $client.call(serial,data).await?.check()?;

    });

}

#[macro_export]
macro_rules! impl_interface {
    ($client:expr=>$interface:ty) => {
        paste::paste! {
              Box::new([<___impl_ $interface _call>]::new($client.clone()))  as  Box<dyn $interface>
        }
    };
}

#[macro_export]
macro_rules! impl_owned_interface {
    ($client:expr=>$interface:ty) => {
        paste::paste! {
              Box::new([<___impl_ $interface _call>]::new($client))  as  Box<dyn $interface>
        }
    };
}
