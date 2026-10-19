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

/// CapsuleMap から引数を一つ取り出して元の型に変換する文を生成する。
/// Value 型の引数は TryInto<Value> を使うと直列化用の {"__value__": ..., "__option__": ...} で包まれるので、
/// 中身の値をそのまま取り出す。
pub(crate) fn argument_statement(type_name: &TypePath, arg_name: &syn::Ident) -> TokenStream {
    let is_value = type_name.qself.is_none()
        && type_name
            .path
            .segments
            .last()
            .is_some_and(|s| s.ident == "Value" && s.arguments.is_empty());
    if is_value {
        quote!(
            let #arg_name : #type_name = __the_only_argument_modified_by_juiz_process_macro__.get(stringify!(#arg_name))?.lock_as_value(|v| v.clone())?;
        ).into()
    } else {
        quote!(
            let #arg_name : #type_name = __the_only_argument_modified_by_juiz_process_macro__.get(stringify!(#arg_name))?.try_into()?;
        ).into()
    }
}
