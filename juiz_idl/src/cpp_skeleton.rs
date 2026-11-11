//! `process` / `container` 宣言から C++ の雛形 (skeleton) を書き出す (仕様 §21.6、issue #468)
//!
//! 名前空間ごとに `<namespace>.skeleton.cpp` を作る。中身は 1 つのコンポーネント (プラグイン) で、
//! 宣言ごとに cppjuiz のマニフェスト・関数 (本体は例外を投げる)・`COMPONENT_*_FACTORY` と、
//! 最後に Component のマニフェストを持つ。record・enum の型は `juiz-idl -g cpp` のヘッダ (`juiz_idl.h`) を使う。
//!
//! cppjuiz の関数が受け取れる型だけを使う (§21.2)。`T?`・`bytes` の引数はエラーにする。

use std::collections::BTreeMap;

use juiz_types::{DataType, TypeSchema};
use serde_json::Value;

use crate::cpp::{double_literal, long_literal, named_path, string_literal, value_literal, KEYWORDS};
use crate::rust_skeleton::camel_case;
use crate::{ContainerSignature, GeneratedFile, IdlError, ProcessSignature, Signatures};

fn generate_error(message: String) -> IdlError {
    IdlError::Generate { message }
}

/// 名前空間ごとの雛形
pub fn generate_cpp_skeleton(signatures: &[Signatures]) -> Result<Vec<GeneratedFile>, IdlError> {
    signatures
        .iter()
        .filter(|s| !s.processes.is_empty() || !s.containers.is_empty())
        .map(|s| {
            let mut contents = String::from(crate::cpp::HEADER);
            contents.push_str(&format!(
                "//\n\
                 // 名前空間 {ns} の process / container 宣言から作った C++ の雛形。コピーして throw の行の本体を書く。\n\
                 // 型は juiz-idl -g cpp のヘッダ juiz_idl.h を使う (CMake の juiz_generate_types() が作る)。\n\
                 // 1 つの名前空間が 1 つのコンポーネント (プラグイン) になる。\n\n",
                ns = s.namespace
            ));
            contents.push_str(&cpp_skeleton_items(s, &s.namespace.replace('.', "_"))?);
            Ok(GeneratedFile {
                path: format!("{}.skeleton.cpp", s.namespace).into(),
                contents,
            })
        })
        .collect()
}

fn schema_type(schema: &TypeSchema) -> Result<DataType, IdlError> {
    schema
        .data_type()
        .map_err(|e| generate_error(format!("{}: {e}", schema.name)))
}

fn plain_ident(name: &str, what: &str) -> Result<String, IdlError> {
    if KEYWORDS.contains(&name) {
        return Err(generate_error(format!(
            "{what} '{name}' is a C++ keyword and cannot be used in the C++ skeleton"
        )));
    }
    Ok(name.to_owned())
}

fn unsupported(where_: &str, t: &DataType) -> IdlError {
    generate_error(format!(
        "{where_}: {} is not supported by the C++ skeleton (cppjuiz functions take boolean, int, long, float, double, string, array, map, any, record, enum and image)",
        t.display_name()
    ))
}

/// 1 つの引数: (関数の引数の型, マニフェストに足す行)
fn argument(
    p: &ProcessSignature,
    a: &crate::ArgumentSignature,
) -> Result<(String, String), IdlError> {
    let t = schema_type(&a.schema)?;
    let name = string_literal(&a.name);
    let desc = string_literal(a.description.as_deref().unwrap_or(""));
    let d = a.default.as_ref();
    Ok(match &t {
        DataType::Boolean => (
            "bool".to_owned(),
            format!(".add_bool_arg({name}, {desc}, {})", d.and_then(Value::as_bool).unwrap_or(false)),
        ),
        DataType::Int | DataType::Long => (
            if matches!(t, DataType::Int) { "int32_t" } else { "int64_t" }.to_owned(),
            format!(".add_int_arg({name}, {desc}, {})", long_literal(d.and_then(Value::as_i64).unwrap_or(0))),
        ),
        DataType::Float | DataType::Double => (
            if matches!(t, DataType::Float) { "float" } else { "double" }.to_owned(),
            format!(".add_float_arg({name}, {desc}, {})", double_literal(d.and_then(Value::as_f64).unwrap_or(0.0))),
        ),
        DataType::String => (
            "const std::string&".to_owned(),
            format!(".add_string_arg({name}, {desc}, {})", string_literal(d.and_then(Value::as_str).unwrap_or(""))),
        ),
        DataType::Array(_) => (
            "const std::vector<juiz::Value>&".to_owned(),
            format!(".add_array_arg({name}, {desc})"),
        ),
        DataType::Map(_) | DataType::Any => (
            "const juiz::Value&".to_owned(),
            format!(".add_object_arg({name}, {desc})"),
        ),
        DataType::Image => (
            "const juiz::Image&".to_owned(),
            format!(".add_image_arg({name}, {desc})"),
        ),
        DataType::Record(_) | DataType::Enum(_) => {
            let path = named_path(&t)?;
            let ty = if matches!(t, DataType::Record(_)) {
                format!("const {path}&")
            } else {
                path.clone()
            };
            let line = match d {
                Some(v) => format!(".add_typed_arg_value<{path}>({name}, {desc}, {})", value_literal(v)),
                None => format!(".add_typed_arg<{path}>({name}, {desc})"),
            };
            (ty, line)
        }
        DataType::Null | DataType::Bytes | DataType::Optional { .. } => {
            return Err(unsupported(&format!("{} argument {}", p.type_name, a.name), &t))
        }
    })
}

/// 戻り値: (std::optional の中の型, マニフェストに足す行)
fn returns(p: &ProcessSignature) -> Result<(String, Option<String>), IdlError> {
    let t = schema_type(&p.output_schema)?;
    Ok(match &t {
        DataType::Boolean => ("bool".to_owned(), None),
        DataType::Int => ("int32_t".to_owned(), None),
        DataType::Long => ("int64_t".to_owned(), None),
        DataType::Float => ("float".to_owned(), None),
        DataType::Double => ("double".to_owned(), None),
        DataType::String => ("std::string".to_owned(), None),
        // void は null を返す (juiz::Value())
        DataType::Null | DataType::Array(_) | DataType::Map(_) | DataType::Any => {
            ("juiz::Value".to_owned(), None)
        }
        DataType::Image => ("juiz::Image".to_owned(), None),
        DataType::Record(_) | DataType::Enum(_) => {
            let path = named_path(&t)?;
            (path.clone(), Some(format!(".returns<{path}>()")))
        }
        DataType::Bytes | DataType::Optional { .. } => {
            return Err(unsupported(&format!("{} return value", p.type_name), &t))
        }
    })
}

/// コメントの行
fn comment(doc: &Option<String>) -> String {
    match doc {
        Some(d) => d
            .lines()
            .map(|l| format!("//{}{l}\n", if l.is_empty() { "" } else { " " }))
            .collect(),
        None => String::new(),
    }
}

/// マニフェスト・関数・FACTORY。`container` は (状態の型, Container の型名)
fn process_code(p: &ProcessSignature, container: Option<(&str, &str)>) -> Result<String, IdlError> {
    let name = plain_ident(&p.type_name, "process")?;
    let mut manif = vec![format!("static auto {name}_manif = ProcessManifest({})", string_literal(&name))];
    if let Some((_, ct)) = container {
        manif.push(format!(".container_type({})", string_literal(ct)));
    }
    if let Some(d) = &p.description {
        manif.push(format!(".description({})", string_literal(d)));
    }
    let mut params = Vec::new();
    let mut used = Vec::new();
    if let Some((state, _)) = container {
        let mut first = "container".to_owned();
        while p.arguments.iter().any(|a| a.name == first) {
            first.push('_');
        }
        params.push(format!("{state}* {first}"));
        used.push(first);
    }
    for a in p.arguments.iter() {
        let an = plain_ident(&a.name, "argument")?;
        let (ty, line) = argument(p, a)?;
        manif.push(line);
        params.push(format!("{ty} {an}"));
        used.push(an);
    }
    let (ret, ret_line) = returns(p)?;
    manif.extend(ret_line);
    let mut out = comment(&p.description);
    out.push_str(&manif.join("\n    "));
    out.push_str(";\n");
    out.push_str(&format!("std::optional<{ret}> {name}({}) {{\n", params.join(", ")));
    for u in used.iter() {
        out.push_str(&format!("    (void){u};\n"));
    }
    out.push_str(&format!(
        "    throw std::runtime_error({});\n}}\n",
        string_literal(&format!("write the body of {name}"))
    ));
    match container {
        Some((state, _)) => out.push_str(&format!(
            "COMPONENT_CONTAINER_PROCESS_FACTORY({state}, {name}_manif, {name});\n"
        )),
        None => out.push_str(&format!("COMPONENT_PROCESS_FACTORY({name}_manif, {name});\n")),
    }
    Ok(out)
}

/// Container の状態の型の名前 (`arm` → `ArmState`)
fn state_name(container: &str) -> String {
    format!("{}State", camel_case(container))
}

fn container_code(c: &ContainerSignature) -> Result<String, IdlError> {
    let name = plain_ident(&c.type_name, "container")?;
    let state = state_name(&c.type_name);
    let mut out = format!(
        "// State of the container {name} (the JIDL does not type it: add the members you need)\n\
         struct {state} {{\n}};\n\n"
    );
    out.push_str(&comment(&c.description));
    out.push_str(&format!("static auto {name}_manif = ContainerManifest({})", string_literal(&name)));
    if let Some(d) = &c.description {
        out.push_str(&format!("\n    .description({})", string_literal(d)));
    }
    out.push_str(&format!(
        ";\n{state}* {name}(juiz::CapsuleMap args) {{\n    (void)args;\n    return new {state}();\n}}\n\
         COMPONENT_CONTAINER_FACTORY({name}_manif, {name});\n"
    ));
    for p in c.container_processes.iter() {
        out.push('\n');
        out.push_str(&process_code(p, Some((&state, &name)))?);
    }
    Ok(out)
}

/// 雛形の中身 (#include、宣言ごとのマニフェストと関数、Component のマニフェスト)。
/// ファイルの先頭の説明は付けない。`juiz pkg create --idl` (docs/spec/pkg.md §6.5) が
/// コンポーネントの名前を `component_name` にして src/<name>.cpp に使う
pub fn cpp_skeleton_items(s: &Signatures, component_name: &str) -> Result<String, IdlError> {
    // マクロが作る名前 (<name>_manif・<name>_manifest など) が重ならないように
    let mut defined: BTreeMap<String, String> = BTreeMap::new();
    let mut define = |n: &str, what: String| -> Result<(), IdlError> {
        if let Some(other) = defined.insert(n.to_owned(), what.clone()) {
            return Err(generate_error(format!(
                "{other} and {what} both become the C++ name {n}"
            )));
        }
        Ok(())
    };
    for p in s.processes.iter() {
        define(&p.type_name, format!("process {}", p.type_name))?;
    }
    for c in s.containers.iter() {
        define(&c.type_name, format!("container {}", c.type_name))?;
        define(&state_name(&c.type_name), format!("the state class of container {}", c.type_name))?;
        for p in c.container_processes.iter() {
            define(&p.type_name, format!("container_process {}", p.type_name))?;
        }
    }
    let mut out = String::from(
        "#include <cstdint>\n#include <optional>\n#include <stdexcept>\n#include <string>\n#include <vector>\n\n\
         #include \"juiz/juiz.h\"\n#include \"juiz_idl.h\"\n",
    );
    for p in s.processes.iter() {
        out.push('\n');
        out.push_str(&process_code(p, None)?);
    }
    for c in s.containers.iter() {
        out.push('\n');
        out.push_str(&container_code(c)?);
    }
    out.push_str(&format!(
        "\n// The manifest of the whole Component (the types in this plugin)\nauto {component_name}_component_manifest() {{\n    return ComponentManifest({})",
        string_literal(component_name)
    ));
    for p in s.processes.iter() {
        out.push_str(&format!("\n        .add_process({}_manifest())", p.type_name));
    }
    for c in s.containers.iter() {
        out.push_str(&format!("\n        .add_container({}_manifest()", c.type_name));
        for p in c.container_processes.iter() {
            out.push_str(&format!("\n            .add_process({}_manifest())", p.type_name));
        }
        out.push(')');
    }
    out.push_str(&format!(
        ";\n}}\nDEFINE_COMPONENT_MANIFEST_ENTRY_POINT({component_name}_component_manifest)\n"
    ));
    Ok(out)
}
