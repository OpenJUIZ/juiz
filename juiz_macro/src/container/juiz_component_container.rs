use super::container_manifest::{
    component_construct_manif_tokenstream, component_manifest_tokenstream,
};
use super::gen_container_factory::component_factory_tokenstream;
use crate::proc_macro::TokenStream;
use crate::util::{change_argument_to_capsule_map, get_body_tokenstream};
use quote::quote;
use syn::{parse_macro_input, ItemFn, Stmt};

use crate::util::parse_attr;
// #use super::process_manifest::{construct_manif_tokenstream, manifest_tokenstream};
// #use super::decorate_function_arg::{get_body_tokenstream, change_argument_to_capsule_map};
//use super::gen_process_factory::factory_tokenstream;

pub(crate) fn juiz_component_container_inner(attr: TokenStream, item: TokenStream) -> TokenStream {
    let manifest_attr = parse_attr(attr);
    let mut ast = parse_macro_input!(item as ItemFn);
    let function_name = ast.sig.ident.clone().to_string();
    let factory_name = function_name.clone() + "_factory";

    // ここで全部 CapsuleMapにしてしまう。元の引数のデータはarg_mapで受け取る
    let arg_map = change_argument_to_capsule_map(&mut ast);

    // このあとclearしちゃうのでまずbodyを保存。
    let body: TokenStream = get_body_tokenstream(&ast);

    ast.block.stmts.clear();

    // 最初に取得した引数のリストから、CapsuleMapから引数それぞれにtry_intoするコードを生成する。
    for (type_name, arg_name) in arg_map.iter() {
        let set_original_argument_statement: TokenStream = quote!(
            let #arg_name : #type_name = __the_only_argument_modified_by_juiz_process_macro__.get(stringify!(#arg_name))?.try_into()?;
        ).into();
        ast.block
            .stmts
            .push(parse_macro_input!(set_original_argument_statement as Stmt));
    }

    // bodyを返す
    ast.block.stmts.push(parse_macro_input!(body as Stmt));
    // 値を返す部分を生成
    let exit: TokenStream = quote! { { body() } }.into();
    ast.block.stmts.push(parse_macro_input!(exit as Stmt));

    // ここからmanifest関数を自動生成する。
    // まず土台となる関数定義
    let manifest2_tokenstream = component_manifest_tokenstream(function_name.clone());
    let mut manifest2_item_fn: ItemFn = syn::parse_macro_input!(manifest2_tokenstream as ItemFn);

    let return_manifest = component_construct_manif_tokenstream(
        function_name,
        &manifest_attr,
        &arg_map,
        factory_name.clone(),
    );
    manifest2_item_fn
        .block
        .stmts
        .push(parse_macro_input!(return_manifest as Stmt));

    // factoryを自動生成する
    let fts = component_factory_tokenstream(ast.sig.ident.clone(), factory_name);
    let factory_item_fn = proc_macro2::TokenStream::from(fts);

    // 最後の最後に全部の関数を並べる。
    quote! {
        #factory_item_fn
        #ast
        #manifest2_item_fn
    }
    .into()
}
