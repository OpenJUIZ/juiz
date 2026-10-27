use crate::proc_macro::TokenStream;
use crate::util::typed::{
    arguments_manifest_statements, manifest_expression, output_manifest_statement, ReturnKind,
};
use quote::{format_ident, quote};
use syn::TypePath;

pub(crate) fn manifest_tokenstream() -> TokenStream {
    quote! {
        pub(crate) fn manifest2() -> juiz_sdk::prelude::ProcessManifest {
        }
    }
    .into()
}

pub(crate) fn component_manifest_tokenstream(proc_type_str: String) -> TokenStream {
    let manifest_function_name_ident = format_ident!("{}", proc_type_str + "_manifest");
    quote! {
        pub(crate) fn #manifest_function_name_ident() -> juiz_sdk::prelude::ProcessManifest {
        }
    }
    .into()
}

fn description_of(manifest_attr: &serde_json::Value, function_name: &str) -> String {
    // attr変数から読み取った値からdescriptionを取得
    manifest_attr
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_owned())
        .unwrap_or_else(|| format!("Default description of Process({function_name})"))
}

/// `construct_manif` (`let mut manif = ...;`) に引数と戻り値を足す
fn construct_manif_inner(
    manifest_attr: &serde_json::Value,
    arg_map: &Vec<(TypePath, syn::Ident)>,
    return_kind: &ReturnKind,
    construct_manif: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    // itemから受け取った引数情報をイテレートしつつ、attrからもらった部分にデフォルト引数およびデスクリプションの情報があれば、それを追加していく
    let arguments = arguments_manifest_statements(manifest_attr, arg_map);
    let output = output_manifest_statement(return_kind);
    quote! {
        #construct_manif
        #arguments
        #output
    }
}

pub(crate) fn construct_manif_tokenstream(
    container_type_ident: syn::Ident,
    function_name: String,
    manifest_attr: &serde_json::Value,
    arg_map: &Vec<(TypePath, syn::Ident)>,
    return_kind: &ReturnKind,
) -> TokenStream {
    let description = description_of(manifest_attr, &function_name);
    // ここでmanifestデータの基本データを作成する部分
    let construct_manif = quote! {
        let mut manif = ProcessManifest::new( #function_name ).description(#description).container(#container_type_ident ::manifest2() );
    };
    let construct_manif =
        construct_manif_inner(manifest_attr, arg_map, return_kind, construct_manif);
    manifest_expression(
        &function_name,
        quote! { juiz_sdk::prelude::ProcessManifest },
        construct_manif,
    )
    .into()
}

pub(crate) fn component_construct_manif_tokenstream(
    container_type_ident: syn::Ident,
    function_name: String,
    manifest_attr: &serde_json::Value,
    arg_map: &Vec<(TypePath, syn::Ident)>,
    return_kind: &ReturnKind,
    factory_name: String,
) -> TokenStream {
    let container_manifest_function_name_ident =
        format_ident!("{}_manifest", container_type_ident.to_string());
    let description = description_of(manifest_attr, &function_name);
    // ここでmanifestデータの基本データを作成する部分
    let construct_manif = quote! {
        let mut manif = ProcessManifest::new( #function_name ).description(#description).container(#container_manifest_function_name_ident() );
    };
    let construct_manif =
        construct_manif_inner(manifest_attr, arg_map, return_kind, construct_manif);
    manifest_expression(
        &function_name,
        quote! { juiz_sdk::prelude::ProcessManifest },
        quote! {
            #construct_manif
            manif = manif.factory(#factory_name);
        },
    )
    .into()
}
