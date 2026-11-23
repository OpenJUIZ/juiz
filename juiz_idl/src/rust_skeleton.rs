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
    ArgumentSignature, ContainerSignature, DesignSkeleton, GenerateOptions, GeneratedFile,
    IdlError, ProcessSignature, Signatures, StateInit,
};

/// 名前空間ごとの雛形
pub fn generate_rust_skeleton(signatures: &[Signatures]) -> Result<Vec<GeneratedFile>, IdlError> {
    generate_rust_skeleton_with(signatures, &GenerateOptions::default())
}

/// extern の名前空間 (仕様 §24.2) の型は `rust_path` の下の型を参照する
pub(crate) fn generate_rust_skeleton_with(
    signatures: &[Signatures],
    options: &GenerateOptions,
) -> Result<Vec<GeneratedFile>, IdlError> {
    signatures
        .iter()
        .filter(|s| !s.processes.is_empty() || !s.containers.is_empty())
        .map(|s| {
            Ok(GeneratedFile {
                path: format!("{}.skeleton.rs", s.namespace).into(),
                contents: skeleton(s, options)?,
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
pub(crate) fn camel_case(name: &str) -> String {
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

fn schema_type(schema: &TypeSchema, options: &GenerateOptions) -> Result<String, IdlError> {
    rust_type(&data_type(schema)?, "", options)
}

fn data_type(schema: &TypeSchema) -> Result<DataType, IdlError> {
    schema
        .data_type()
        .map_err(|e| generate_error(format!("{}: {e}", schema.name)))
}

/// Rust の文字列リテラル
fn str_lit(s: &str) -> String {
    format!("{s:?}")
}

/// 雛形の作り方
#[derive(Clone, Copy)]
enum Mode<'a> {
    /// .jidl の宣言から (`juiz idl -g rust-skeleton`、`juiz pkg create --idl`)
    Jidl,
    /// 設計文書から (`juiz pkg create -d`・`-t`、docs/spec/design.md §6.2)。
    /// 説明は属性にだけ書き、既定値は書けるものは属性の書き方で書き、関数は `fn` (pkg.md §6.1 の形)
    Design(&'a DesignSkeleton),
}

/// 属性の書き方 (`a = 0`) で書ける既定値 (数・真偽・文字列)
fn attribute_value(v: &Value) -> Option<String> {
    match v {
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::String(s) => Some(str_lit(s)),
        _ => None,
    }
}

/// マクロの属性の中身 (`typed = "all"` と description と arguments)
fn attributes(
    description: &Option<String>,
    arguments: &[ArgumentSignature],
    extra: &[String],
    mode: Mode,
) -> String {
    let mut lines: Vec<String> = extra.to_vec();
    lines.push("typed = \"all\"".to_owned());
    if let Some(d) = description {
        lines.push(format!("description = {}", str_lit(d)));
    }
    let defaults: serde_json::Map<String, Value> = arguments
        .iter()
        .filter_map(|a| a.default.clone().map(|d| (a.name.clone(), d)))
        .collect();
    let descriptions: Vec<&ArgumentSignature> = arguments
        .iter()
        .filter(|a| a.description.is_some())
        .collect();
    // 設計文書から作るときは、数・真偽・文字列の既定値を属性の書き方で書く (読みやすいので)
    let plain_defaults: Option<Vec<String>> = match mode {
        Mode::Design(_) => arguments
            .iter()
            .filter_map(|a| a.default.as_ref().map(|d| (a, d)))
            .map(|(a, d)| attribute_value(d).map(|v| format!("{} = {v}", a.name)))
            .collect(),
        Mode::Jidl => None,
    };
    if let (Some(plain), true) = (&plain_defaults, descriptions.is_empty()) {
        if !plain.is_empty() {
            lines.push(format!("arguments = {{ default = {{ {} }} }}", plain.join(" ")));
        }
    } else if !defaults.is_empty() || !descriptions.is_empty() {
        let mut args = vec!["arguments = {".to_owned()];
        match &plain_defaults {
            Some(plain) if !plain.is_empty() => {
                args.push(format!("    default = {{ {} }}", plain.join(" ")));
            }
            _ if !defaults.is_empty() => {
                // 既定値は JSON の文字列で渡す (属性の書き方では負の数・配列・null を書けないため)
                let json = serde_json::to_string(&Value::Object(defaults)).unwrap();
                args.push(format!("    default = {}", str_lit(&json)));
            }
            _ => {}
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

fn parameters(
    arguments: &[ArgumentSignature],
    options: &GenerateOptions,
) -> Result<Vec<String>, IdlError> {
    arguments
        .iter()
        .map(|a| {
            Ok(format!(
                "{}: {}",
                plain_ident(&a.name, "argument")?,
                schema_type(&a.schema, options)?
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

fn argument_names(arguments: &[ArgumentSignature]) -> Vec<String> {
    arguments.iter().map(|a| a.name.clone()).collect()
}

/// 関数の本体。設計文書の本体の断片があればそれ、無ければ未実装の印
fn body(name: &str, used: &[String], mode: Mode) -> String {
    if let Mode::Design(d) = mode {
        if let Some(b) = d.bodies.get(name) {
            return indent(b);
        }
    }
    format!(
        "{}    todo!({})\n",
        use_arguments(used),
        str_lit(&format!("write the body of {name}"))
    )
}

/// 4 つの空白で字下げする (空の行はそのまま)
fn indent(code: &str) -> String {
    code.trim_end_matches('\n')
        .lines()
        .map(|l| {
            if l.is_empty() {
                "\n".to_owned()
            } else {
                format!("    {l}\n")
            }
        })
        .collect()
}

fn fn_keyword(mode: Mode) -> &'static str {
    match mode {
        Mode::Jidl => "pub fn",
        Mode::Design(_) => "fn",
    }
}

fn doc(description: &Option<String>, mode: Mode) -> String {
    match mode {
        Mode::Jidl => doc_lines(description, ""),
        // 説明は属性の description に書く
        Mode::Design(_) => String::new(),
    }
}

fn process_fn(p: &ProcessSignature, options: &GenerateOptions, mode: Mode) -> Result<String, IdlError> {
    let name = plain_ident(&p.type_name, "process")?;
    let mut out = doc(&p.description, mode);
    out.push_str(&format!(
        "#[juiz_component_process{}]\n",
        attributes(&p.description, &p.arguments, &[], mode)
    ));
    let params = parameters(&p.arguments, options)?;
    out.push_str(&format!(
        "{} {name}({}) -> JuizResult<{}> {{\n{}}}\n",
        fn_keyword(mode),
        params.join(", "),
        schema_type(&p.output_schema, options)?,
        body(&name, &argument_names(&p.arguments), mode)
    ));
    Ok(out)
}

/// 状態の初期値の Rust の式 (docs/spec/design.md §3.3・§4.4)
fn value_expr(t: &DataType, v: &Value, options: &GenerateOptions) -> Result<String, IdlError> {
    Ok(match (t, v) {
        (DataType::Optional { .. }, Value::Null) => "None".to_owned(),
        (DataType::Optional { inner, .. }, v) => format!("Some({})", value_expr(inner, v, options)?),
        (DataType::Boolean, Value::Bool(b)) => b.to_string(),
        (DataType::Int | DataType::Long, Value::Number(n)) if n.is_i64() || n.is_u64() => n.to_string(),
        (DataType::Float | DataType::Double, Value::Number(n)) if n.as_f64().is_some() => {
            format!("{:?}", n.as_f64().unwrap())
        }
        (DataType::String, Value::String(s)) if s.is_empty() => "::std::string::String::new()".to_owned(),
        (DataType::String, Value::String(s)) => format!("{}.to_owned()", str_lit(s)),
        (DataType::Array(_), Value::Array(a)) if a.is_empty() => "::std::vec::Vec::new()".to_owned(),
        (DataType::Map(_), Value::Object(m)) if m.is_empty() => {
            "::std::collections::HashMap::new()".to_owned()
        }
        // そのほか (record・enum・bytes・any・中身のある配列と辞書) は辞書の表し方の値から読む
        _ => format!(
            "<{} as ::juiz_sdk::types::JuizType>::from_value(&::juiz_sdk::prelude::serde_json::from_str::<::juiz_sdk::prelude::Value>({}).expect(\"the initial value is JSON\"))?",
            rust_type(t, "", options)?,
            str_lit(&serde_json::to_string(v).unwrap())
        ),
    })
}

fn container_code(c: &ContainerSignature, options: &GenerateOptions, mode: Mode) -> Result<String, IdlError> {
    let name = plain_ident(&c.type_name, "container")?;
    let state = ident(&camel_case(&c.type_name));
    let mut out = doc(&c.description, mode);
    match mode {
        Mode::Jidl => out.push_str(&format!(
            "/// State of the container `{name}` (the JIDL does not type it: add the fields you need)\n\
             pub struct {state} {{}}\n\n\
             #[juiz_component_container]\n\
             pub fn {name}() -> JuizResult<Box<{state}>> {{\n    Ok(Box::new({state} {{}}))\n}}\n"
        )),
        Mode::Design(_) => out.push_str(&design_container(c, &name, &state, options, mode)?),
    }
    for p in c.container_processes.iter() {
        let pname = plain_ident(&p.type_name, "container_process")?;
        out.push('\n');
        out.push_str(&doc(&p.description, mode));
        if p.readonly && matches!(mode, Mode::Design(_)) {
            out.push_str(
                "/// Receiving the container as `&ContainerImpl` (not `&mut`) makes this ContainerProcess readonly.\n",
            );
        }
        out.push_str(&format!(
            "#[juiz_component_container_process{}]\n",
            attributes(
                &p.description,
                &p.arguments,
                &[format!("container_type = {}", str_lit(&name))],
                mode
            )
        ));
        let container = if p.readonly { "&" } else { "&mut " };
        let mut params = vec![format!("container: {container}ContainerImpl<{state}>")];
        params.extend(parameters(&p.arguments, options)?);
        let mut used = vec!["container".to_owned()];
        used.extend(argument_names(&p.arguments));
        out.push_str(&format!(
            "{} {pname}({}) -> JuizResult<{}> {{\n{}}}\n",
            fn_keyword(mode),
            params.join(", "),
            schema_type(&p.output_schema, options)?,
            body(&pname, &used, mode)
        ));
    }
    Ok(out)
}

/// 設計文書の Container: 状態の struct と、引数から状態を作る関数 (docs/spec/design.md §3.3)
fn design_container(
    c: &ContainerSignature,
    name: &str,
    state: &str,
    options: &GenerateOptions,
    mode: Mode,
) -> Result<String, IdlError> {
    let mut out = format!("/// State of the container `{name}`.\n#[repr(Rust)]\npub struct {state} {{");
    if c.state.is_empty() {
        out.push_str("}\n\n");
    } else {
        out.push('\n');
        for s in &c.state {
            out.push_str(&doc_lines(&s.description, "    "));
            out.push_str(&format!(
                "    pub {}: {},\n",
                plain_ident(&s.name, "state")?,
                schema_type(&s.schema, options)?
            ));
        }
        out.push_str("}\n\n");
    }
    out.push_str(&format!(
        "#[juiz_component_container{}]\n",
        attributes(&c.description, &c.arguments, &[], mode)
    ));
    let params = parameters(&c.arguments, options)?;
    // 状態の値の式。同じ引数を 2 回以上使うときは、最後のほかは clone する
    let mut fields = Vec::new();
    let uses = |arg: &str| {
        c.state
            .iter()
            .filter(|s| matches!(&s.init, StateInit::Argument(a) if a == arg))
            .count()
    };
    let mut used_so_far: Vec<String> = Vec::new();
    for s in &c.state {
        let t = data_type(&s.schema)?;
        let expr = match &s.init {
            StateInit::Argument(arg) => {
                let a = c.arguments.iter().find(|a| a.name == *arg).ok_or_else(|| {
                    generate_error(format!(
                        "the state {} of the container {name} is initialized by '{arg}', which is not an argument",
                        s.name
                    ))
                })?;
                let at = data_type(&a.schema)?;
                used_so_far.push(arg.clone());
                let n = used_so_far.iter().filter(|u| *u == arg).count();
                let v = if n < uses(arg) { format!("{arg}.clone()") } else { arg.clone() };
                if matches!(t, DataType::Optional { .. }) && !matches!(at, DataType::Optional { .. }) {
                    format!("Some({v})")
                } else {
                    v
                }
            }
            StateInit::Value(v) => value_expr(&t, v, options)?,
        };
        // 引数と同じ名前ならフィールドの初期化の省略形にする
        fields.push(if expr == s.name { expr } else { format!("{}: {expr}", s.name) });
    }
    let unused: Vec<String> = c
        .arguments
        .iter()
        .filter(|a| !used_so_far.contains(&a.name))
        .map(|a| a.name.clone())
        .collect();
    let value = if fields.is_empty() {
        format!("{state} {{}}")
    } else {
        format!(
            "{state} {{\n{}    }}",
            fields.iter().map(|f| format!("        {f},\n")).collect::<String>()
        )
    };
    out.push_str(&format!(
        "fn {name}({}) -> JuizResult<Box<{state}>> {{\n{}    Ok(Box::new({value}))\n}}\n",
        params.join(", "),
        use_arguments(&unused),
    ));
    Ok(out)
}

fn skeleton(s: &Signatures, options: &GenerateOptions) -> Result<String, IdlError> {
    let mut out = String::from(HEADER);
    out.push_str(&format!(
        "//\n\
         // 名前空間 {ns} の process / container 宣言から作った Rust の雛形。コピーして todo!() の本体を書く。\n\
         // 型は crate::… で参照するので、クレートの一番上で juiz_idl.rs を読み込んでおく:\n\
         //     include!(concat!(env!(\"OUT_DIR\"), \"/juiz_idl.rs\"));\n\
         // 1 つの名前空間が 1 つのコンポーネント (cdylib) になる。\n\n",
        ns = s.namespace
    ));
    out.push_str(&rust_skeleton_items(
        s,
        &s.namespace.replace('.', "_"),
        options,
    )?);
    Ok(out)
}

/// 雛形の中身 (`use juiz_sdk::prelude::*;`、宣言ごとの関数、`juiz_component_manifest!`)。
/// ファイルの先頭の説明は付けない。`juiz pkg create --idl` (docs/spec/pkg.md §6.5) が
/// コンポーネントの名前を `component_name` にして src/lib.rs に埋め込むのに使う
pub fn rust_skeleton_items(
    s: &Signatures,
    component_name: &str,
    options: &GenerateOptions,
) -> Result<String, IdlError> {
    items(s, component_name, options, Mode::Jidl)
}

/// 設計文書から作る雛形の中身 (docs/spec/design.md §6.2)。[`rust_skeleton_items`] と同じ並びで、
/// Container の引数・状態 (`Signatures` の `arguments`・`state`)、ContainerProcess の readonly
/// (`&ContainerImpl` で受ける)、本体の断片 (`design.bodies`) と最後のコード (`design.tail`) も出す。
/// `juiz pkg create -d`・`-t` が src/lib.rs に使う
pub fn rust_design_items(
    s: &Signatures,
    component_name: &str,
    options: &GenerateOptions,
    design: &DesignSkeleton,
) -> Result<String, IdlError> {
    items(s, component_name, options, Mode::Design(design))
}

fn items(
    s: &Signatures,
    component_name: &str,
    options: &GenerateOptions,
    mode: Mode,
) -> Result<String, IdlError> {
    let mut out = match mode {
        Mode::Jidl => String::from("#[allow(unused_imports)]\nuse juiz_sdk::prelude::*;\n"),
        Mode::Design(_) => String::from("use juiz_sdk::prelude::*;\n"),
    };
    for p in s.processes.iter() {
        out.push('\n');
        out.push_str(&process_fn(p, options, mode)?);
    }
    for c in s.containers.iter() {
        out.push('\n');
        out.push_str(&container_code(c, options, mode)?);
    }
    // コンポーネントのマニフェスト (設計文書では pkg.md §6.1 と同じ [ a, b ] の書き方)
    let list = |names: Vec<&str>| match mode {
        Mode::Jidl => format!("[{}]", names.join(", ")),
        Mode::Design(_) if names.is_empty() => "[]".to_owned(),
        Mode::Design(_) => format!("[ {} ]", names.join(", ")),
    };
    out.push_str(&format!(
        "\njuiz_component_manifest!(\n    component_name = {}\n",
        str_lit(component_name)
    ));
    if !s.containers.is_empty() {
        out.push_str("    containers = {\n");
        for c in s.containers.iter() {
            let names: Vec<&str> = c
                .container_processes
                .iter()
                .map(|p| p.type_name.as_str())
                .collect();
            out.push_str(&format!("        {} = {}\n", c.type_name, list(names)));
        }
        out.push_str("    }\n");
    }
    if !s.processes.is_empty() {
        let names: Vec<&str> = s.processes.iter().map(|p| p.type_name.as_str()).collect();
        out.push_str(&format!("    processes = {}\n", list(names)));
    }
    out.push_str(");\n");
    if let Mode::Design(DesignSkeleton { tail: Some(t), .. }) = mode {
        out.push('\n');
        out.push_str(t);
        if !t.ends_with('\n') {
            out.push('\n');
        }
    }
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
