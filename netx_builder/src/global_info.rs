use proc_macro2::{Ident, TokenStream};
use syn::punctuated::Punctuated;
use syn::{FnArg, ReturnType, Token};

/// Represents information about a function.
#[derive(Clone, Debug)]
pub struct FuncInfo {
    /// A tag associated with the function.
    pub tag: i32,
    /// A type tag for the function.
    pub tt: u8,
    /// The name of the function.
    pub func_name: String,
    /// The types of the function's arguments.
    pub args_type: Vec<TokenStream>,
    /// The inputs of the function.
    pub inputs: Punctuated<FnArg, Token![,]>,
    /// The names of the function's inputs.
    pub input_names: Vec<Ident>,
    /// The return type of the function.
    pub output: ReturnType,
}
