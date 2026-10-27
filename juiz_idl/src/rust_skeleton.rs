//! `process` / `container` 宣言から Rust の雛形 (skeleton) を書き出す (仕様 §18、issue #192)
//!
//! 名前空間ごとに `<namespace>.skeleton.rs` を作る。中身は 1 つのコンポーネント (プラグイン) で、
//! 宣言ごとに `#[juiz_component_process]` などの付いた関数 (本体は `todo!()`) と
//! `juiz_component_manifest!` を持つ。コピーして本体を書いて使う (build.rs では作らない)。
//!
//! マクロに `typed = "all"` を付けるので、基本型の引数も型参照を持ち、manifest が
//! `<namespace>.processes.json` のシグネチャと一致する。型は `crate::…` のパスで参照するので、
//! `juiz_idl.rs` をクレートの一番上で読み込んでおく。

use juiz_types::{DataType, TypeSchema};
use serde_json::Value;

use crate::rust::{doc_lines, ident, rust_type, HEADER, KEYWORDS};
use crate::{
    ArgumentSignature, ContainerSignature, GeneratedFile, IdlError, ProcessSignature, Signatures,
};

/// 名前空間ごとの雛形
pub fn generate_rust_skeleton(signatures: &[Signatures]) -> Result<Vec<GeneratedFile>, IdlError> {
    signatures
        .iter()
        .filter(|s| !s.processes.is_empty() || !s.containers.is_empty())
        .map(|s| {
            Ok(GeneratedFile {
                path: format!("{}.skeleton.rs", s.namespace).into(),
                contents: skeleton(s)?,
            })
        })
        .collect()
}

fn generate_error(message: String) -> IdlError {
    IdlError::Generate { message }
}

/// 関数名・引数名。マクロは名前をそのまま `type_name` と引数名にするので、raw identifier にはできない
fn plain_ident(name: &str, what: &str) -> Result<String, IdlError> {
    if KEYWORDS.contains(&name) {
        return Err(generate_error(format!(
            "{what} '{name}' is a Rust keyword and cannot be used in the Rust skeleton"
        )));
    }
    Ok(name.to_owned())
}

/// `motor_state` → `MotorState`
fn camel_case(name: &str) -> String {
    name.split('_')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn schema_type(schema: &TypeSchema) -> Result<String, IdlError> {
    let t: DataType = schema
        .data_type()
        .map_err(|e| generate_error(format!("{}: {e}", schema.name)))?;
    rust_type(&t, "")
}

/// Rust の文字列リテラル
fn str_lit(s: &str) -> String {
    format!("{s:?}")
}

/// マクロの属性の中身 (`typed = "all"` と description と arguments)
fn attributes(p: &ProcessSignature, extra: &[String]) -> String {
    let mut lines: Vec<String> = extra.to_vec();
    lines.push("typed = \"all\"".to_owned());
    if let Some(d) = &p.description {
        lines.push(format!("description = {}", str_lit(d)));
    }
    let defaults: serde_json::Map<String, Value> = p
        .arguments
        .iter()
        .filter_map(|a| a.default.clone().map(|d| (a.name.clone(), d)))
        .collect();
    let descriptions: Vec<&ArgumentSignature> = p
        .arguments
        .iter()
        .filter(|a| a.description.is_some())
        .collect();
    if !defaults.is_empty() || !descriptions.is_empty() {
        let mut args = vec!["arguments = {".to_owned()];
        if !defaults.is_empty() {
            // 既定値は JSON の文字列で渡す (属性の書き方では負の数・配列・null を書けないため)
            let json = serde_json::to_string(&Value::Object(defaults)).unwrap();
            args.push(format!("    default = {}", str_lit(&json)));
        }
        if !descriptions.is_empty() {
            args.push("    description = {".to_owned());
            for a in descriptions {
                args.push(format!(
                    "        {} = {}",
                    a.name,
                    str_lit(a.description.as_deref().unwrap())
                ));
            }
            args.push("    }".to_owned());
        }
        args.push("}".to_owned());
        lines.push(args.join("\n"));
    }
    let body = lines.join("\n").replace('\n', "\n    ");
    format!("(\n    {body}\n)")
}

fn parameters(p: &ProcessSignature) -> Result<Vec<String>, IdlError> {
    p.arguments
        .iter()
        .map(|a| {
            Ok(format!(
                "{}: {}",
                plain_ident(&a.name, "argument")?,
                schema_type(&a.schema)?
            ))
        })
        .collect()
}

/// 引数を使っていない警告を出さないための文 (本体を書くときに消す)
fn use_arguments(names: &[String]) -> String {
    match names.len() {
        0 => String::new(),
        1 => format!("    let _ = {};\n", names[0]),
        _ => format!("    let _ = ({});\n", names.join(", ")),
    }
}

fn argument_names(p: &ProcessSignature) -> Vec<String> {
    p.arguments.iter().map(|a| a.name.clone()).collect()
}

fn process_fn(p: &ProcessSignature) -> Result<String, IdlError> {
    let name = plain_ident(&p.type_name, "process")?;
    let mut out = doc_lines(&p.description, "");
    out.push_str(&format!(
        "#[juiz_component_process{}]\n",
        attributes(p, &[])
    ));
    let params = parameters(p)?;
    out.push_str(&format!(
        "pub fn {name}({}) -> JuizResult<{}> {{\n{}    todo!({})\n}}\n",
        params.join(", "),
        schema_type(&p.output_schema)?,
        use_arguments(&argument_names(p)),
        str_lit(&format!("write the body of {name}"))
    ));
    Ok(out)
}

fn container_code(c: &ContainerSignature) -> Result<String, IdlError> {
    let name = plain_ident(&c.type_name, "container")?;
    let state = ident(&camel_case(&c.type_name));
    let mut out = doc_lines(&c.description, "");
    out.push_str(&format!(
        "/// Container `{name}` の状態 (JIDL では型付けしない。自由に書く)\n\
         pub struct {state} {{}}\n\n\
         #[juiz_component_container]\n\
         pub fn {name}() -> JuizResult<Box<{state}>> {{\n    Ok(Box::new({state} {{}}))\n}}\n"
    ));
    for p in c.container_processes.iter() {
        let pname = plain_ident(&p.type_name, "container_process")?;
        out.push('\n');
        out.push_str(&doc_lines(&p.description, ""));
        out.push_str(&format!(
            "#[juiz_component_container_process{}]\n",
            attributes(p, &[format!("container_type = {}", str_lit(&name))])
        ));
        let mut params = vec![format!("container: &mut ContainerImpl<{state}>")];
        params.extend(parameters(p)?);
        let mut used = vec!["container".to_owned()];
        used.extend(argument_names(p));
        out.push_str(&format!(
            "pub fn {pname}({}) -> JuizResult<{}> {{\n{}    todo!({})\n}}\n",
            params.join(", "),
            schema_type(&p.output_schema)?,
            use_arguments(&used),
            str_lit(&format!("write the body of {pname}"))
        ));
    }
    Ok(out)
}

fn skeleton(s: &Signatures) -> Result<String, IdlError> {
    let mut out = String::from(HEADER);
    out.push_str(&format!(
        "//\n\
         // 名前空間 {ns} の process / container 宣言から作った Rust の雛形。コピーして todo!() の本体を書く。\n\
         // 型は crate::… で参照するので、クレートの一番上で juiz_idl.rs を読み込んでおく:\n\
         //     include!(concat!(env!(\"OUT_DIR\"), \"/juiz_idl.rs\"));\n\
         // 1 つの名前空間が 1 つのコンポーネント (cdylib) になる。\n\n\
         #[allow(unused_imports)]\n\
         use juiz_sdk::prelude::*;\n",
        ns = s.namespace
    ));
    for p in s.processes.iter() {
        out.push('\n');
        out.push_str(&process_fn(p)?);
    }
    for c in s.containers.iter() {
        out.push('\n');
        out.push_str(&container_code(c)?);
    }
    // コンポーネントのマニフェスト
    out.push_str(&format!(
        "\njuiz_component_manifest!(\n    component_name = {}\n",
        str_lit(&s.namespace.replace('.', "_"))
    ));
    if !s.containers.is_empty() {
        out.push_str("    containers = {\n");
        for c in s.containers.iter() {
            let names: Vec<&str> = c
                .container_processes
                .iter()
                .map(|p| p.type_name.as_str())
                .collect();
            out.push_str(&format!(
                "        {} = [{}]\n",
                c.type_name,
                names.join(", ")
            ));
        }
        out.push_str("    }\n");
    }
    if !s.processes.is_empty() {
        let names: Vec<&str> = s.processes.iter().map(|p| p.type_name.as_str()).collect();
        out.push_str(&format!("    processes = [{}]\n", names.join(", ")));
    }
    out.push_str(");\n");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camel_case_of_container_names() {
        assert_eq!(camel_case("motor"), "Motor");
        assert_eq!(camel_case("motor_state"), "MotorState");
    }
}
