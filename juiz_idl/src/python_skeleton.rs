//! `process` / `container` 宣言から Python の雛形 (skeleton) を書き出す (仕様 §20.6、issue #468)
//!
//! 名前空間ごとに `<namespace>.skeleton.py` を作る。中身は pyjuiz のデコレータを付けた関数
//! (本体は `raise NotImplementedError`) で、引数・戻り値に型注釈を付ける。record・enum の型は
//! `juiz-idl -g python` が作るパッケージから import する (出力先を `sys.path` に入れておく)。
//!
//! pyjuiz のデコレータが受け付ける型注釈だけを使う (§20.3)。`bool`・`int`・`float`・`str`・`list`・`dict`・
//! 生成した record / enum・`PIL.Image.Image`、戻り値の `None` (`void`)。`T?`・`bytes` はエラーにする。

use std::collections::{BTreeMap, BTreeSet};

use juiz_types::{DataType, TypeSchema};
use serde_json::Value;

use crate::python::{docstring, literal, namespace_alias, str_lit, KEYWORDS};
use crate::rust_skeleton::camel_case;
use crate::{
    ArgumentSignature, ContainerSignature, DesignSkeleton, GeneratedFile, IdlError,
    ProcessSignature, Signatures, StateInit,
};

/// デコレータなど、雛形が自分で使う名前 (関数名・型名に使えない)
const USED: [&str; 5] = [
    "juiz_process",
    "juiz_container",
    "juiz_container_process",
    "Image",
    "NotImplementedError",
];

fn generate_error(message: String) -> IdlError {
    IdlError::Generate { message }
}

/// 名前空間ごとの雛形
pub fn generate_python_skeleton(signatures: &[Signatures]) -> Result<Vec<GeneratedFile>, IdlError> {
    signatures
        .iter()
        .filter(|s| !s.processes.is_empty() || !s.containers.is_empty())
        .map(|s| {
            let mut contents = String::from(crate::python::HEADER);
            contents.push_str(&format!(
                "#\n\
                 # 名前空間 {ns} の process / container 宣言から作った Python の雛形。コピーして NotImplementedError の本体を書く。\n\
                 # record・enum の型は juiz-idl -g python が作るパッケージ (出力先を sys.path に入れる) から読む。\n\
                 # 1 つの名前空間が 1 つのコンポーネントのファイルになる。\n\n",
                ns = s.namespace
            ));
            contents.push_str(&python_skeleton_items(s)?);
            Ok(GeneratedFile {
                path: format!("{}.skeleton.py", s.namespace).into(),
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

fn named(t: &DataType) -> Option<(&str, &str)> {
    match t {
        DataType::Record(r) => Some((&r.namespace, &r.name)),
        DataType::Enum(e) => Some((&e.namespace, &e.name)),
        _ => None,
    }
}

/// 雛形の中の型の名前。ほかの名前とぶつからなければ `from a.b import T`、ぶつかれば `import a.b as _ns_a_b`
struct TypeNames {
    /// (名前空間, 型名) → 書き方
    refs: BTreeMap<(String, String), String>,
    from_imports: BTreeMap<String, BTreeSet<String>>,
    module_imports: BTreeSet<String>,
}

impl TypeNames {
    fn new(types: &[(String, String)], taken: &BTreeSet<String>) -> Result<Self, IdlError> {
        let mut by_name: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for (ns, name) in types.iter() {
            if KEYWORDS.contains(&name.as_str()) || KEYWORDS.contains(&ns.split('.').next().unwrap_or("")) {
                return Err(generate_error(format!(
                    "'{ns}.{name}' cannot be used in Python (a keyword)"
                )));
            }
            by_name.entry(name).or_default().insert(ns);
        }
        let mut s = Self {
            refs: BTreeMap::new(),
            from_imports: BTreeMap::new(),
            module_imports: BTreeSet::new(),
        };
        for (ns, name) in types.iter() {
            let plain = by_name[name.as_str()].len() == 1 && !taken.contains(name) && !name.starts_with('_');
            let r = if plain {
                s.from_imports.entry(ns.clone()).or_default().insert(name.clone());
                name.clone()
            } else {
                s.module_imports.insert(ns.clone());
                format!("{}.{name}", namespace_alias(ns))
            };
            s.refs.insert((ns.clone(), name.clone()), r);
        }
        Ok(s)
    }

    fn get(&self, t: &DataType) -> String {
        let (ns, name) = named(t).unwrap();
        self.refs[&(ns.to_owned(), name.to_owned())].clone()
    }

    fn imports(&self) -> String {
        let mut out = String::new();
        for (ns, names) in self.from_imports.iter() {
            out.push_str(&format!(
                "from {ns} import {}\n",
                names.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        for ns in self.module_imports.iter() {
            out.push_str(&format!("import {ns} as {}\n", namespace_alias(ns)));
        }
        out
    }
}

fn unsupported(where_: &str, t: &DataType) -> IdlError {
    generate_error(format!(
        "{where_}: {} is not supported by the Python skeleton (the pyjuiz decorators take bool, int, float, str, list, dict, record, enum and image)",
        t.display_name()
    ))
}

/// 型注釈。`where_` はエラーの文に使う
fn annotation(t: &DataType, names: &TypeNames, where_: &str) -> Result<String, IdlError> {
    Ok(match t {
        DataType::Boolean => "bool".to_owned(),
        DataType::Int | DataType::Long => "int".to_owned(),
        DataType::Float | DataType::Double => "float".to_owned(),
        DataType::String => "str".to_owned(),
        DataType::Array(_) => "list".to_owned(),
        DataType::Map(_) | DataType::Any => "dict".to_owned(),
        DataType::Record(_) | DataType::Enum(_) => names.get(t),
        DataType::Image => "Image".to_owned(),
        DataType::Null | DataType::Bytes | DataType::Optional { .. } => {
            return Err(unsupported(where_, t))
        }
    })
}

/// 既定値の Python の式
fn default_expr(t: &DataType, v: &Value, names: &TypeNames) -> String {
    match (t, v) {
        (DataType::Float | DataType::Double, Value::Number(n)) => match n.as_f64() {
            Some(f) => format!("{f:?}"),
            None => literal(v),
        },
        (DataType::Record(_) | DataType::Enum(_), _) => {
            format!("{}.from_value({})", names.get(t), literal(v))
        }
        _ => literal(v),
    }
}

/// 型なしの引数で、既定値を書かないときに pyjuiz が使う値
fn zero_value(t: &DataType) -> Option<&'static str> {
    Some(match t {
        DataType::Boolean => "False",
        DataType::Int | DataType::Long => "0",
        DataType::Float | DataType::Double => "0.0",
        DataType::String => "\"\"",
        DataType::Array(_) => "[]",
        DataType::Map(_) | DataType::Any => "{}",
        DataType::Image => "None",
        _ => return None,
    })
}

fn plain_ident(name: &str, what: &str) -> Result<String, IdlError> {
    if KEYWORDS.contains(&name) {
        return Err(generate_error(format!(
            "{what} '{name}' is a Python keyword and cannot be used in the Python skeleton"
        )));
    }
    Ok(name.to_owned())
}

/// 引数の並び (`first` は ContainerProcess の Container の引数)
fn parameters(
    type_name: &str,
    arguments: &[ArgumentSignature],
    first: Option<String>,
    names: &TypeNames,
) -> Result<Vec<String>, IdlError> {
    let mut out: Vec<String> = first.into_iter().collect();
    let mut defaulted: Option<&str> = None;
    for a in arguments.iter() {
        let name = plain_ident(&a.name, "argument")?;
        let t = schema_type(&a.schema)?;
        let ann = annotation(&t, names, &format!("{type_name} argument {}", a.name))?;
        match &a.default {
            Some(d) if !matches!(t, DataType::Image) => {
                defaulted = Some(&a.name);
                out.push(format!("{name}: {ann} = {}", default_expr(&t, d, names)));
            }
            _ => {
                // 型なしの引数は、既定値を書かなくても pyjuiz が型の 0 にする (§20.3)。
                // 既定値のある引数の後ろでは、その 0 を書く (Python の文法のため。manifest は同じ)
                if let (Some(_), Some(zero)) = (defaulted, zero_value(&t)) {
                    out.push(format!("{name}: {ann} = {zero}"));
                    continue;
                }
                if let Some(prev) = defaulted {
                    return Err(generate_error(format!(
                        "{type_name}: argument {} has no default but comes after {prev}, which has one; a Python function cannot have it (give {} a default or move it before {prev})",
                        a.name, a.name
                    )));
                }
                out.push(format!("{name}: {ann}"));
            }
        }
    }
    Ok(out)
}

fn returns(p: &ProcessSignature, names: &TypeNames) -> Result<String, IdlError> {
    let t = schema_type(&p.output_schema)?;
    if matches!(t, DataType::Null) {
        // void (pyjuiz は -> None を出力なしとして扱う)
        return Ok("None".to_owned());
    }
    annotation(&t, names, &format!("{} return value", p.type_name))
}

fn function(
    decorator: &str,
    p: &ProcessSignature,
    first: Option<String>,
    names: &TypeNames,
    mode: Mode,
) -> Result<String, IdlError> {
    let name = plain_ident(&p.type_name, "process")?;
    let params = parameters(&p.type_name, &p.arguments, first, names)?;
    let mut out = format!(
        "{decorator}\ndef {name}({}) -> {}:\n",
        params.join(", "),
        returns(p, names)?
    );
    out.push_str(&docstring(&p.description, "    "));
    out.push_str(&body(&name, mode));
    Ok(out)
}

/// 関数の本体。設計文書の本体の断片があればそれ、無ければ未実装の印
fn body(name: &str, mode: Mode) -> String {
    if let Mode::Design(d) = mode {
        if let Some(b) = d.bodies.get(name) {
            return b
                .trim_end_matches('\n')
                .lines()
                .map(|l| if l.is_empty() { "\n".to_owned() } else { format!("    {l}\n") })
                .collect();
        }
    }
    format!(
        "    raise NotImplementedError({})\n",
        str_lit(&format!("write the body of {name}"))
    )
}

/// 雛形の作り方
#[derive(Clone, Copy)]
enum Mode<'a> {
    /// .jidl の宣言から (`juiz idl -g python-skeleton`、`juiz pkg create --idl`)
    Jidl,
    /// 設計文書から (`juiz pkg create -d`・`-t`、docs/spec/design.md §6.2)。
    /// 状態のクラスは `<Name>` (pkg.md §6.2・§2.4 と同じ) で、引数から状態を作る
    Design(&'a DesignSkeleton),
}

/// Container の状態のクラスの名前 (`arm` → `ArmState`)
pub(crate) fn state_name(container: &str) -> String {
    format!("{}State", camel_case(container))
}

/// 雛形の中の状態のクラスの名前 (設計文書からは `<Name>`、.jidl からは `<Name>State`)
fn state_class(container: &str, mode: Mode) -> String {
    match mode {
        Mode::Jidl => state_name(container),
        Mode::Design(_) => camel_case(container),
    }
}

/// 状態の値の型注釈 (デコレータが読まないので、pyjuiz が受けない型も書ける)
fn state_annotation(t: &DataType, names: &TypeNames) -> String {
    match t {
        DataType::Bytes => "bytes".to_owned(),
        DataType::Any => "object".to_owned(),
        DataType::Optional { inner, .. } => format!("{} | None", state_annotation(inner, names)),
        DataType::Array(i) => format!("list[{}]", state_annotation(i, names)),
        DataType::Map(i) => format!("dict[str, {}]", state_annotation(i, names)),
        t => annotation(t, names, "").unwrap_or_else(|_| "object".to_owned()),
    }
}

/// 状態の初期値の Python の式 (docs/spec/design.md §3.3・§4.4)
fn state_value(t: &DataType, v: &Value, names: &TypeNames) -> String {
    match (t, v) {
        (DataType::Optional { .. }, Value::Null) => "None".to_owned(),
        (DataType::Optional { inner, .. }, v) => state_value(inner, v, names),
        (DataType::Bytes, Value::String(s)) if s.is_empty() => "b\"\"".to_owned(),
        (DataType::Bytes, Value::String(s)) => format!("base64.b64decode({})", str_lit(s)),
        _ => default_expr(t, v, names),
    }
}

fn container_code(c: &ContainerSignature, names: &TypeNames, mode: Mode) -> Result<String, IdlError> {
    let name = plain_ident(&c.type_name, "container")?;
    let state = state_class(&c.type_name, mode);
    let mut out = match mode {
        Mode::Jidl => {
            let mut out = format!(
                "class {state}:\n    \"\"\"State of the container {name} (the JIDL does not type it: add the attributes you need).\"\"\"\n\n\n\
                 @juiz_container\ndef {name}() -> {state}:\n"
            );
            out.push_str(&docstring(&c.description, "    "));
            out.push_str(&format!("    return {state}()\n"));
            out
        }
        Mode::Design(_) => design_container(c, &name, &state, names)?,
    };
    for p in c.container_processes.iter() {
        // 引数と同じ名前なら Container の引数の名前を変える
        let mut first = "container".to_owned();
        while p.arguments.iter().any(|a| a.name == first) {
            first.push('_');
        }
        let readonly = if p.readonly { ", readonly=True" } else { "" };
        out.push_str("\n\n");
        out.push_str(&function(
            &format!("@juiz_container_process(container_type={}{readonly})", str_lit(&name)),
            p,
            Some(format!("{first}: {state}")),
            names,
            mode,
        )?);
    }
    Ok(out)
}

/// 設計文書の Container: 状態のクラスと、引数から状態を作る関数 (docs/spec/design.md §3.3)
fn design_container(
    c: &ContainerSignature,
    name: &str,
    state: &str,
    names: &TypeNames,
) -> Result<String, IdlError> {
    let mut out = format!("class {state}:\n    \"\"\"State of the container {name}.\"\"\"\n");
    if !c.state.is_empty() {
        let mut params = Vec::new();
        let mut body = String::new();
        for s in &c.state {
            let n = plain_ident(&s.name, "state")?;
            let t = schema_type(&s.schema)?;
            params.push(format!("{n}: {}", state_annotation(&t, names)));
            let comment = match &s.description {
                Some(d) if !d.trim().is_empty() => {
                    format!("  # {}", d.split_whitespace().collect::<Vec<_>>().join(" "))
                }
                _ => String::new(),
            };
            body.push_str(&format!("        self.{n} = {n}{comment}\n"));
        }
        out.push_str(&format!("\n    def __init__(self, {}):\n{body}", params.join(", ")));
    }
    let params = parameters(name, &c.arguments, None, names)?;
    out.push_str(&format!(
        "\n\n@juiz_container\ndef {name}({}) -> {state}:\n",
        params.join(", ")
    ));
    out.push_str(&docstring(&c.description, "    "));
    let mut values = Vec::new();
    for s in &c.state {
        let t = schema_type(&s.schema)?;
        values.push(match &s.init {
            StateInit::Argument(a) => plain_ident(a, "argument")?,
            StateInit::Value(v) => state_value(&t, v, names),
        });
    }
    out.push_str(&format!("    return {state}({})\n", values.join(", ")));
    Ok(out)
}

/// 宣言の引数・戻り値に出てくる名前付き型
fn collect_named(t: &DataType, out: &mut BTreeSet<(String, String)>) {
    if let Some((ns, name)) = named(t) {
        out.insert((ns.to_owned(), name.to_owned()));
    }
}

/// 状態の型に出てくる名前付き型 (配列・辞書・T? の中も。型注釈に書くので)
fn collect_all_named(t: &DataType, out: &mut BTreeSet<(String, String)>) {
    match t {
        DataType::Array(i) | DataType::Map(i) => collect_all_named(i, out),
        DataType::Optional { inner, .. } => collect_all_named(inner, out),
        t => collect_named(t, out),
    }
}

/// 雛形の中身 (import、宣言ごとの関数)。ファイルの先頭の説明は付けない。
/// `juiz pkg create --idl` (docs/spec/pkg.md §6.5) が app/python/<name>.py に使う
pub fn python_skeleton_items(s: &Signatures) -> Result<String, IdlError> {
    items(s, Mode::Jidl)
}

/// 設計文書から作る雛形の中身 (docs/spec/design.md §6.2)。[`python_skeleton_items`] と同じ並びで、
/// Container の引数・状態 (状態のクラスは `<Name>`)、ContainerProcess の `readonly=True`、
/// 本体の断片 (`design.bodies`) と最後のコード (`design.tail`) も出す。`juiz pkg create -d`・`-t` が使う
pub fn python_design_items(s: &Signatures, design: &DesignSkeleton) -> Result<String, IdlError> {
    items(s, Mode::Design(design))
}

fn items(s: &Signatures, mode: Mode) -> Result<String, IdlError> {
    // 関数・クラスの名前 (重なれば Python では後のものが前のものを隠すのでエラー)
    let mut defined: BTreeMap<String, String> = BTreeMap::new();
    let mut define = |n: String, what: String| -> Result<(), IdlError> {
        if USED.contains(&n.as_str()) || (n == "base64" && matches!(mode, Mode::Design(_))) {
            return Err(generate_error(format!(
                "{what} becomes the Python name {n}, which the skeleton uses itself"
            )));
        }
        if let Some(other) = defined.insert(n.clone(), what.clone()) {
            return Err(generate_error(format!(
                "{other} and {what} both become the Python name {n}"
            )));
        }
        Ok(())
    };
    for p in s.processes.iter() {
        define(p.type_name.clone(), format!("process {}", p.type_name))?;
    }
    for c in s.containers.iter() {
        define(c.type_name.clone(), format!("container {}", c.type_name))?;
        define(state_class(&c.type_name, mode), format!("the state class of container {}", c.type_name))?;
        for p in c.container_processes.iter() {
            define(p.type_name.clone(), format!("container_process {}", p.type_name))?;
        }
    }
    let taken: BTreeSet<String> = defined.keys().cloned().collect();

    let mut types = BTreeSet::new();
    let all = s
        .processes
        .iter()
        .chain(s.containers.iter().flat_map(|c| c.container_processes.iter()));
    let mut uses_image = false;
    let mut uses_base64 = false;
    for p in all {
        for schema in p.arguments.iter().map(|a| &a.schema).chain([&p.output_schema]) {
            let t = schema_type(schema)?;
            uses_image |= matches!(t, DataType::Image);
            collect_named(&t, &mut types);
        }
    }
    for c in s.containers.iter() {
        for a in c.arguments.iter() {
            collect_named(&schema_type(&a.schema)?, &mut types);
        }
        for st in c.state.iter() {
            let t = schema_type(&st.schema)?;
            collect_all_named(&t, &mut types);
            if let StateInit::Value(Value::String(v)) = &st.init {
                let inner = match &t {
                    DataType::Optional { inner, .. } => inner.as_ref(),
                    t => t,
                };
                uses_base64 |= matches!(inner, DataType::Bytes) && !v.is_empty();
            }
        }
    }
    let names = TypeNames::new(&types.into_iter().collect::<Vec<_>>(), &taken)?;

    let mut decorators = Vec::new();
    if !s.containers.is_empty() {
        decorators.push("juiz_container");
        if s.containers.iter().any(|c| !c.container_processes.is_empty()) {
            decorators.push("juiz_container_process");
        }
    }
    if !s.processes.is_empty() {
        decorators.push("juiz_process");
    }
    let mut out = String::new();
    if uses_base64 {
        out.push_str("import base64\n\n");
    }
    out.push_str(&format!("from juiz import {}\n", decorators.join(", ")));
    if uses_image {
        out.push_str("from PIL.Image import Image\n");
    }
    let imports = names.imports();
    if !imports.is_empty() {
        out.push('\n');
        out.push_str(&imports);
    }
    for p in s.processes.iter() {
        out.push_str("\n\n");
        out.push_str(&function("@juiz_process", p, None, &names, mode)?);
    }
    for c in s.containers.iter() {
        out.push_str("\n\n");
        out.push_str(&container_code(c, &names, mode)?);
    }
    if let Mode::Design(DesignSkeleton { tail: Some(t), .. }) = mode {
        out.push_str("\n\n");
        out.push_str(t);
        if !t.ends_with('\n') {
            out.push('\n');
        }
    }
    Ok(out)
}
