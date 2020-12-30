mod global_info;


extern crate proc_macro;
use quote::{quote,format_ident};
use proc_macro::{TokenStream};
use syn::{parse_macro_input, ItemTrait, parse_quote, NestedMeta, Lit, TraitItemMethod, TraitItem, ItemImpl, FnArg, Type, ReturnType, PathArguments, GenericArgument, Pat};
use proc_macro_roids::{namespace_parameter};
use global_info::*;


fn have_tag(method:&TraitItemMethod)->Option<i32>{
    if let Some(tag) =namespace_parameter(&method.attrs,&parse_quote!(tag)){
       match tag {
           NestedMeta::Lit(value)=>{
               match value {
                   Lit::Int(v)=>{
                      return Some( v.to_string().parse::<i32>().expect("tag type error not i32"))
                   },
                   _=>panic!("tag type error not i32")
               }
           },
           _=>panic!("tag error")
       }
    }
    None
}

fn get_function_tt(tag_id:i32,func_name:String, rt:Type)->u8{
    match rt{
        Type::Path(tp)=>{
            if let Some(seq)= tp.path.segments.first(){
                if seq.ident=="Result"{
                    match &seq.arguments{
                        PathArguments::AngleBracketed(arg)=>{
                            if arg.args.len()==2{
                                if let GenericArgument::Type(Type::Path(  checkbox))= &arg.args[1] {
                                    if let Some(checkbox_name)= checkbox.path.segments.first(){
                                        if checkbox_name.ident=="Box"{
                                            return if let GenericArgument::Type(Type::Tuple(rt)) = &arg.args[0] {
                                                if rt.elems.is_empty(){
                                                    1
                                                }
                                                else{
                                                    2
                                                }
                                            } else {
                                                2
                                            }
                                        }
                                    }
                                }
                            }
                            panic!("4 error return type by:{} {},fix like Result<?,Box<dyn Error>>",tag_id,func_name)
                        },
                        _=>{
                            panic!("3 error return type by:{} {},fix like Result<?,Box<dyn Error>>",tag_id,func_name)
                        }
                    }
                }
            }
            panic!("2 error return type by:{} {}",tag_id,func_name)
        }
        _=>{
            panic!("1 error return type by:{} {}",tag_id,func_name)
        }
    }
}

fn get_impl_func(funcs:&[FuncInfo]) -> Vec<proc_macro2::TokenStream> {
    let mut ret=Vec::new();
    for func in funcs {
        let fn_name=format_ident!("{}",func.func_name);
        let inputs=func.inputs.clone();
        let output=func.output.clone();
        let input_names=func.input_names.clone();
        let tag=func.tag;
        match func.tt {
            0 => {
                ret.push(quote! {
                    async fn #fn_name(#inputs) {
                       call!(@run_not_err self.client=>#tag;#(#input_names ,)*);
                    }
                });
            },
            1 => {
                ret.push(quote! {
                    async fn #fn_name(#inputs) #output{
                        call!(@checkrun self.client=>#tag;#(#input_names ,)*);
                        Ok(())
                    }
                });
            },
            2 => {
                ret.push(quote! {
                    async fn #fn_name(#inputs) #output{
                       Ok(call!(self.client=>#tag;#(#input_names ,)*))
                    }
                });
            },
            _ => {
                panic!("error tt:{}", func.tt);
            }
        }
    }
    ret
}

#[proc_macro_attribute]
pub fn build_trait(args:TokenStream, input: TokenStream) -> TokenStream {
    let mut ast = parse_macro_input!(input as ItemTrait);
    let funcs = get_funcs_info(&mut ast);
    let controller_name=args.to_string();
    let interface_name=ast.ident.clone();
    let impl_interface_struct_name= format_ident!("___impl_{}_call",interface_name);
    let impl_func=get_impl_func(&funcs);
    let impl_interface=quote! {
        #[allow(non_camel_case_types)]
        pub struct #impl_interface_struct_name <T>{
               client:T
        }

        impl<T> #impl_interface_struct_name<T>{
            pub fn new(client:T)->#impl_interface_struct_name<T>{
                #impl_interface_struct_name{
                    client
                }
            }
        }

        #[aqueue_trait]
        impl<T:SessionSave+'static> #interface_name for #impl_interface_struct_name<Arc<Actor<NetXClient<T>>>>{
            #(#impl_func)*
        }

    };


    if controller_name != "" {
        let controller = format_ident!("{}",controller_name);
        let make=  funcs.iter().map(|func|{
            let struct_name=format_ident!("__struct{}",func.tag.to_string());
            let func_name=format_ident!("{}",func.func_name);
            let tt=func.tt;
            let tag=func.tag;

            let mut arg_names=Vec::new();
            let mut read_token=Vec::new();

            for (index,token) in func.args_type.iter().enumerate() {
                let arg_name=format_ident!("arg{}",index.to_string());
                read_token.push(quote! {
                  let #arg_name=data.get_le::<#token>()?;
                });
                arg_names.push(arg_name);
            }

            let args_len=func.args_type.len();
            let call= match tt{
                0=>{
                    quote! {
                        let args_len=data.get_le::<i32>()? as usize;
                        if args_len!=#args_len{
                             return Err("args len error".into());
                        }
                        #( #read_token)*
                        self.controller.#func_name (#(#arg_names,)*).await;
                        Ok(RetResult::success())
                    }
                }
                1=>{
                    quote! {
                        let args_len=data.get_le::<i32>()? as usize;
                        if args_len!=#args_len{
                             return Err("args len error".into());
                        }
                        #( #read_token)*
                        self.controller.#func_name (#(#arg_names,)*).await?;
                        Ok(RetResult::success())
                    }
                }
                2=>{
                    quote! {
                        let args_len=data.get_le::<i32>()? as usize;
                        if args_len!=#args_len{
                             return Err("args len error".into());
                        }
                        #( #read_token)*
                        let ret=self.controller.#func_name (#(#arg_names,)*).await?;
                        let mut result=RetResult::success();
                        result.add_arg_buff(ret.to_data());
                        Ok(result)
                    }
                }
                _=>{
                    quote! {
                           unimplemented!()
                    }
                }
            };
            quote! {
                {
                     struct #struct_name{
                         controller:Arc<#controller>
                     };

                     #[aqueue_trait]
                     impl FunctionInfo for #struct_name{
                         #[inline]
                         fn function_type(&self) -> u8 { #tt  }
                         #[inline]
                         async fn call(&self, mut data: Data) -> Result<RetResult, Box<dyn std::error::Error>> {
                             #call
                         }
                     }
                     let tag_id=#tag;
                     if let Some(_)= dict.insert(tag_id,Box::new(#struct_name{
                        controller:self.clone()
                     }) as Box<dyn FunctionInfo>){
                        panic!("Repeated function tag ID:{}",tag_id);
                     }
                 }
            }
        });
        let expanded = quote! {
            #[aqueue_trait]
            #ast

            impl IController for #controller{
                #[inline]
                fn register(self:Arc<Self>) -> Result<std::collections::HashMap<i32, Box<dyn FunctionInfo>>, Box<dyn std::error::Error>> {
                    use data_rw::{Data, ToData};
                    let mut dict=std::collections::HashMap::new();
                    #( #make)*
                    Ok(dict)
                }
            }

        };

        TokenStream::from(expanded)

    } else {

        let expanded = quote! {
            #[aqueue_trait]
            #ast

            #impl_interface
        };

        TokenStream::from(expanded)
    }
}

fn get_funcs_info(ast: &mut ItemTrait) -> Vec<FuncInfo> {
    let mut funcs = Vec::new();
    for item in &mut ast.items {
        if let TraitItem::Method(method) = item {
            if let Some(tag_id) = have_tag(method) {
                let sig = &mut method.sig;
                if sig.asyncness.is_some() {
                    let func_name = sig.ident.to_string();
                    let mut args_type = Vec::new();
                    let inputs=sig.inputs.clone();
                    let output=sig.output.clone();
                    let mut input_names =Vec::new();
                    for args in &sig.inputs {
                          if let FnArg::Typed(pat_type) =args {
                              let tt = &pat_type.ty;
                              args_type.push(quote!(#tt));

                              match &*pat_type.pat {
                                  Pat::Ident(a) => {
                                      input_names.push(a.ident.clone());
                                  },
                                  _ => { panic!("error arg name") }
                              }
                          }
                    }
                    let tt = match &sig.output {
                        ReturnType::Default => 0,
                        ReturnType::Type(_, tt) => get_function_tt(tag_id, func_name.clone(), *tt.clone())
                    };
                    let f_info = FuncInfo {
                        tag: tag_id,
                        tt,
                        func_name,
                        args_type,
                        inputs,
                        input_names,
                        output
                    };

                    funcs.push(f_info);
                }
            }
        }
    }
    funcs
}


#[proc_macro_attribute]
pub fn build_impl(_:TokenStream, input: TokenStream)-> TokenStream {
    let ast = parse_macro_input!(input as ItemImpl);
    TokenStream::from(quote! {
            #[aqueue_trait]
            #ast
        })
}


#[proc_macro_attribute]
pub fn tag(_:TokenStream, input: TokenStream) -> TokenStream {
    input
}

