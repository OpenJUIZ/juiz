use crate::proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn, Stmt};

use super::gen_process_factory::factory_tokenstream;
use super::process_manifest::{construct_manif_tokenstream, manifest_tokenstream};
use crate::util::typed::change_return_to_capsule;
use crate::util::{argument_statement, exit_tokenstream};
use crate::util::parse_attr;
use crate::util::{change_argument_to_capsule_map, get_body_tokenstream};

pub(crate) fn juiz_process_inner(attr: TokenStream, item: TokenStream) -> TokenStream {
    let manifest_attr = parse_attr(attr);
    let mut ast = parse_macro_input!(item as ItemFn);
    let function_name = ast.sig.ident.clone().to_string();

    // ここで全部 CapsuleMapにしてしまう。元の引数のデータはarg_mapで受け取る
    let arg_map = change_argument_to_capsule_map(&mut ast);

    // println!("arg_map!: {arg_map:?}");
    // このあとclearしちゃうのでまずbodyを保存。
    // 戻り値が JuizType の型なら JuizResult<Capsule> に書き換える (issue #192)
    let return_kind = change_return_to_capsule(&mut ast);
    let body: TokenStream = get_body_tokenstream(&ast, &return_kind);

    ast.block.stmts.clear();

    // 最初に取得した引数のリストから、CapsuleMapから引数それぞれにtry_intoするコードを生成する。
    for (type_name, arg_name) in arg_map.iter() {
        let set_original_argument_statement: TokenStream = argument_statement(type_name, arg_name);
        ast.block
            .stmts
            .push(parse_macro_input!(set_original_argument_statement as Stmt));
    }

    // bodyを返す
    ast.block.stmts.push(parse_macro_input!(body as Stmt));
    // 値を返す部分を生成
    let exit: TokenStream = exit_tokenstream(&return_kind);
    ast.block.stmts.push(parse_macro_input!(exit as Stmt));

    // ここからmanifest関数を自動生成する。
    // まず土台となる関数定義
    let manifest2_tokenstream = manifest_tokenstream();
    let mut manifest2_item_fn: ItemFn = syn::parse_macro_input!(manifest2_tokenstream as ItemFn);

    let return_manifest = construct_manif_tokenstream(function_name, &manifest_attr, &arg_map, &return_kind);
    manifest2_item_fn
        .block
        .stmts
        .push(parse_macro_input!(return_manifest as Stmt));

    // factoryを自動生成する
    let fts = factory_tokenstream(ast.sig.ident.clone());
    let factory_item_fn = proc_macro2::TokenStream::from(fts);

    // 最後の最後に全部の関数を並べる。
    quote! {
        #factory_item_fn
        #ast
        #manifest2_item_fn
        //#factory_item_fn
    }
    .into()
}
