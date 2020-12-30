use proc_macro2::{TokenStream, Ident};
use syn::punctuated::Punctuated;
use syn::{FnArg, Token, ReturnType};

#[derive(Clone,Debug)]
pub  struct FuncInfo {
    pub tag:i32,
    pub tt:u8,
    pub func_name:String,
    pub args_type:Vec<TokenStream>,
    pub inputs: Punctuated<FnArg, Token![,]>,
    pub input_names:Vec<Ident>,
    pub output: ReturnType,

}

