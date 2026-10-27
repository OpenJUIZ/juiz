use crate::proc_macro::TokenStream;
use crate::util::typed::{
    arguments_manifest_statements, manifest_expression, output_manifest_statement, ReturnKind,
};
use quote::{format_ident, quote};
use syn::TypePath;

pub(crate) fn manifest_tokenstream() -> TokenStream {
    quote! {
        pub(crate) fn manifest() -> juiz_sdk::prelude::ProcessManifest {
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

fn construct_manif_inner(
    function_name: &str,
    manifest_attr: &serde_json::Value,
    arg_map: &Vec<(TypePath, syn::Ident)>,
    return_kind: &ReturnKind,
) -> proc_macro2::TokenStream {
    // attr変数から読み取った値からdescriptionを取得
    let default_description = format!("Default description of Process({function_name})");
    let description = manifest_attr
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or(default_description.as_str());

    // itemから受け取った引数情報をイテレートしつつ、attrからもらった部分にデフォルト引数およびデスクリプションの情報があれば、それを追加していく
    let arguments = arguments_manifest_statements(manifest_attr, arg_map);
    let output = output_manifest_statement(return_kind);
    quote! {
        let mut manif = ProcessManifest::new( #function_name ).description(#description);
        #arguments
        #output
    }
}

pub(crate) fn construct_manif_tokenstream(
    function_name: String,
    manifest_attr: &serde_json::Value,
    arg_map: &Vec<(TypePath, syn::Ident)>,
    return_kind: &ReturnKind,
) -> TokenStream {
    let construct_manif =
        construct_manif_inner(&function_name, manifest_attr, arg_map, return_kind);
    manifest_expression(
        &function_name,
        quote! { juiz_sdk::prelude::ProcessManifest },
        construct_manif,
    )
    .into()
}

pub(crate) fn component_construct_manif_tokenstream(
    function_name: String,
    manifest_attr: &serde_json::Value,
    arg_map: &Vec<(TypePath, syn::Ident)>,
    return_kind: &ReturnKind,
    factory_name: String,
) -> TokenStream {
    let construct_manif =
        construct_manif_inner(&function_name, manifest_attr, arg_map, return_kind);
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
