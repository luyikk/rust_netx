use tokio::net::tcp::OwnedReadHalf;
use tokio::io::AsyncReadExt;
use data_rw::Data;
use std::io;


#[aqueue::aqueue_trait]
pub trait OwnedReadHalfExt{
    async fn read_string(&mut self)->io::Result<String>;
    async fn read_buff(&mut self)->io::Result<Data>;
    async fn read_buff_by(&mut self,data:&mut Data)->io::Result<usize>;
}

#[aqueue::aqueue_trait]
impl OwnedReadHalfExt for &mut OwnedReadHalf{
    #[inline]
    async fn read_string(&mut self)->io::Result<String>{
        let len= self.read_u32_le().await? as usize;
        let mut data=vec![0;len];
        let r=self.read_exact(&mut data).await?;
        debug_assert_eq!(len, r);
        Ok(String::from_utf8_lossy(&data).to_string())
    }
    #[inline]
    async fn read_buff(&mut self) ->io::Result<Data> {
        let len=( self.read_u32_le().await? -4) as usize;
        let mut data=Data::with_len(len,0);
        let r=self.read_exact(&mut data).await?;
        debug_assert_eq!(len, r);
        Ok(data)
    }

    #[inline]
    async fn read_buff_by(&mut self,data:&mut Data)->io::Result<usize>{
        let len=( self.read_u32_le().await? -4) as usize;
        data.resize(len,0);
        data.set_position(0);
        let r=self.read_exact(data).await?;
        Ok(r)
    }
}