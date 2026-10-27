//! OpenAPI (3.0) のスキーマへの書き出し (データ型 Step 8、issue #197、仕様 docs/spec/datatype.md §23.3)
//!
//! - HTTP ブローカーの `/api-docs/openapi.json` (Swagger UI の `/docs`) の `components.schemas` に、
//!   型の登録所の名前付き型と、Factory ごとの引数・戻り値のスキーマを足す ([`openapi_components`])。
//! - Process の `openapi_spec()` (`process/openapi_spec` の API) が返す、その Process だけのスキーマ ([`process_openapi_spec`])。
//!
//! 表すのは辞書の表し方 (仕様 §4)。utoipa の書き出す OpenAPI 3.0.3 に合わせるので、
//! JSON Schema 2020-12 (juiz-idl の `-g json-schema`) とは書き方が少し違う (`nullable`、`format`)。

use std::collections::{BTreeMap, HashMap};

use juiz_sdk::{
    manifests::{ArgumentManifest, ArgumentType, ProcessManifest},
    types::DataType,
};
use serde_json::{json, Map};

use super::collect_named_types;
use crate::prelude::*;

/// 名前付き型の識別子 (`<完全名>@sha256:…`) → `components.schemas` のキー
///
/// キーは完全名。同じ完全名で指紋の違う型が並ぶときは `<完全名>_<指紋の先頭 8 桁>` にする
/// (OpenAPI のキーに `@` と `:` は使えない)。
struct SchemaKeys(HashMap<String, String>);

impl SchemaKeys {
    fn new(types: &[DataType]) -> Self {
        let mut by_name: BTreeMap<String, Vec<&DataType>> = BTreeMap::new();
        for t in types.iter() {
            if let Some(n) = t.full_name() {
                by_name.entry(n).or_default().push(t);
            }
        }
        let mut keys = HashMap::new();
        for (name, ts) in by_name.iter() {
            for t in ts.iter() {
                let fp = t.fingerprint();
                let key = if ts.len() == 1 {
                    name.clone()
                } else {
                    format!("{name}_{}", short_hex(&fp))
                };
                keys.insert(format!("{name}@{fp}"), key);
            }
        }
        SchemaKeys(keys)
    }

    fn key(&self, t: &DataType) -> String {
        let name = t.full_name().unwrap_or_default();
        self.0
            .get(&format!("{name}@{}", t.fingerprint()))
            .cloned()
            .unwrap_or(name)
    }
}

fn short_hex(fingerprint: &str) -> String {
    fingerprint
        .trim_start_matches("sha256:")
        .chars()
        .take(8)
        .collect()
}

/// `components.schemas` のキーに使えない文字 (`[A-Za-z0-9._-]` 以外) を `_` にする
fn sanitize_key(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// 型の OpenAPI のスキーマ。名前付き型は `#/components/schemas/<キー>` を参照する
fn schema_of(t: &DataType, keys: &SchemaKeys) -> Value {
    match t {
        DataType::Null => json!({"nullable": true, "description": "null"}),
        DataType::Boolean => json!({"type": "boolean"}),
        DataType::Int => json!({"type": "integer", "format": "int32"}),
        DataType::Long => json!({"type": "integer", "format": "int64"}),
        DataType::Float => json!({"type": "number", "format": "float"}),
        DataType::Double => json!({"type": "number", "format": "double"}),
        DataType::String => json!({"type": "string"}),
        // 辞書では base64 の文字列 (仕様 §4)
        DataType::Bytes => json!({"type": "string", "format": "byte"}),
        DataType::Array(items) => json!({"type": "array", "items": schema_of(items, keys)}),
        DataType::Map(values) => {
            json!({"type": "object", "additionalProperties": schema_of(values, keys)})
        }
        DataType::Optional { inner, .. } => {
            with_extra(schema_of(inner, keys), "nullable", true.into())
        }
        DataType::Record(_) | DataType::Enum(_) => {
            json!({"$ref": format!("#/components/schemas/{}", keys.key(t))})
        }
        // 何でも入る
        DataType::Any => json!({}),
        // 辞書には入らない (Capsule::Image)。HTTP では multipart のファイルで送る
        DataType::Image => {
            json!({"type": "string", "format": "binary", "x-juiz-type": "image"})
        }
    }
}

/// スキーマに属性を足す。OpenAPI 3.0 では `$ref` の隣の属性が無視されるので、`allOf` で包む
fn with_extra(schema: Value, key: &str, value: Value) -> Value {
    let mut m = match schema {
        Value::Object(m) if !m.contains_key("$ref") => m,
        other => {
            let mut m = Map::new();
            m.insert("allOf".to_owned(), Value::Array(vec![other]));
            m
        }
    };
    m.insert(key.to_owned(), value);
    Value::Object(m)
}

/// record・enum の中身
fn named_body(t: &DataType, keys: &SchemaKeys) -> Value {
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
                let mut p = schema_of(&f.data_type, keys);
                if let Some(doc) = &f.doc {
                    p = with_extra(p, "description", doc.as_str().into());
                }
                match f.default_as_dict() {
                    Some(d) => p = with_extra(p, "default", d),
                    None => required.push(Value::from(f.name.as_str())),
                }
                properties.insert(f.name.clone(), p);
            }
            m.insert("properties".to_owned(), Value::Object(properties));
            // OpenAPI 3.0 の required は空にできない
            if !required.is_empty() {
                m.insert("required".to_owned(), Value::Array(required));
            }
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
    m.insert("x-juiz-fingerprint".to_owned(), t.fingerprint().into());
    Value::Object(m)
}

/// 型なしの引数のスキーマ (`type_name` から)
fn untyped_argument_schema(t: &ArgumentType) -> Value {
    match t {
        ArgumentType::Bool => json!({"type": "boolean"}),
        ArgumentType::Int => json!({"type": "integer", "format": "int64"}),
        ArgumentType::Float => json!({"type": "number", "format": "double"}),
        ArgumentType::String => json!({"type": "string"}),
        ArgumentType::Array => json!({"type": "array", "items": {}}),
        ArgumentType::Object => json!({"type": "object"}),
        ArgumentType::Image => {
            json!({"type": "string", "format": "binary", "x-juiz-type": "image"})
        }
    }
}

fn argument_schema(a: &ArgumentManifest, keys: &SchemaKeys) -> Value {
    let data_type = a.schema.as_ref().and_then(|s| s.data_type().ok());
    let mut s = match &data_type {
        Some(t) => schema_of(t, keys),
        None => untyped_argument_schema(&a.type_name),
    };
    let description = a.description.as_str().to_owned();
    if !description.is_empty() {
        s = with_extra(s, "description", description.into());
    }
    // 既定値のない型付きの引数は default が null (画像は既定値を書かない)
    let has_default =
        !(a.default.is_null() && (a.schema.is_some() || a.type_name == ArgumentType::Image));
    if has_default && a.type_name != ArgumentType::Image {
        s = with_extra(s, "default", a.default.clone());
    }
    s
}

/// 引数の辞書 (`call` の本体) のスキーマ
fn arguments_schema(m: &ProcessManifest, keys: &SchemaKeys) -> Value {
    let mut properties = Map::new();
    for a in m.arguments.iter() {
        properties.insert(a.name.clone(), argument_schema(a, keys));
    }
    let mut s = Map::new();
    let description = m.description.as_str().to_owned();
    if !description.is_empty() {
        s.insert("description".to_owned(), description.into());
    }
    s.insert("type".to_owned(), "object".into());
    s.insert("properties".to_owned(), Value::Object(properties));
    Value::Object(s)
}

fn output_data_type(m: &ProcessManifest) -> Option<DataType> {
    m.output_schema.as_ref().and_then(|s| s.data_type().ok())
}

/// manifest の型参照の名前付き型 (入れ子も含む)
fn named_types_of(manifests: &[&ProcessManifest]) -> Vec<DataType> {
    let mut types = Vec::new();
    for m in manifests.iter() {
        let schemas = m
            .arguments
            .iter()
            .filter_map(|a| a.schema.as_ref())
            .chain(m.output_schema.iter());
        for s in schemas {
            if let Ok(t) = s.validate() {
                collect_named_types(&t, &mut types);
            }
        }
    }
    // 同じ型 (完全名と指紋が同じ) は 1 つにする
    let mut seen = std::collections::HashSet::new();
    types.retain(|t| {
        seen.insert(format!(
            "{}@{}",
            t.full_name().unwrap_or_default(),
            t.fingerprint()
        ))
    });
    types
}

fn named_schemas(types: &[DataType], keys: &SchemaKeys, out: &mut Map<String, Value>) {
    for t in types.iter() {
        out.insert(keys.key(t), named_body(t, keys));
    }
}

/// Factory の種類 (`process` か `container_process`) と manifest
pub type FactoryManifest = (&'static str, ProcessManifest);

/// `components.schemas` に足すもの
///
/// - 名前付き型: キーは完全名 (同じ完全名の型が複数あれば `<完全名>_<指紋の先頭 8 桁>`)
/// - Factory ごとに `<種類>.<型名>.arguments` (引数の辞書) と、戻り値が型付きなら `<種類>.<型名>.returns`。
///   ContainerProcess の型名の前には Container の型名を付ける (`container_process.<Container の型>.<型名>`)
pub fn openapi_components(factories: &[FactoryManifest]) -> Map<String, Value> {
    let manifests: Vec<&ProcessManifest> = factories.iter().map(|(_, m)| m).collect();
    let types = named_types_of(&manifests);
    let keys = SchemaKeys::new(&types);
    let mut out = Map::new();
    named_schemas(&types, &keys, &mut out);
    for (kind, m) in factories.iter() {
        let base = match (&m.container_type, *kind) {
            (Some(ct), "container_process") => format!("{kind}.{ct}.{}", m.type_name),
            _ => format!("{kind}.{}", m.type_name),
        };
        let base = sanitize_key(&base);
        out.insert(format!("{base}.arguments"), arguments_schema(m, &keys));
        if let Some(t) = output_data_type(m) {
            out.insert(format!("{base}.returns"), schema_of(&t, &keys));
        }
    }
    out
}

/// 1 つの Process のスキーマ (`process/openapi_spec` の応答)
///
/// `{"arguments": 引数の辞書, "returns": 戻り値 (型なしなら null), "components": {"schemas": 使う名前付き型}}`。
/// 参照は `#/components/schemas/<完全名>`。
pub fn process_openapi_spec(profile: &ProcessProfile) -> Value {
    let mut m = ProcessManifest::new(&profile.type_name);
    m.description = profile.description.clone();
    m.output_schema = profile.output_schema.clone();
    m.arguments = profile
        .arguments
        .iter()
        .map(|a| ArgumentManifest {
            type_name: a.type_name.clone(),
            name: a.name.clone(),
            description: a.description.clone(),
            default: a.default.clone(),
            schema: a.schema.clone(),
        })
        .collect();
    let m = &m;
    let types = named_types_of(&[m]);
    let keys = SchemaKeys::new(&types);
    let mut schemas = Map::new();
    named_schemas(&types, &keys, &mut schemas);
    let returns = output_data_type(m)
        .map(|t| schema_of(&t, &keys))
        .unwrap_or(Value::Null);
    json!({
        "arguments": arguments_schema(m, &keys),
        "returns": returns,
        "components": { "schemas": schemas },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use juiz_sdk::types::{EnumType, RecordField, RecordType, TypeSchema};

    fn point(extra: bool) -> DataType {
        let mut r = RecordType::new("a", "Point")
            .doc("点")
            .field(RecordField::new("x", DataType::Double));
        if extra {
            r = r.field(
                RecordField::new("z", DataType::Double)
                    .default_value(jvalue!(0.0))
                    .unwrap(),
            );
        }
        DataType::Record(r)
    }

    fn pose() -> DataType {
        let mode =
            DataType::Enum(EnumType::new("a", "Mode", &["Idle", "Run"]).default_symbol("Idle"));
        DataType::Record(
            RecordType::new("a", "Pose")
                .field(RecordField::new("p", point(false)).doc("位置"))
                .field(
                    RecordField::new("tag", DataType::optional(DataType::String))
                        .default_value(Value::Null)
                        .unwrap(),
                )
                .field(RecordField::new("mode", mode))
                .field(RecordField::new("data", DataType::Bytes)),
        )
    }

    fn schema(t: &DataType) -> TypeSchema {
        TypeSchema::from_data_type(t).unwrap()
    }

    #[test]
    fn components_for_types_and_factories() {
        let m = ProcessManifest::new("move")
            .description("動かす")
            .add_typed_arg("pose", "姿勢", schema(&pose()), Value::Null)
            .unwrap()
            .add_float_arg("speed", "", 1.0)
            .set_output_schema(Some(schema(&pose())))
            .unwrap();
        let mut cm = ProcessManifest::new("get");
        cm.container_type = Some("robot".to_owned());
        let cm = cm.set_output_schema(Some(schema(&point(true)))).unwrap();
        let c = openapi_components(&[("process", m), ("container_process", cm)]);

        // 同じ完全名で指紋の違う Point は指紋の先頭で分ける
        let p1 = format!("a.Point_{}", short_hex(&point(false).fingerprint()));
        let p2 = format!("a.Point_{}", short_hex(&point(true).fingerprint()));
        assert!(c.contains_key(&p1) && c.contains_key(&p2), "{c:?}");
        let pose = &c["a.Pose"];
        assert_eq!(pose["type"], "object");
        assert_eq!(pose["required"], jvalue!(["p", "mode", "data"]));
        // $ref の隣に書けない属性は allOf で包む
        assert_eq!(
            pose["properties"]["p"],
            jvalue!({"allOf": [{"$ref": format!("#/components/schemas/{p1}")}], "description": "位置"})
        );
        assert_eq!(
            pose["properties"]["tag"],
            jvalue!({"type": "string", "nullable": true, "default": null})
        );
        assert_eq!(
            pose["properties"]["data"],
            jvalue!({"type": "string", "format": "byte"})
        );
        assert_eq!(c["a.Mode"]["enum"], jvalue!(["Idle", "Run"]));
        assert_eq!(c["a.Mode"]["default"], "Idle");
        assert_eq!(c["a.Pose"]["x-juiz-fingerprint"], jvalue!(pose_fp()));

        let args = &c["process.move.arguments"];
        assert_eq!(args["description"], "動かす");
        assert_eq!(
            args["properties"]["pose"],
            jvalue!({"allOf": [{"$ref": "#/components/schemas/a.Pose"}], "description": "姿勢"})
        );
        assert_eq!(
            args["properties"]["speed"],
            jvalue!({"type": "number", "format": "double", "default": 1.0})
        );
        assert_eq!(
            c["process.move.returns"],
            jvalue!({"$ref": "#/components/schemas/a.Pose"})
        );
        assert_eq!(
            c["container_process.robot.get.returns"],
            jvalue!({"$ref": format!("#/components/schemas/{p2}")})
        );
        assert_eq!(
            c["container_process.robot.get.arguments"]["properties"],
            jvalue!({})
        );
    }

    fn pose_fp() -> String {
        pose().fingerprint()
    }

    #[test]
    fn spec_of_one_process() {
        let m = ProcessManifest::new("move")
            .add_typed_arg("pose", "", schema(&pose()), Value::Null)
            .unwrap()
            .add_int_arg("n", "回数", 3);
        let s = process_openapi_spec(&m.name("mv0").try_into().unwrap());
        assert_eq!(s["returns"], Value::Null);
        assert_eq!(
            s["arguments"]["properties"]["n"],
            jvalue!({"type": "integer", "format": "int64", "description": "回数", "default": 3})
        );
        assert_eq!(
            s["arguments"]["properties"]["pose"],
            jvalue!({"$ref": "#/components/schemas/a.Pose"})
        );
        let schemas = s["components"]["schemas"].as_object().unwrap();
        let mut keys: Vec<&String> = schemas.keys().collect();
        keys.sort();
        assert_eq!(keys, vec!["a.Mode", "a.Point", "a.Pose"]);
        assert_eq!(
            schemas["a.Pose"]["properties"]["p"]["allOf"][0]["$ref"],
            "#/components/schemas/a.Point"
        );
    }
}
