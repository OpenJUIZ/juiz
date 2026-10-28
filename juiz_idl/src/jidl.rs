//! 型を JIDL の宣言に書き戻す (`juiz type show` の表示、仕様 §23、issue #197)
//!
//! 書いたものは juiz-idl で読み直せて、同じ指紋の型になる。
//! Avro の属性 (`juiz.constants` など) は `@juiz.constants(…)` の注釈で書く。

use juiz_types::DataType;
use serde_json::{Map, Value};

use crate::model::collect_named;

/// バッククォートで囲む名前 (JIDL のキーワード、仕様 §9.2)
const KEYWORDS: [&str; 22] = [
    "namespace",
    "import",
    "idl",
    "schema",
    "record",
    "enum",
    "process",
    "container",
    "container_process",
    "void",
    "boolean",
    "int",
    "long",
    "float",
    "double",
    "string",
    "bytes",
    "null",
    "any",
    "image",
    "array",
    "map",
];

fn name(n: &str) -> String {
    if KEYWORDS.contains(&n) || n == "union" {
        format!("`{n}`")
    } else {
        n.to_owned()
    }
}

fn named(t: &DataType) -> (&str, &str) {
    match t {
        DataType::Record(r) => (&r.namespace, &r.name),
        DataType::Enum(e) => (&e.namespace, &e.name),
        _ => unreachable!(),
    }
}

/// 型の式。`ns` はファイルの名前空間 (同じ名前空間の型は短い名前で書く)。
/// `default_is_null` は `T?` の既定値が無いか `null` か (`T?` と書けるかどうかを決める、仕様 §9.2)
pub(crate) fn type_expr(t: &DataType, ns: &str, default_is_null: bool) -> String {
    match t {
        DataType::Null => "null".to_owned(),
        DataType::Boolean => "boolean".to_owned(),
        DataType::Int => "int".to_owned(),
        DataType::Long => "long".to_owned(),
        DataType::Float => "float".to_owned(),
        DataType::Double => "double".to_owned(),
        DataType::String => "string".to_owned(),
        DataType::Bytes => "bytes".to_owned(),
        DataType::Array(items) => format!("array<{}>", type_expr(items, ns, true)),
        DataType::Map(values) => format!("map<{}>", type_expr(values, ns, true)),
        DataType::Optional { inner, null_first } => {
            let inner = type_expr(inner, ns, true);
            if *null_first == default_is_null {
                format!("{inner}?")
            } else if *null_first {
                format!("union {{ null, {inner} }}")
            } else {
                format!("union {{ {inner}, null }}")
            }
        }
        DataType::Record(_) | DataType::Enum(_) => {
            let (tns, n) = named(t);
            if tns == ns {
                name(n)
            } else {
                // 完全名の要素はキーワードでも囲まなくてよい (`.` を含む名前はキーワードにならない)
                format!("{tns}.{n}")
            }
        }
        DataType::Any => "any".to_owned(),
        DataType::Image => "image".to_owned(),
    }
}

pub(crate) fn doc_comment(doc: &Option<String>, pad: &str) -> String {
    match doc {
        Some(d) => format!("{pad}/** {} */\n", d.replace("*/", "* /")),
        None => String::new(),
    }
}

fn aliases(a: &[String]) -> String {
    format!("@aliases({}) ", serde_json::to_string(a).unwrap())
}

/// 属性を注釈 `@名前(値) ` の並びで書く。名前に `.` を含まないものは注釈で書けないので書かない
pub(crate) fn attribute_annotations(attributes: &Map<String, Value>) -> String {
    attributes
        .iter()
        .filter(|(k, _)| k.contains('.') && is_qualified_name(k))
        .map(|(k, v)| format!("@{k}({}) ", serde_json::to_string(v).unwrap()))
        .collect()
}

fn is_qualified_name(n: &str) -> bool {
    n.split('.').all(juiz_types::is_valid_type_name)
}

/// 名前 (キーワードならバッククォートで囲む)
pub(crate) fn jidl_name(n: &str) -> String {
    name(n)
}

/// 名前付き型 `t` と、それが使う名前付き型の JIDL の宣言。名前空間は `t` のもの
pub fn to_jidl(t: &DataType) -> String {
    let mut types = Vec::new();
    collect_named(t, &mut types);
    let ns = match t {
        DataType::Record(_) | DataType::Enum(_) => named(t).0.to_owned(),
        _ => String::new(),
    };
    let mut out = String::new();
    if !ns.is_empty() {
        out.push_str(&format!("namespace {ns};\n"));
    }
    // 調べたい型を先に、使っている型を後に書く (宣言の順は自由、仕様 §9.2)
    types.reverse();
    for d in types.iter() {
        out.push('\n');
        out.push_str(&declaration(d, &ns));
    }
    out
}

/// 名前付き型 1 つの宣言。`ns` はファイルの名前空間 (違えば `@namespace` を付ける)
pub(crate) fn declaration(d: &DataType, ns: &str) -> String {
    let mut out = String::new();
    let (dns, dname) = named(d);
    let (doc, als, attrs) = match d {
        DataType::Record(r) => (&r.doc, &r.aliases, &r.attributes),
        DataType::Enum(e) => (&e.doc, &e.aliases, &e.attributes),
        _ => unreachable!(),
    };
    out.push_str(&doc_comment(doc, ""));
    if dns != ns {
        out.push_str(&format!(
            "@namespace({})\n",
            serde_json::to_string(dns).unwrap()
        ));
    }
    if !als.is_empty() {
        out.push_str(aliases(als).trim_end());
        out.push('\n');
    }
    let attrs = attribute_annotations(attrs);
    if !attrs.is_empty() {
        out.push_str(attrs.trim_end());
        out.push('\n');
    }
    match d {
        DataType::Record(r) => {
            out.push_str(&format!("record {} {{\n", name(dname)));
            for f in r.fields.iter() {
                out.push_str(&doc_comment(&f.doc, "  "));
                out.push_str("  ");
                if !f.aliases.is_empty() {
                    out.push_str(&aliases(&f.aliases));
                }
                out.push_str(&attribute_annotations(&f.attributes));
                let default = f.default_as_dict();
                let default_is_null = matches!(default, None | Some(Value::Null));
                out.push_str(&type_expr(&f.data_type, dns, default_is_null));
                out.push(' ');
                out.push_str(&name(&f.name));
                if let Some(v) = default {
                    out.push_str(&format!(" = {}", serde_json::to_string(&v).unwrap()));
                }
                out.push_str(";\n");
            }
            out.push_str("}\n");
        }
        DataType::Enum(e) => {
            let symbols = e.symbols.iter().map(|s| name(s)).collect::<Vec<_>>();
            out.push_str(&format!(
                "enum {} {{ {} }}",
                name(dname),
                symbols.join(", ")
            ));
            if let Some(d) = &e.default {
                out.push_str(&format!(" = {}", name(d)));
            }
            out.push_str(";\n");
        }
        _ => unreachable!(),
    }
    out
}
