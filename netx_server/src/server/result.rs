use data_rw::{Data, DataOwnedReader};
use serde::{Deserialize, Serialize};
use std::io;
use std::io::ErrorKind;
use std::ops::{Index, IndexMut};

/// A struct representing the result of an operation.
///
/// # Fields
/// - `is_error`: A boolean indicating if the result is an error.
/// - `error_id`: An integer representing the error ID.
/// - `msg`: A string containing the message associated with the result.
/// - `arguments`: A vector of `DataOwnedReader` containing additional arguments.
#[derive(Debug)]
pub struct RetResult {
    pub is_error: bool,
    pub error_id: i32,
    pub msg: String,
    pub arguments: Vec<DataOwnedReader>,
}

impl RetResult {
    /// Creates a new `RetResult`.
    ///
    /// # Arguments
    ///
    /// * `is_error` - A boolean indicating if the result is an error.
    /// * `error_id` - An integer representing the error ID.
    /// * `msg` - A string containing the message associated with the result.
    /// * `args` - A vector of `DataOwnedReader` containing additional arguments.
    ///
    /// # Returns
    ///
    /// A new `RetResult` instance.
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

    /// Creates a new `RetResult` representing a successful operation.
    ///
    /// # Returns
    ///
    /// A new `RetResult` instance with `is_error` set to `false`.
    #[inline]
    pub fn success() -> RetResult {
        RetResult {
            is_error: false,
            error_id: 0,
            msg: "Success".to_string(),
            arguments: Vec::new(),
        }
    }

    /// Creates a new `RetResult` representing an error.
    ///
    /// # Arguments
    ///
    /// * `error_id` - An integer representing the error ID.
    /// * `msg` - A string containing the error message.
    ///
    /// # Returns
    ///
    /// A new `RetResult` instance with `is_error` set to `true`.
    #[inline]
    pub fn error(error_id: i32, msg: String) -> RetResult {
        RetResult {
            is_error: true,
            error_id,
            msg,
            arguments: Vec::new(),
        }
    }

    /// Adds a serialized argument to the `RetResult`.
    ///
    /// # Arguments
    ///
    /// * `p` - The argument to be serialized and added.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The type of the argument to be serialized.
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
    /// * `dr` - The `DataOwnedReader` to create the `RetResult` from.
    ///
    /// # Returns
    ///
    /// A `Result` containing the new `RetResult` or an error.
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

    /// Returns the number of arguments in the `RetResult`.
    ///
    /// # Returns
    ///
    /// The number of arguments.
    #[inline]
    pub fn len(&self) -> usize {
        self.arguments.len()
    }

    /// Checks if the `RetResult` has no arguments.
    ///
    /// # Returns
    ///
    /// `true` if there are no arguments, `false` otherwise.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.arguments.is_empty()
    }

    /// Checks if the `RetResult` is an error and returns it if not.
    ///
    /// # Returns
    ///
    /// A `Result` containing the `RetResult` or an error.
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
    ///
    /// # Returns
    ///
    /// A `Result` containing a mutable reference to the argument or an error.
    #[inline]
    pub fn get(&mut self, index: usize) -> io::Result<&mut DataOwnedReader> {
        if index >= self.len() {
            return Err(io::Error::new(ErrorKind::Other, "index >= len"));
        }
        Ok(&mut self.arguments[index])
    }

    /// Deserializes the first argument in the `RetResult`.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The type to deserialize to.
    ///
    /// # Returns
    ///
    /// A `Result` containing the deserialized value or an error.
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
