use std::collections::HashMap;

use crate::proc_macro::TokenStream;
use crate::util::parse_arg_map;
use quote::quote;
use syn::{parse_quote, ItemFn, TypePath};

use super::parse_arg_map_skip_first;

pub(crate) fn change_argument_to_capsule_map(ast: &mut ItemFn) -> Vec<(TypePath, syn::Ident)> {
    // ここで引数の名前と型名のリストを受け取っておく
    let arg_map = parse_arg_map(ast);

    // ここで全部 CapsuleMapにしてしまう。唯一の引数は __the_onlyなんちゃらかんちゃら
    ast.sig.inputs.clear();
    let ass: ItemFn = parse_quote!(
        fn __unused_function_name__(
            __the_only_argument_modified_by_juiz_process_macro__: juiz_sdk::prelude::CapsuleMap,
        ) {
        }
    );
    let u = ass.sig.inputs;
    ast.sig.inputs.push(u.first().unwrap().clone());
    arg_map
}

pub(crate) fn change_container_process_argument_to_capsule_map(
    ast: &mut ItemFn,
) -> HashMap<TypePath, syn::Ident> {
    let fst_arg = ast.sig.inputs.first().unwrap().clone();

    // ここで引数の名前と型名のリストを受け取っておく
    let arg_map = parse_arg_map_skip_first(ast);

    // ここで全部 CapsuleMapにしてしまう。唯一の引数は __the_onlyなんちゃらかんちゃら
    ast.sig.inputs.clear();
    let ass: ItemFn = parse_quote!(
        fn __unused_function_name__(
            __the_only_argument_modified_by_juiz_process_macro__: juiz_sdk::prelude::CapsuleMap,
        ) {
        }
    );
    let u = ass.sig.inputs;
    ast.sig.inputs.push(fst_arg);
    ast.sig.inputs.push(u.first().unwrap().clone());

    //println!("ast: {}", ast.to_token_stream().to_string());
    //println!("arg_map: {arg_map:?}");
    arg_map
}

pub(crate) fn get_body_tokenstream(ast: &ItemFn) -> TokenStream {
    let mut body = quote! {};
    for s in &ast.block.stmts {
        body = quote! {
            #body
            #s
        };
    }
    quote! {
        let mut body = || { #body };
    }
    .into()
}
