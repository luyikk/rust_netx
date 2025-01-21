use data_rw::{Data, DataOwnedReader};
use serde::{Deserialize, Serialize};
use std::io;
use std::ops::{Index, IndexMut};
use tokio::io::ErrorKind;

/// A structure representing the result of an operation.
#[derive(Debug)]
pub struct RetResult {
    /// Indicates if the result is an error.
    pub is_error: bool,
    /// The error ID if the result is an error.
    pub error_id: i32,
    /// The message associated with the result.
    pub msg: String,
    /// The arguments associated with the result.
    pub arguments: Vec<DataOwnedReader>,
}

impl RetResult {
    /// Creates a new `RetResult`.
    ///
    /// # Arguments
    ///
    /// * `is_error` - A boolean indicating if the result is an error.
    /// * `error_id` - The error ID.
    /// * `msg` - The message associated with the result.
    /// * `args` - The arguments associated with the result.
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

    /// Creates a `RetResult` representing a successful operation.
    #[inline]
    pub fn success() -> RetResult {
        RetResult {
            is_error: false,
            error_id: 0,
            msg: "Success".to_string(),
            arguments: Vec::new(),
        }
    }

    /// Creates a `RetResult` representing an error.
    ///
    /// # Arguments
    ///
    /// * `error_id` - The error ID.
    /// * `msg` - The error message.
    #[inline]
    pub fn error(error_id: i32, msg: String) -> RetResult {
        RetResult {
            is_error: true,
            error_id,
            msg,
            arguments: Vec::new(),
        }
    }

    /// Adds a serialized argument to the result.
    ///
    /// # Arguments
    ///
    /// * `p` - The argument to be serialized and added.
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

    /// Creates a `RetResult` from a `DataOwnedReader`.
    ///
    /// # Arguments
    ///
    /// * `dr` - The `DataOwnedReader` to create the result from.
    #[inline]
    pub(crate) fn from(mut dr: DataOwnedReader) -> crate::error::Result<RetResult> {
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

    /// Returns the number of arguments in the result.
    #[inline]
    pub fn len(&self) -> usize {
        self.arguments.len()
    }

    /// Checks if the result has no arguments.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.arguments.is_empty()
    }

    /// Checks the result and returns an error if it is an error result.
    #[inline]
    pub fn check(self) -> crate::error::Result<RetResult> {
        if self.is_error {
            Err(crate::error::Error::CallError(self.error_id, self.msg))
        } else {
            Ok(self)
        }
    }

    /// Gets a mutable reference to an argument by index.
    ///
    /// # Arguments
    ///
    /// * `index` - The index of the argument to get.
    #[inline]
    pub fn get(&mut self, index: usize) -> io::Result<&mut DataOwnedReader> {
        if index >= self.len() {
            return Err(io::Error::new(ErrorKind::Other, "index >= len"));
        }
        Ok(&mut self.arguments[index])
    }

    /// Deserializes the first argument in the result.
    ///
    /// # Arguments
    ///
    /// * `T` - The type to deserialize the argument into.
    #[inline]
    pub fn deserialize<'a, T: Deserialize<'a> + 'static>(&'a mut self) -> crate::error::Result<T> {
        if self.is_empty() {
            return Err(io::Error::new(ErrorKind::Other, "index >= len").into());
        }
        Ok(self.arguments[0].pack_to()?)
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
