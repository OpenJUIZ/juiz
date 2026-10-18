use proc_macro::TokenStream;
use quote::{format_ident, quote};
use serde_json::{Map, Value};
use syn::{parse_macro_input, ItemFn, Stmt};

use crate::util::{build_info_tokenstream, parse_attr};

pub(crate) fn juiz_component_manifest_inner(attr: TokenStream) -> TokenStream {
    let manifest_attr = parse_attr(attr);
    let component_name = match manifest_attr.get("component_name") {
        Some(cn_value) => match cn_value.as_str() {
            Some(cn) => cn.to_owned(),
            None => {
                panic!("component_nameは文字列出なければなりません");
            }
        },
        None => {
            panic!("component_nameを指定してください")
        }
    };

    let token_stream: TokenStream = quote! {
        #[no_mangle]
        pub unsafe extern "Rust" fn component_manifest() -> ComponentManifest {
            //env_logger::init();
            let mut manif = ComponentManifest::new(#component_name);
        }
    }
    .into();

    let mut item_fn: ItemFn = syn::parse_macro_input!(token_stream as ItemFn);

    let proc_attrs: Vec<Value> = match manifest_attr.get("processes") {
        Some(v) => match v.as_array() {
            Some(arr) => arr.clone(),
            None => {
                panic!("processesアトリビュートはリストでなければなりません。")
            }
        },
        None => Vec::new(),
    };
    for proc_attr in proc_attrs.iter() {
        let proc_name = proc_attr.as_str().unwrap();
        let process_manif_function_ident = format_ident!("{}_manifest", proc_name);
        let token_stream: TokenStream = quote! {
            manif = manif.add_process(#process_manif_function_ident());
        }
        .into();
        item_fn
            .block
            .stmts
            .push(parse_macro_input!(token_stream as Stmt));
    }

    let cont_attrs: Map<String, Value> = match manifest_attr.get("containers") {
        Some(v) => match v.as_object() {
            Some(arr) => arr.clone(),
            None => {
                panic!("containersアトリビュートは辞書でなければなりません。")
            }
        },
        None => Map::new(),
    };
    for (cont_name, cont_proc_list) in cont_attrs.iter() {
        // let cont_name = cont_name.as_str().unwrap();
        //println!("cont: {}", cont_name);
        let cont_manif_function_ident = format_ident!("{}_manifest", cont_name);
        let cont_manif_ident = format_ident!("{}_cont_manif", cont_name);
        let mut token_stream = quote! {
            let mut #cont_manif_ident = #cont_manif_function_ident();
        };
        // item_fn.block.stmts.push(parse_macro_input!(token_stream01 as Stmt));

        let cont_proc_attrs: Vec<Value> = match cont_proc_list.as_array() {
            Some(arr) => arr.clone(),
            None => {
                panic!("containersアトリビュートは辞書でなければなりません。")
            }
        };
        for proc_attr in cont_proc_attrs.iter() {
            let proc_name = proc_attr.as_str().unwrap();
            let process_manif_function_ident = format_ident!("{}_manifest", proc_name);
            //println!("proc:{}", proc_name);
            let token_stream02 = quote! {
                //println!("cont_manif.add_process(proc_name)");
                #cont_manif_ident = #cont_manif_ident.add_process(#process_manif_function_ident());
                //
            };
            //println!("token_stream02:{}", token_stream02.to_string());
            //item_fn.block.stmts.push(parse_macro_input!(token_stream02 as Stmt));
            token_stream = quote! {

                #token_stream
                #token_stream02
            };
        }

        let token_stream03 = quote! {
            manif = manif.add_container(#cont_manif_ident);
        };
        let token_stream_asm: TokenStream = quote! {
            {
            #token_stream
            #token_stream03
            }
        }
        .into();
        // "token_stream_asm: {}", token_stream_asm.to_string());
        item_fn
            .block
            .stmts
            .push(parse_macro_input!(token_stream_asm as Stmt));
    }

    let return_token_stream: TokenStream = quote! {
        return manif;
    }
    .into();

    item_fn
        .block
        .stmts
        .push(parse_macro_input!(return_token_stream as Stmt));

    // 最後の最後に全部の関数を並べる。
    let build_info = build_info_tokenstream("component_manifest");
    let ts = quote! {
        #build_info

        #item_fn
    };

    // println!("{}", ts.to_string());

    ts.into()
}
