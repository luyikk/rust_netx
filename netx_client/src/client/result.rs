use anyhow::{bail, Result};
use data_rw::{Data, DataOwnedReader};
use serde::{Deserialize, Serialize};
use std::io;
use std::ops::{Index, IndexMut};
use tokio::io::ErrorKind;

#[derive(Debug)]
pub struct RetResult {
    pub is_error: bool,
    pub error_id: i32,
    pub msg: String,
    pub arguments: Vec<DataOwnedReader>,
}

impl RetResult {
    #[inline]
    pub fn new(
        is_error: bool,
        error_id: i32,
        msg: String,
        args: Vec<DataOwnedReader>,
    ) -> RetResult {
        RetResult {
            is_error,
            error_id,
            msg,
            arguments: args,
        }
    }
    #[inline]
    pub fn success() -> RetResult {
        RetResult {
            is_error: false,
            error_id: 0,
            msg: "Success".to_string(),
            arguments: Vec::new(),
        }
    }

    #[inline]
    pub fn error(error_id: i32, msg: String) -> RetResult {
        RetResult {
            is_error: true,
            error_id,
            msg,
            arguments: Vec::new(),
        }
    }

    #[inline]
    pub fn add_arg_buff<T: Serialize>(&mut self, p: T) {
        match Data::pack_from(p) {
            Ok(data) => {
                self.arguments.push(DataOwnedReader::new(data.into()));
            }
            Err(er) => {
                log::error!("Data serialize error：{} {}", er, line!())
            }
        }
    }
    #[inline]
    pub(crate) fn from(mut dr: DataOwnedReader) -> Result<RetResult> {
        if dr.read_fixed::<bool>()? {
            Ok(RetResult::new(
                true,
                dr.read_fixed::<i32>()?,
                dr.read_fixed_str()?.to_string(),
                Vec::new(),
            ))
        } else {
            let len = dr.read_fixed::<i32>()?;
            let mut buffs = Vec::with_capacity(len as usize);
            for _ in 0..len {
                buffs.push(DataOwnedReader::new(dr.read_fixed_buf()?.to_vec()));
            }
            Ok(RetResult::new(false, 0, "success".into(), buffs))
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.arguments.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.arguments.is_empty()
    }

    #[inline]
    pub fn check(self) -> Result<RetResult> {
        if self.is_error {
            bail!("{}:{}", self.error_id, self.msg)
        } else {
            Ok(self)
        }
    }

    #[inline]
    pub fn get(&mut self, index: usize) -> io::Result<&mut DataOwnedReader> {
        if index >= self.len() {
            return Err(io::Error::new(ErrorKind::Other, "index >= len"));
        }
        Ok(&mut self.arguments[index])
    }

    #[inline]
    pub fn deserialize<'a, T: Deserialize<'a> + 'static>(&'a mut self) -> Result<T> {
        if self.is_empty() {
            return Err(io::Error::new(ErrorKind::Other, "index >= len").into());
        }
        self.arguments[0].pack_to()
    }
}

impl Index<usize> for RetResult {
    type Output = DataOwnedReader;
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
