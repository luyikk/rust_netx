use tokio::net::tcp::OwnedReadHalf;
use std::io;
use tokio::io::AsyncReadExt;
use data_rw::Data;
use std::error::Error;
use serde::{Deserialize, Deserializer};
use std::ops::{Deref, DerefMut};
use serde::de::Visitor;
use log::*;


#[aqueue::aqueue_trait]
pub trait OwnedReadHalfExt{
    async fn read_string(&mut self)->Result<String,Box<dyn Error>>;
    async fn read_buff(&mut self)->Result<Data,Box<dyn Error>>;
}

#[aqueue::aqueue_trait]
impl OwnedReadHalfExt for &mut OwnedReadHalf{
    #[inline]
    async fn read_string(&mut self)->Result<String,Box<dyn Error>>{
        let len= self.read_u32_le().await? as usize;
        let mut data=vec![0;len];
        let r=self.read_exact(&mut data).await?;
        debug_assert_eq!(len, r);
        Ok(String::from_utf8_lossy(&data).to_string())
    }
    #[inline]
    async fn read_buff(&mut self) -> Result<Data, Box<dyn Error>> {
        let len=( self.read_u32_le().await? -4) as usize;
        let mut data=Data::with_len(len,0);
        let r=self.read_exact(&mut data).await?;
        debug_assert_eq!(len, r);
        Ok(data)
    }
}