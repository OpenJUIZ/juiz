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
use crate::{
    ContainerSignature, DesignSkeleton, GeneratedFile, IdlError, ProcessSignature, Signatures, StateInit,
};

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
    design: bool,
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
            format!(".add_int_arg({name}, {desc}, {})", {
                let i = d.and_then(Value::as_i64).unwrap_or(0);
                // 設計文書の雛形は 32 bit に収まる値を INT64_C なしで書く (今の -t と同じ)
                if design && i32::try_from(i).is_ok() { i.to_string() } else { long_literal(i) }
            }),
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
        let (ty, line) = argument(p, a, false)?;
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

// ---- 設計文書から作る雛形 (docs/spec/design.md §6.2、設計文書 Step 3) ----

/// 4 つの空白で字下げする (空の行はそのまま)
fn indent(code: &str) -> String {
    code.trim_end_matches('\n')
        .lines()
        .map(|l| if l.is_empty() { "\n".to_owned() } else { format!("    {l}\n") })
        .collect()
}

/// 関数の本体。設計文書の本体の断片があればそれ、無ければ未実装の印
fn design_body(name: &str, used: &[String], design: &DesignSkeleton) -> String {
    if let Some(b) = design.bodies.get(name) {
        return indent(b);
    }
    let mut out: String = used.iter().map(|u| format!("    (void){u};\n")).collect();
    out.push_str(&format!(
        "    throw std::runtime_error({});\n",
        string_literal(&format!("write the body of {name}"))
    ));
    out
}

/// 設計文書の Process・ContainerProcess。`container` は (状態のクラス, Container の型名)
fn design_process(
    p: &ProcessSignature,
    container: Option<(&str, &str)>,
    design: &DesignSkeleton,
) -> Result<String, IdlError> {
    let name = plain_ident(&p.type_name, "process")?;
    let mut out = match container {
        None if p.arguments.is_empty() => format!("// Process {name}\n"),
        None => format!(
            "// Process {name}. The arguments are passed in the order they are added to the manifest\n"
        ),
        Some(_) if p.readonly => format!(
            "// ContainerProcess {name}. readonly() in the manifest and the const pointer make it readonly\n\
             // (juiz may run readonly container processes of the same container in parallel)\n"
        ),
        Some(_) => format!("// ContainerProcess {name}\n"),
    };
    let mut manif = vec![format!("static auto {name}_manif = ProcessManifest({})", string_literal(&name))];
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
        let constness = if p.readonly { "const " } else { "" };
        params.push(format!("{constness}{state}* {first}"));
        used.push(first);
    }
    for a in p.arguments.iter() {
        let an = plain_ident(&a.name, "argument")?;
        let (ty, line) = argument(p, a, true)?;
        manif.push(line);
        params.push(format!("{ty} {an}"));
        used.push(an);
    }
    let (ret, ret_line) = returns(p)?;
    manif.extend(ret_line);
    if p.readonly {
        manif.push(".readonly()".to_owned());
    }
    out.push_str(&manif.join("\n    "));
    out.push_str(";\n");
    out.push_str(&format!("std::optional<{ret}> {name}({}) {{\n", params.join(", ")));
    out.push_str(&design_body(&name, &used, design));
    out.push_str("}\n");
    match container {
        Some((state, _)) => out.push_str(&format!(
            "COMPONENT_CONTAINER_PROCESS_FACTORY({state}, {name}_manif, {name});\n"
        )),
        None => out.push_str(&format!("COMPONENT_PROCESS_FACTORY({name}_manif, {name});\n")),
    }
    Ok(out)
}

/// 整数・小数・真偽・文字列の値は、そのまま C++ のリテラルにする (`0`・`1.5`・`true`・`"a"`)
fn plain_literal(t: &DataType, v: &Value) -> Result<String, IdlError> {
    Ok(match (t, v) {
        (DataType::Int | DataType::Long, Value::Number(n)) => match n.as_i64() {
            Some(i) if i32::try_from(i).is_ok() => i.to_string(),
            _ => crate::cpp::default_initializer(t, v)?,
        },
        (DataType::Float, Value::Number(n)) => format!("{}f", double_literal(n.as_f64().unwrap_or(0.0))),
        (DataType::Double, Value::Number(n)) => double_literal(n.as_f64().unwrap_or(0.0)),
        (DataType::Boolean, Value::Bool(b)) => b.to_string(),
        (DataType::String, Value::String(s)) => format!("std::string({})", string_literal(s)),
        // 空の配列・辞書は {} (値の初期化)
        (DataType::Array(_), Value::Array(a)) if a.is_empty() => "{}".to_owned(),
        (DataType::Map(_), Value::Object(m)) if m.is_empty() => "{}".to_owned(),
        _ => crate::cpp::default_initializer(t, v)?,
    })
}

/// app.conf の Container の値 (args) から引数を読む式
fn container_argument(c: &ContainerSignature, a: &crate::ArgumentSignature) -> Result<(String, String), IdlError> {
    let t = schema_type(&a.schema)?;
    let n = string_literal(&a.name);
    let ty = crate::cpp::cpp_type(&t)?;
    let get = match &t {
        DataType::Boolean => format!("args.get_bool({n})"),
        DataType::Int => format!("int32_t(args.get_int({n}))"),
        DataType::Long => format!("args.get_int({n})"),
        DataType::Float => format!("float(args.get_float({n}))"),
        DataType::Double => format!("args.get_float({n})"),
        DataType::String => format!("args.get_string({n})"),
        DataType::Enum(_) => format!("juiz::type_traits<{ty}>::from_value(juiz::Value(args.get_string({n})))"),
        DataType::Array(_) => format!("juiz::type_traits<{ty}>::from_value(juiz::Value(args.get_array({n})))"),
        DataType::Record(_) | DataType::Map(_) => {
            format!("juiz::type_traits<{ty}>::from_value(juiz::Value(args.get_object({n})))")
        }
        _ => {
            return Err(generate_error(format!(
                "container {} argument {}: {} is not supported for a Container argument by the C++ skeleton (use boolean, int, long, float, double, string, array, map, record or enum)",
                c.type_name,
                a.name,
                t.display_name()
            )))
        }
    };
    let zero = crate::zero_value(&t);
    let default = plain_literal(&t, a.default.as_ref().unwrap_or(&zero))?;
    Ok((ty, format!("args.has({n}) ? {get} : {default}")))
}

/// 設計文書の Container: 状態のクラス (`<Name>`)、引数から状態を作る関数、ContainerProcess
fn design_container(c: &ContainerSignature, design: &DesignSkeleton) -> Result<String, IdlError> {
    let name = plain_ident(&c.type_name, "container")?;
    let class = design_state_name(&c.type_name);
    // 状態のクラス。コンストラクタは状態の初期値に使う引数を受け取る
    let mut ctor_params: Vec<(String, String)> = Vec::new();
    let mut inits = Vec::new();
    let mut fields = String::new();
    for s in c.state.iter() {
        let sn = plain_ident(&s.name, "state")?;
        let t = schema_type(&s.schema)?;
        let ty = crate::cpp::cpp_type(&t)?;
        let expr = match &s.init {
            StateInit::Argument(arg) => {
                let a = c.arguments.iter().find(|a| a.name == *arg).ok_or_else(|| {
                    generate_error(format!(
                        "the state {} of the container {name} is initialized by '{arg}', which is not an argument",
                        s.name
                    ))
                })?;
                if !ctor_params.iter().any(|(n, _)| n == arg) {
                    ctor_params.push((arg.clone(), crate::cpp::cpp_type(&schema_type(&a.schema)?)?));
                }
                arg.clone()
            }
            StateInit::Value(v) => plain_literal(&t, v)?,
        };
        inits.push(format!("{sn}({expr})"));
        fields.push_str(&format!("    {ty} {sn};"));
        match &s.description {
            Some(d) => fields.push_str(&format!("  // {}\n", d.lines().collect::<Vec<_>>().join(" "))),
            None => fields.push('\n'),
        }
    }
    // コンストラクタの引数の順は Container の引数の順
    ctor_params.sort_by_key(|(n, _)| c.arguments.iter().position(|a| a.name == *n));
    let mut out = format!("// State of the container {name}\nclass {class} {{\npublic:\n");
    if !c.state.is_empty() {
        let params: Vec<String> = ctor_params.iter().map(|(n, t)| format!("{t} {n}")).collect();
        // 状態が 1 つなら 1 行、2 つ以上なら初期化を 1 行ずつ
        if inits.len() == 1 {
            out.push_str(&format!("    explicit {class}({}) : {} {{}}\n", params.join(", "), inits[0]));
        } else {
            out.push_str(&format!(
                "    explicit {class}({})\n        : {} {{}}\n",
                params.join(", "),
                inits.join(",\n          ")
            ));
        }
    }
    out.push_str(&fields);
    out.push_str("};\n\n");
    // Container の関数
    let names: Vec<&str> = c.arguments.iter().map(|a| a.name.as_str()).collect();
    if names.is_empty() {
        out.push_str(&format!("// Container {name}\n"));
    } else {
        out.push_str(&format!(
            "// Container {name}. args holds the container entry of app/app.conf ({})\n",
            names.join(", ")
        ));
    }
    out.push_str(&format!("static auto {name}_manif = ContainerManifest({})", string_literal(&name)));
    if let Some(d) = &c.description {
        out.push_str(&format!("\n    .description({})", string_literal(d)));
    }
    out.push_str(&format!(";\n{class}* {name}(juiz::CapsuleMap args) {{\n"));
    if c.arguments.is_empty() {
        out.push_str("    (void)args;\n");
    }
    for a in c.arguments.iter() {
        let an = plain_ident(&a.name, "argument")?;
        let (ty, expr) = container_argument(c, a)?;
        out.push_str(&format!("    {ty} {an} = {expr};\n"));
    }
    for a in c.arguments.iter() {
        if !ctor_params.iter().any(|(n, _)| *n == a.name) {
            out.push_str(&format!("    (void){};\n", a.name));
        }
    }
    let ctor_args: Vec<&str> = ctor_params.iter().map(|(n, _)| n.as_str()).collect();
    out.push_str(&format!(
        "    return new {class}({});\n}}\nCOMPONENT_CONTAINER_FACTORY({name}_manif, {name});\n",
        ctor_args.join(", ")
    ));
    for p in c.container_processes.iter() {
        out.push('\n');
        out.push_str(&design_process(p, Some((&class, &name)), design)?);
    }
    Ok(out)
}

/// 設計文書の Container の状態のクラスの名前 (`my_robot` → `MyRobot`、design.md §5.2 の N4)
fn design_state_name(container: &str) -> String {
    camel_case(container)
}

/// 設計文書から作る雛形 (docs/spec/design.md §6.2)。[`cpp_skeleton_items`] と同じ並びで、ファイルの先頭の説明
/// (`design.header`)、Container の引数・状態 (状態のクラスは `<Name>`)、ContainerProcess の readonly
/// (`.readonly()` と const のポインタ)、本体の断片 (`design.bodies`)、Component の説明も出す。
/// `juiz pkg create -d`・`-t` が src/<name>.cpp に使う
pub fn cpp_design_items(
    s: &Signatures,
    component_name: &str,
    design: &DesignSkeleton,
) -> Result<String, IdlError> {
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
        define(&design_state_name(&c.type_name), format!("the state class of container {}", c.type_name))?;
        for p in c.container_processes.iter() {
            define(&p.type_name, format!("container_process {}", p.type_name))?;
        }
    }
    let mut body = String::new();
    for p in s.processes.iter() {
        body.push('\n');
        body.push_str(&design_process(p, None, design)?);
    }
    for c in s.containers.iter() {
        body.push('\n');
        body.push_str(&design_container(c, design)?);
    }
    body.push_str(&format!(
        "\n// The manifest of the whole Component (the types in this plugin)\nauto {component_name}_component_manifest() {{\n    return ComponentManifest({})",
        string_literal(component_name)
    ));
    if let Some(d) = &design.description {
        body.push_str(&format!("\n        .description({})", string_literal(d)));
    }
    for p in s.processes.iter() {
        body.push_str(&format!("\n        .add_process({}_manifest())", p.type_name));
    }
    for c in s.containers.iter() {
        body.push_str(&format!("\n        .add_container({}_manifest()", c.type_name));
        for p in c.container_processes.iter() {
            body.push_str(&format!("\n            .add_process({}_manifest())", p.type_name));
        }
        body.push(')');
    }
    body.push_str(&format!(
        ";\n}}\nDEFINE_COMPONENT_MANIFEST_ENTRY_POINT({component_name}_component_manifest)\n"
    ));
    // 使うものだけ #include する
    let mut includes = vec!["<cstdint>", "<optional>"];
    for (used, header) in [
        ("std::map<", "<map>"),
        ("std::runtime_error", "<stdexcept>"),
        ("std::string", "<string>"),
        ("std::vector<", "<vector>"),
    ] {
        if body.contains(used) {
            includes.push(header);
        }
    }
    includes.sort();
    let mut out = design.header.clone();
    for i in includes {
        out.push_str(&format!("#include {i}\n"));
    }
    out.push_str("\n#include \"juiz/juiz.h\"\n");
    if design.types {
        out.push_str("#include \"juiz_idl.h\"\n");
    }
    out.push_str(&body);
    Ok(out)
}
