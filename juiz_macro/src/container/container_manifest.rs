use crate::proc_macro::TokenStream;
use crate::util::typed::{arguments_manifest_statements, manifest_expression};
use quote::{format_ident, quote};
use syn::TypePath;

pub(crate) fn manifest_tokenstream() -> TokenStream {
    quote! {
        pub fn manifest2() -> juiz_sdk::prelude::ContainerManifest {
        }
    }
    .into()
}

pub(crate) fn component_manifest_tokenstream(proc_type_str: String) -> TokenStream {
    let manifest_function_name_ident = format_ident!("{}", proc_type_str + "_manifest");
    quote! {
        pub fn #manifest_function_name_ident() -> juiz_sdk::prelude::ContainerManifest {
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
        .unwrap_or_else(|| format!("Default description of Container({function_name})"))
}

/// attr の `model = { path = "...", ... }` (型の既定のモデル、docs/spec/container-model.md §5.6) を
/// manifest に入れる文。中身の検査は実行時 (manifest を作るとき) に `ModelManifest::from_value` で
fn model_statement(manifest_attr: &serde_json::Value) -> proc_macro2::TokenStream {
    match manifest_attr.get("model") {
        None => quote! {},
        Some(model) => {
            let json = model.to_string();
            quote! {
                manif.model = Some(juiz_sdk::manifests::ModelManifest::from_value(
                    &juiz_sdk::serde_json::from_str::<juiz_sdk::serde_json::Value>(#json)?,
                )?);
            }
        }
    }
}

pub(crate) fn construct_manif_tokenstream(
    function_name: String,
    manifest_attr: &serde_json::Value,
    arg_map: &Vec<(TypePath, syn::Ident)>,
) -> TokenStream {
    let description = description_of(manifest_attr, &function_name);
    // itemから受け取った引数情報をイテレートしつつ、attrからもらった部分にデフォルト引数およびデスクリプションの情報があれば、それを追加していく
    let arguments = arguments_manifest_statements(manifest_attr, arg_map);
    let model = model_statement(manifest_attr);
    manifest_expression(
        &function_name,
        quote! { juiz_sdk::prelude::ContainerManifest },
        quote! {
            let mut manif = ContainerManifest::new( #function_name ).description(#description);
            #arguments
            #model
        },
    )
    .into()
}

pub(crate) fn component_construct_manif_tokenstream(
    function_name: String,
    manifest_attr: &serde_json::Value,
    arg_map: &Vec<(TypePath, syn::Ident)>,
    factory_name: String,
) -> TokenStream {
    let description = description_of(manifest_attr, &function_name);
    let arguments = arguments_manifest_statements(manifest_attr, arg_map);
    let model = model_statement(manifest_attr);
    manifest_expression(
        &function_name,
        quote! { juiz_sdk::prelude::ContainerManifest },
        quote! {
            let mut manif = ContainerManifest::new( #function_name ).description(#description);
            manif = manif.factory(#factory_name);
            #arguments
            #model
        },
    )
    .into()
}
