//! JSON Schema (2020-12) の書き出し。辞書の表し方 (仕様 §4) を表す
//!
//! 管理画面のフォームや Swagger から使う想定。型付きの値が辞書としてどう見えるかを表すもので、
//! 互換性の判定や指紋には使わない (正本は .avsc)。

use juiz_types::DataType;
use serde_json::{json, Map, Value};

use crate::model::collect_named;

pub const JSON_SCHEMA_DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";

/// 名前付き型 1 つの JSON Schema。参照する名前付き型は `$defs` に完全名で入れる
pub fn json_schema_of(t: &DataType) -> Value {
    let mut named = Vec::new();
    collect_named(t, &mut named);
    let mut defs = Map::new();
    for n in named.iter() {
        if n != t {
            defs.insert(n.full_name().unwrap(), named_body(n));
        }
    }
    let mut root = Map::new();
    root.insert("$schema".to_owned(), JSON_SCHEMA_DIALECT.into());
    let body = match t {
        DataType::Record(_) | DataType::Enum(_) => named_body(t),
        _ => schema_of(t),
    };
    if let Value::Object(m) = body {
        root.extend(m);
    }
    root.insert("x-juiz-fingerprint".to_owned(), t.fingerprint().into());
    if !defs.is_empty() {
        root.insert("$defs".to_owned(), Value::Object(defs));
    }
    Value::Object(root)
}

/// record / enum の中身
fn named_body(t: &DataType) -> Value {
    let mut m = Map::new();
    match t {
        DataType::Record(r) => {
            m.insert("title".to_owned(), r.full_name().into());
            if let Some(doc) = &r.doc {
                m.insert("description".to_owned(), doc.as_str().into());
            }
            m.insert("type".to_owned(), "object".into());
            let mut properties = Map::new();
            let mut required = Vec::new();
            for f in r.fields.iter() {
                let mut p = match schema_of(&f.data_type) {
                    Value::Object(p) => p,
                    _ => unreachable!(),
                };
                if let Some(doc) = &f.doc {
                    p.insert("description".to_owned(), doc.as_str().into());
                }
                match f.default_as_dict() {
                    Some(d) => {
                        p.insert("default".to_owned(), d);
                    }
                    None => required.push(Value::from(f.name.as_str())),
                }
                properties.insert(f.name.clone(), Value::Object(p));
            }
            m.insert("properties".to_owned(), Value::Object(properties));
            // 型にないキーは読むときに無視する (仕様 §4) ので additionalProperties は制限しない
            m.insert("required".to_owned(), Value::Array(required));
        }
        DataType::Enum(e) => {
            m.insert("title".to_owned(), e.full_name().into());
            if let Some(doc) = &e.doc {
                m.insert("description".to_owned(), doc.as_str().into());
            }
            m.insert("type".to_owned(), "string".into());
            m.insert("enum".to_owned(), e.symbols.clone().into());
            if let Some(d) = &e.default {
                m.insert("default".to_owned(), d.as_str().into());
            }
        }
        _ => unreachable!(),
    }
    Value::Object(m)
}

fn schema_of(t: &DataType) -> Value {
    match t {
        DataType::Null => json!({"type": "null"}),
        DataType::Boolean => json!({"type": "boolean"}),
        DataType::Int => json!({"type": "integer", "minimum": i32::MIN, "maximum": i32::MAX}),
        DataType::Long => json!({"type": "integer", "minimum": i64::MIN, "maximum": i64::MAX}),
        DataType::Float | DataType::Double => json!({"type": "number"}),
        DataType::String => json!({"type": "string"}),
        DataType::Bytes => json!({"type": "string", "contentEncoding": "base64"}),
        DataType::Array(items) => json!({"type": "array", "items": schema_of(items)}),
        DataType::Map(values) => {
            json!({"type": "object", "additionalProperties": schema_of(values)})
        }
        DataType::Optional { inner, .. } => json!({"anyOf": [schema_of(inner), {"type": "null"}]}),
        DataType::Record(_) | DataType::Enum(_) => {
            json!({"$ref": format!("#/$defs/{}", t.full_name().unwrap())})
        }
        // 何でも入る
        DataType::Any => json!({}),
        // 辞書には入らない (Capsule::Image で渡す)。引数・戻り値の型そのものとしてだけ出てくる
        DataType::Image => json!({"x-juiz-type": "image"}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use juiz_types::{RecordField, RecordType};

    #[test]
    fn top_record_refers_to_defs() {
        let point = DataType::Record(
            RecordType::new("a", "P").field(RecordField::new("x", DataType::Double)),
        );
        let pose = DataType::Record(
            RecordType::new("a", "Q")
                .field(RecordField::new("p", point))
                .field(
                    RecordField::new("t", DataType::Double)
                        .default_value(json!(1.0))
                        .unwrap(),
                ),
        );
        let s = json_schema_of(&pose);
        assert_eq!(s["title"], "a.Q");
        assert_eq!(s["properties"]["p"]["$ref"], "#/$defs/a.P");
        assert_eq!(s["properties"]["t"]["default"], 1.0);
        assert_eq!(s["required"], json!(["p"]));
        assert_eq!(s["$defs"]["a.P"]["required"], json!(["x"]));
    }
}
