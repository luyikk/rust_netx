use data_rw::Data;
use std::io;
use aqueue::{AResult, AError};
use std::io::ErrorKind;
use serde::{Deserialize, Serialize};
use std::ops::{Index, IndexMut};
use std::error::Error;

#[derive(Debug)]
pub struct RetResult {
    pub is_error:bool,
    pub error_id:i32,
    pub msg:String,
    pub arguments:Vec<Data>
}


impl RetResult {
    #[inline]
    pub fn new(is_error:bool,error_id:i32, msg:String,args:Vec<Data>)-> RetResult {
        RetResult {
            is_error,
            error_id,
            msg,
            arguments:args
        }
    }
    #[inline]
    pub fn success()->RetResult{
        RetResult{
            is_error:false,
            error_id:0,
            msg:"Success".to_string(),
            arguments:Vec::new()
        }
    }

    #[inline]
    pub fn error(error_id:i32,msg:String)->RetResult{
        RetResult{
            is_error:true,
            error_id,
            msg,
            arguments:Vec::new()
        }
    }

    #[inline]
    pub fn add_arg_buff<T:Serialize>(&mut self,p:T){
        let mut data= Data::new();
        if let Err(er)=data.serde_serialize(p){
            log::error!("Data serialize error：{} {}",er,line!())
        }
        self.arguments.push(data);
    }

    #[inline]
    pub fn from(mut data:Data)->io::Result<RetResult>{
        if data.get_le::<bool>()?{
            let error_id=data.get_le::<i32>()?;
            let error_msg=data.get_le::<String>()?;
            Ok(RetResult::new(true,error_id,error_msg,Vec::new()))
        }
        else{
            let len=data.get_le::<i32>()?;
            let mut buffs=Vec::with_capacity(len as usize);
            for _ in 0..len {
                buffs.push(Data::from(data.get_le::<Vec<u8>>()?));
            }
            Ok(RetResult::new(false,0,"success".into(),buffs))
        }
    }

    #[inline]
    pub fn len(&self)->usize{
        self.arguments.len()
    }

    #[inline]
    pub fn is_empty(&self)->bool{
        self.arguments.is_empty()
    }

    #[inline]
    pub fn check(self)->AResult<RetResult>{
        if self.is_error{
            Err(AError::StrErr(format!("{}:{}",self.error_id,self.msg)))
        }else {
            Ok(self)
        }
    }

    #[inline]
    pub fn get(&mut self, index:usize) ->io::Result<&mut Data>{
        if index>=self.len(){
            return Err(io::Error::new(ErrorKind::Other,"index >= len"))
        }
        Ok(& mut self.arguments[index])
    }

    #[inline]
    pub fn deserialize<'a,T:Deserialize<'a>>(&'a mut self)->Result<T,Box<dyn Error>>{
        if self.is_empty(){
            return Err(io::Error::new(ErrorKind::Other,"index >= len").into())
        }
        let r= T::deserialize(&mut self.arguments[0])?;
        Ok(r)
    }

}

impl Index<usize> for RetResult {
    type Output = Data;
    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        &self.arguments[index]
    }
}

impl IndexMut<usize> for RetResult {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.arguments[index]
    }
}
