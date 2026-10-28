//! 引数・戻り値の型ごとのコード生成 (issue #192)
//!
//! 今までの型 (`bool`、`i64`、`f64`、`String`、`Value`、`DynamicImage`、`Vec<Value | f64 | i64>`) は今のとおり
//! 型なしの引数にする。それ以外の型は `juiz_sdk::types::JuizType` を実装した型として扱い、
//! manifest に型参照 (`schema`) を載せ、引数は `JuizType::extract` で受け取る。

use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use serde_json::{json, Value};
use syn::{GenericArgument, PathArguments, ReturnType, Type, TypePath};

fn joined_type_name(type_path: &TypePath) -> String {
    type_path
        .path
        .segments
        .iter()
        .map(|seg| seg.ident.to_string())
        .collect::<Vec<String>>()
        .join("::")
}

/// `Vec<要素>` の要素の型名
fn vec_element_type_name(type_path: &TypePath) -> Option<String> {
    let seg = type_path.path.segments.first()?;
    seg.arguments
        .clone()
        .into_token_stream()
        .into_iter()
        .find_map(|v| match v {
            proc_macro2::TokenTree::Ident(ident) => Some(ident.to_string()),
            _ => None,
        })
}

/// 今までの書き方の (型なしの) 引数か
pub(crate) fn is_untyped_argument(type_path: &TypePath) -> bool {
    match joined_type_name(type_path).as_str() {
        "bool" | "i64" | "f64" | "String" | "Value" | "DynamicImage" => true,
        "Vec" => matches!(
            vec_element_type_name(type_path).as_deref(),
            Some("Value" | "f64" | "i64")
        ),
        _ => false,
    }
}

/// manifest に引数を一つ足す文 (`manif = manif.add_xxx_arg(...);`) を生成する。
///
/// 生成した文は `JuizResult` を返すクロージャの中に置く (型付きの引数は `?` を使う)。
/// `manif` は `ProcessManifest` でも `ContainerManifest` でもよい。
pub(crate) fn argument_manifest_statement(
    type_path: &TypePath,
    arg_name: &str,
    description: &str,
    default: Option<&Value>,
    all_typed: bool,
) -> TokenStream {
    // 引数のタイプで分岐。タイプはitemから受け取ったタイプが優先。defaultが別のタイプだったらデフォルトのデフォルトが振られる。
    let type_name = if all_typed {
        String::new()
    } else {
        joined_type_name(type_path)
    };
    match type_name.as_str() {
        "bool" => {
            let default_value = default.and_then(|v| v.as_bool()).unwrap_or(false);
            quote! { manif = manif.add_bool_arg(#arg_name, #description, #default_value); }
        }
        "i64" => {
            let default_value = default.and_then(|v| v.as_i64()).unwrap_or(0);
            quote! { manif = manif.add_int_arg(#arg_name, #description, #default_value); }
        }
        "f64" => {
            let default_value = default.and_then(|v| v.as_f64()).unwrap_or(0.0);
            quote! { manif = manif.add_float_arg(#arg_name, #description, #default_value); }
        }
        "String" => {
            let default_value = default.and_then(|v| v.as_str()).unwrap_or("");
            quote! { manif = manif.add_string_arg(#arg_name, #description, #default_value); }
        }
        "Value" => {
            let value_str = serde_json::to_string(default.unwrap_or(&json!({}))).unwrap();
            quote! { manif = manif.add_object_arg(#arg_name, #description, juiz_sdk::prelude::serde_json::from_str(#value_str).unwrap()); }
        }
        "DynamicImage" => {
            quote! { manif = manif.add_image_arg(#arg_name, #description); }
        }
        _ if !all_typed && is_untyped_argument(type_path) => {
            // Vec<Value | f64 | i64>
            quote! { manif = manif.add_array_arg(#arg_name, #description, Vec::new()); }
        }
        _ => {
            // 型付きの引数 (仕様 datatype.md §6)。既定値は辞書の表し方で、書かなければ「既定値なし」(null)
            let value_str = serde_json::to_string(default.unwrap_or(&Value::Null)).unwrap();
            quote! {
                manif = manif.add_typed_arg(
                    #arg_name,
                    #description,
                    <#type_path as juiz_sdk::types::JuizType>::type_schema()?,
                    juiz_sdk::prelude::serde_json::from_str(#value_str).unwrap(),
                )?;
            }
        }
    }
}

/// マクロの属性 `typed = "all"` があるか。あれば今までの型 (`f64` など) の引数・`Value` の戻り値も型付きにする
/// (juiz-idl が process 宣言から作る雛形が使う。manifest がシグネチャ `<namespace>.processes.json` と一致する)
pub(crate) fn all_typed(manifest_attr: &Value) -> bool {
    match manifest_attr.get("typed") {
        None => false,
        Some(v) if v.as_str() == Some("all") => true,
        Some(v) => panic!("typed の値は \"all\" だけです ({v})"),
    }
}

/// 属性の `arguments = { default = ... }`。`{ 名前 = 値 ... }` のほか、JSON の文字列
/// (`default = "{\"speed\": 1.0}"`) でも書ける (属性の書き方では書けない負の数・配列・null を書くため)
fn argument_defaults(argument_value: &Value) -> Value {
    match argument_value.get("default") {
        None => json!({}),
        Some(Value::String(s)) => match serde_json::from_str::<Value>(s) {
            Ok(v) if v.is_object() => v,
            Ok(v) => {
                panic!("arguments の default の JSON はオブジェクトでなければなりません ({v})")
            }
            Err(e) => panic!("arguments の default の JSON が読めません ({e})"),
        },
        Some(v) => v.clone(),
    }
}

/// マクロの属性の `arguments = { default = {...} description = {...} }` から、引数ごとの文を作る
pub(crate) fn arguments_manifest_statements(
    manifest_attr: &Value,
    arg_map: &[(TypePath, syn::Ident)],
) -> TokenStream {
    let empty_value = json!({});
    let all_typed = all_typed(manifest_attr);
    let argument_value = manifest_attr.get("arguments").unwrap_or(&empty_value);
    let defaults = argument_defaults(argument_value);
    let descriptions = argument_value.get("description").unwrap_or(&empty_value);
    let mut statements = quote! {};
    for (type_path, arg_name_ident) in arg_map.iter() {
        let arg_name = arg_name_ident.to_string();
        let default_desc = format!("Default description for argument {arg_name}");
        let description = descriptions
            .get(arg_name.as_str())
            .and_then(|v| v.as_str())
            .unwrap_or(default_desc.as_str());
        let statement = argument_manifest_statement(
            type_path,
            &arg_name,
            description,
            defaults.get(arg_name.as_str()),
            all_typed,
        );
        statements = quote! {
            #statements
            #statement
        };
    }
    statements
}

/// manifest を作る文 (`let mut manif = ...;` から始まる) を、manifest を返す式にする。
///
/// 型付きの引数・戻り値の型参照を作るときのエラー (既定値が型に合わないなど) は、
/// プラグインを読み込むときに型の名前と一緒に panic で知らせる。
pub(crate) fn manifest_expression(
    function_name: &str,
    manifest_type: TokenStream,
    construct_manif: TokenStream,
) -> TokenStream {
    quote! {
        {
            #[allow(unused_mut)]
            let build = || -> juiz_sdk::prelude::JuizResult<#manifest_type> {
                #construct_manif
                Ok(manif)
            };
            match build() {
                Ok(manif) => manif,
                Err(e) => panic!("manifest of '{}' is invalid. Error({:#})", #function_name, e),
            }
        }
    }
}

/// 関数の戻り値の扱い
pub(crate) enum ReturnKind {
    /// 書き換えない。`JuizResult<Capsule>` / `JuizResult<JuizResponse>` (今の書き方) と、コンテナの `JuizResult<Box<T>>`
    AsIs,
    /// `JuizResult<Value>`。型なしの値として返す
    Value,
    /// `JuizResult<T>` (`T: JuizType`)。`JuizType::into_capsule` で返し、manifest に `output_schema` を載せる
    Typed(Type),
}

fn is_named(ty: &Type, names: &[&str]) -> bool {
    match ty {
        Type::Path(p) => {
            p.qself.is_none()
                && p.path.segments.last().is_some_and(|s| {
                    names.contains(&s.ident.to_string().as_str()) && s.arguments.is_empty()
                })
        }
        _ => false,
    }
}

/// `JuizResult<T>` / `Result<T, E>` の `T`
fn result_ok_type(output: &ReturnType) -> Option<Type> {
    let ReturnType::Type(_, ty) = output else {
        return None;
    };
    let Type::Path(p) = ty.as_ref() else {
        return None;
    };
    let seg = p.path.segments.last()?;
    if seg.ident != "JuizResult" && seg.ident != "Result" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    args.args.iter().find_map(|a| match a {
        GenericArgument::Type(t) => Some(t.clone()),
        _ => None,
    })
}

pub(crate) fn return_kind(output: &ReturnType, all_typed: bool) -> ReturnKind {
    match result_ok_type(output) {
        Some(t) if is_named(&t, &["Capsule", "JuizResponse"]) => ReturnKind::AsIs,
        Some(t) if !all_typed && is_named(&t, &["Value"]) => ReturnKind::Value,
        Some(t) => ReturnKind::Typed(t),
        // 戻り値が JuizResult でなければ、今のとおりそのまま (コンパイルエラーで知らせる)
        None => ReturnKind::AsIs,
    }
}

/// 戻り値の型を `JuizResult<Capsule>` に書き換え、元の型を返す
pub(crate) fn change_return_to_capsule(ast: &mut syn::ItemFn, all_typed: bool) -> ReturnKind {
    let kind = return_kind(&ast.sig.output, all_typed);
    if !matches!(kind, ReturnKind::AsIs) {
        ast.sig.output =
            syn::parse_quote!(-> juiz_sdk::prelude::JuizResult<juiz_sdk::prelude::Capsule>);
    }
    kind
}

/// manifest に戻り値の型参照を載せる文
pub(crate) fn output_manifest_statement(kind: &ReturnKind) -> TokenStream {
    match kind {
        ReturnKind::Typed(t) => quote! { manif = manif.returns::<#t>()?; },
        _ => quote! {},
    }
}
