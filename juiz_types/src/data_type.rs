//! 型のモデル (仕様 docs/spec/datatype.md §3)。Avro のスキーマ (.avsc) を読み書きする
//!
//! 遅延評価 (#3) でネイティブ値を渡すときに使う `destroy` / `clone` の関数は、型そのものではなく
//! 言語ごとの登録所 (Step 4 #193) に置く。型の側からは [`super::TypeTag`] で ABI タグを付けられる。

use std::collections::{HashMap, HashSet};

use base64::Engine;
use regex::Regex;
use serde_json::Map;
use std::sync::OnceLock;

use crate::{Result, TypeError};
use serde_json::Value;

/// 名前空間 `juiz` は予約 (仕様 §3.4)
pub const RESERVED_NAMESPACE: &str = "juiz";
/// `any` を表す予約名 (仕様 §3.2)
pub const ANY_FULL_NAME: &str = "juiz.Any";
/// `image` を表す予約名 (仕様 §3.3)
pub const IMAGE_FULL_NAME: &str = "juiz.Image";

/// 1 つの型。Avro の型のうち仕様 §3.1 で使えるものだけを表す
#[derive(Clone, Debug, PartialEq)]
pub enum DataType {
    Null,
    Boolean,
    Int,
    Long,
    Float,
    Double,
    String,
    Bytes,
    Array(Box<DataType>),
    Map(Box<DataType>),
    /// `T?`。`null_first` は Avro の union の並び (`["null",T]` なら true)。並びは指紋に入る
    Optional {
        inner: Box<DataType>,
        null_first: bool,
    },
    Record(RecordType),
    Enum(EnumType),
    Any,
    Image,
}

/// レコード (名前付き型)
#[derive(Clone, Debug, PartialEq)]
pub struct RecordType {
    pub name: String,
    /// 型の名前空間 (空ならなし)。ID ネームスペースとは別物
    pub namespace: String,
    pub doc: Option<String>,
    /// 書かれたままの別名。完全名でなければ `namespace` を補って読む
    pub aliases: Vec<String>,
    pub fields: Vec<RecordField>,
    /// 上以外の属性 (`juiz.constants` など)。指紋にも互換性の判定にも使わない
    pub attributes: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecordField {
    pub name: String,
    pub doc: Option<String>,
    pub aliases: Vec<String>,
    /// Avro の JSON 表記の既定値 (`bytes` は 0〜255 の文字列)。辞書の表記は [`RecordField::default_value`]
    pub default: Option<Value>,
    pub data_type: DataType,
    pub attributes: Map<String, Value>,
}

/// 列挙 (名前付き型)
#[derive(Clone, Debug, PartialEq)]
pub struct EnumType {
    pub name: String,
    pub namespace: String,
    pub doc: Option<String>,
    pub aliases: Vec<String>,
    pub symbols: Vec<String>,
    pub default: Option<String>,
    pub attributes: Map<String, Value>,
}

pub(crate) fn schema_invalid(path: &str, message: impl Into<String>) -> TypeError {
    TypeError::SchemaInvalid {
        path: path.to_owned(),
        message: message.into(),
    }
}

fn name_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").unwrap())
}

/// Avro の名前の規則 (`[A-Za-z_][A-Za-z0-9_]*`) に合うか (仕様 §3.1)
pub fn is_valid_type_name(name: &str) -> bool {
    name_regex().is_match(name)
}

/// Avro の名前の規則で検査する。型付きの引数名の検査にも使う
pub fn validate_type_name(name: &str, path: &str) -> Result<()> {
    if is_valid_type_name(name) {
        Ok(())
    } else {
        Err(schema_invalid(
            path,
            format!("name '{name}' must match [A-Za-z_][A-Za-z0-9_]*"),
        ))
    }
}

fn validate_namespace_str(namespace: &str, path: &str) -> Result<()> {
    if namespace.is_empty() {
        return Ok(());
    }
    for element in namespace.split('.') {
        if !is_valid_type_name(element) {
            return Err(schema_invalid(
                path,
                format!("namespace '{namespace}' has an invalid element '{element}'"),
            ));
        }
    }
    Ok(())
}

fn join_full_name(namespace: &str, name: &str) -> String {
    if namespace.is_empty() {
        name.to_owned()
    } else {
        format!("{namespace}.{name}")
    }
}

/// 別名を完全名にする (`.` を含まなければ型の名前空間を補う)
fn alias_full_name(namespace: &str, alias: &str) -> String {
    if alias.contains('.') {
        alias.to_owned()
    } else {
        join_full_name(namespace, alias)
    }
}

impl RecordType {
    pub fn new(namespace: &str, name: &str) -> Self {
        Self {
            name: name.to_owned(),
            namespace: namespace.to_owned(),
            doc: None,
            aliases: Vec::new(),
            fields: Vec::new(),
            attributes: Map::new(),
        }
    }

    pub fn full_name(&self) -> String {
        join_full_name(&self.namespace, &self.name)
    }

    /// 別名を完全名にしたもの
    pub fn alias_full_names(&self) -> Vec<String> {
        self.aliases
            .iter()
            .map(|a| alias_full_name(&self.namespace, a))
            .collect()
    }

    pub fn doc(mut self, doc: &str) -> Self {
        self.doc = Some(doc.to_owned());
        self
    }

    pub fn alias(mut self, alias: &str) -> Self {
        self.aliases.push(alias.to_owned());
        self
    }

    pub fn field(mut self, field: RecordField) -> Self {
        self.fields.push(field);
        self
    }

    pub fn get_field(&self, name: &str) -> Option<&RecordField> {
        self.fields.iter().find(|f| f.name == name)
    }
}

impl RecordField {
    pub fn new(name: &str, data_type: DataType) -> Self {
        Self {
            name: name.to_owned(),
            doc: None,
            aliases: Vec::new(),
            default: None,
            data_type,
            attributes: Map::new(),
        }
    }

    pub fn doc(mut self, doc: &str) -> Self {
        self.doc = Some(doc.to_owned());
        self
    }

    pub fn alias(mut self, alias: &str) -> Self {
        self.aliases.push(alias.to_owned());
        self
    }

    /// 既定値を辞書の表記 (仕様 §4) で付ける。
    ///
    /// `T?` のフィールドは、既定値が `null` なら `["null",T]`、それ以外なら `[T,"null"]` の並びにする (仕様 §9.2)。
    pub fn default_value(mut self, default: Value) -> Result<Self> {
        if let DataType::Optional { null_first, .. } = &mut self.data_type {
            *null_first = default.is_null();
        }
        let path = self.name.clone();
        self.default = Some(dict_to_avro_default(&self.data_type, &default, &path)?);
        Ok(self)
    }

    /// 既定値を辞書の表記 (仕様 §4) で返す
    pub fn default_as_dict(&self) -> Option<Value> {
        self.default
            .as_ref()
            .map(|d| avro_default_to_dict(&self.data_type, d))
    }
}

impl EnumType {
    pub fn new(namespace: &str, name: &str, symbols: &[&str]) -> Self {
        Self {
            name: name.to_owned(),
            namespace: namespace.to_owned(),
            doc: None,
            aliases: Vec::new(),
            symbols: symbols.iter().map(|s| (*s).to_owned()).collect(),
            default: None,
            attributes: Map::new(),
        }
    }

    pub fn full_name(&self) -> String {
        join_full_name(&self.namespace, &self.name)
    }

    pub fn alias_full_names(&self) -> Vec<String> {
        self.aliases
            .iter()
            .map(|a| alias_full_name(&self.namespace, a))
            .collect()
    }

    pub fn doc(mut self, doc: &str) -> Self {
        self.doc = Some(doc.to_owned());
        self
    }

    pub fn alias(mut self, alias: &str) -> Self {
        self.aliases.push(alias.to_owned());
        self
    }

    pub fn default_symbol(mut self, symbol: &str) -> Self {
        self.default = Some(symbol.to_owned());
        self
    }
}

impl DataType {
    pub fn array(items: DataType) -> Self {
        DataType::Array(Box::new(items))
    }

    pub fn map(values: DataType) -> Self {
        DataType::Map(Box::new(values))
    }

    /// `T?` (`["null",T]`)
    pub fn optional(inner: DataType) -> Self {
        DataType::Optional {
            inner: Box::new(inner),
            null_first: true,
        }
    }

    /// 名前付き型 (record / enum と予約名) の完全名
    pub fn full_name(&self) -> Option<String> {
        match self {
            DataType::Record(r) => Some(r.full_name()),
            DataType::Enum(e) => Some(e.full_name()),
            DataType::Any => Some(ANY_FULL_NAME.to_owned()),
            DataType::Image => Some(IMAGE_FULL_NAME.to_owned()),
            _ => None,
        }
    }

    /// 表示名 (仕様 §5.2)。record と enum は完全名、それ以外は JIDL の書き方
    pub fn display_name(&self) -> String {
        match self {
            DataType::Null => "null".to_owned(),
            DataType::Boolean => "boolean".to_owned(),
            DataType::Int => "int".to_owned(),
            DataType::Long => "long".to_owned(),
            DataType::Float => "float".to_owned(),
            DataType::Double => "double".to_owned(),
            DataType::String => "string".to_owned(),
            DataType::Bytes => "bytes".to_owned(),
            DataType::Array(items) => format!("array<{}>", items.display_name()),
            DataType::Map(values) => format!("map<{}>", values.display_name()),
            DataType::Optional { inner, .. } => format!("{}?", inner.display_name()),
            DataType::Record(r) => r.full_name(),
            DataType::Enum(e) => e.full_name(),
            DataType::Any => "any".to_owned(),
            DataType::Image => "image".to_owned(),
        }
    }

    /// 位置の表示 (`Pose2D.position.x`) の先頭に使う名前。名前付き型は短い名前、それ以外は表示名
    pub fn path_root(&self) -> String {
        match self {
            DataType::Record(r) => r.name.clone(),
            DataType::Enum(e) => e.name.clone(),
            _ => self.display_name(),
        }
    }

    /// manifest の `type_name` (仕様 §6 の表)。`juiz_sdk` の `ArgumentType` の名前 (`"Bool"`、`"Object"` など) を返す
    pub fn argument_type_name(&self) -> &'static str {
        match self {
            DataType::Boolean => "Bool",
            DataType::Int | DataType::Long => "Int",
            DataType::Float | DataType::Double => "Float",
            DataType::String | DataType::Bytes | DataType::Enum(_) => "String",
            DataType::Array(_) => "Array",
            DataType::Map(_) | DataType::Record(_) | DataType::Any | DataType::Null => "Object",
            DataType::Image => "Image",
            DataType::Optional { inner, .. } => inner.argument_type_name(),
        }
    }

    pub fn is_nullable(&self) -> bool {
        matches!(self, DataType::Null | DataType::Optional { .. })
    }

    /// Avro のスキーマ (.avsc の JSON) を読む。仕様 §3 に合わなければ `TypeSchemaInvalidError`
    pub fn from_avro(avro: &Value) -> Result<DataType> {
        let mut parser = Parser::default();
        parser.parse(avro, "", "", true)
    }

    /// Avro のスキーマの文字列を読む
    pub fn from_avro_str(avro: &str) -> Result<DataType> {
        let value: Value = serde_json::from_str(avro)
            .map_err(|e| schema_invalid("", format!("schema is not JSON: {e}")))?;
        Self::from_avro(&value)
    }

    /// それだけで完結した Avro のスキーマを書き出す (仕様 §6 の `schema.avro`)。
    ///
    /// 名前付き型は最初に出てきた場所で展開し、2 回目からは完全名で参照する。`doc`、`default`、`aliases` は残す。
    /// 同じ完全名で中身の違う型が混ざっていればエラー。
    pub fn to_avro(&self) -> Result<Value> {
        // 書く前に検査する (組み立てた型が §3 に合っているか、同じ名前の型がぶつかっていないか)
        let mut definitions = HashMap::new();
        self.collect_named(&mut definitions, &self.path_root())?;
        let avro = self.write_avro(&mut HashSet::new(), "");
        DataType::from_avro(&avro)?;
        Ok(avro)
    }

    /// Parsing Canonical Form (Avro 1.11、仕様 §5.1)
    pub fn canonical_form(&self) -> String {
        let mut out = String::new();
        self.write_pcf(&mut HashSet::new(), &mut out);
        out
    }

    /// 指紋 `sha256:<64 桁>` (仕様 §5.1)
    pub fn fingerprint(&self) -> String {
        fingerprint_of_canonical_form(&self.canonical_form())
    }

    fn collect_named(&self, definitions: &mut HashMap<String, String>, path: &str) -> Result<()> {
        let check = |full_name: String, definitions: &mut HashMap<String, String>| {
            let pcf = self.canonical_form();
            match definitions.get(&full_name) {
                Some(existing) if existing != &pcf => Err(schema_invalid(
                    path,
                    format!("named type '{full_name}' is defined twice with different contents"),
                )),
                Some(_) => Ok(false),
                None => {
                    definitions.insert(full_name, pcf);
                    Ok(true)
                }
            }
        };
        match self {
            DataType::Array(items) => items.collect_named(definitions, &format!("{path}[]")),
            DataType::Map(values) => values.collect_named(definitions, &format!("{path}{{}}")),
            DataType::Optional { inner, .. } => inner.collect_named(definitions, path),
            DataType::Record(r) => {
                if check(r.full_name(), definitions)? {
                    for f in r.fields.iter() {
                        f.data_type
                            .collect_named(definitions, &format!("{path}.{}", f.name))?;
                    }
                }
                Ok(())
            }
            DataType::Enum(e) => check(e.full_name(), definitions).map(|_| ()),
            _ => Ok(()),
        }
    }

    fn write_avro(&self, seen: &mut HashSet<String>, enclosing_namespace: &str) -> Value {
        fn named_header(
            obj: &mut Map<String, Value>,
            type_name: &str,
            name: &str,
            namespace: &str,
            enclosing_namespace: &str,
            doc: &Option<String>,
            aliases: &[String],
        ) {
            obj.insert("type".to_owned(), type_name.into());
            obj.insert("name".to_owned(), name.into());
            // 外側に名前空間があれば、空の名前空間も明示する (書かないと外側のものを継ぐため)
            if !namespace.is_empty() || !enclosing_namespace.is_empty() {
                obj.insert("namespace".to_owned(), namespace.into());
            }
            if let Some(doc) = doc {
                obj.insert("doc".to_owned(), doc.as_str().into());
            }
            if !aliases.is_empty() {
                obj.insert("aliases".to_owned(), aliases.to_vec().into());
            }
        }
        match self {
            DataType::Null => "null".into(),
            DataType::Boolean => "boolean".into(),
            DataType::Int => "int".into(),
            DataType::Long => "long".into(),
            DataType::Float => "float".into(),
            DataType::Double => "double".into(),
            DataType::String => "string".into(),
            DataType::Bytes => "bytes".into(),
            DataType::Array(items) => serde_json::json!({
                "type": "array",
                "items": items.write_avro(seen, enclosing_namespace),
            }),
            DataType::Map(values) => serde_json::json!({
                "type": "map",
                "values": values.write_avro(seen, enclosing_namespace),
            }),
            DataType::Optional { inner, null_first } => {
                let inner = inner.write_avro(seen, enclosing_namespace);
                if *null_first {
                    serde_json::json!(["null", inner])
                } else {
                    serde_json::json!([inner, "null"])
                }
            }
            DataType::Any | DataType::Image => {
                let full_name = self.full_name().unwrap();
                if !seen.insert(full_name.clone()) {
                    return full_name.into();
                }
                let name = full_name.rsplit('.').next().unwrap();
                serde_json::json!({"type": "record", "name": name, "namespace": RESERVED_NAMESPACE, "fields": []})
            }
            DataType::Record(r) => {
                let full_name = r.full_name();
                if !seen.insert(full_name.clone()) {
                    return full_name.into();
                }
                let mut obj = Map::new();
                named_header(
                    &mut obj,
                    "record",
                    &r.name,
                    &r.namespace,
                    enclosing_namespace,
                    &r.doc,
                    &r.aliases,
                );
                let fields: Vec<Value> = r
                    .fields
                    .iter()
                    .map(|f| {
                        let mut fobj = Map::new();
                        fobj.insert("name".to_owned(), f.name.as_str().into());
                        fobj.insert(
                            "type".to_owned(),
                            f.data_type.write_avro(seen, &r.namespace),
                        );
                        if let Some(doc) = &f.doc {
                            fobj.insert("doc".to_owned(), doc.as_str().into());
                        }
                        if let Some(default) = &f.default {
                            fobj.insert("default".to_owned(), default.clone());
                        }
                        if !f.aliases.is_empty() {
                            fobj.insert("aliases".to_owned(), f.aliases.clone().into());
                        }
                        for (k, v) in f.attributes.iter() {
                            fobj.insert(k.clone(), v.clone());
                        }
                        Value::Object(fobj)
                    })
                    .collect();
                obj.insert("fields".to_owned(), fields.into());
                for (k, v) in r.attributes.iter() {
                    obj.insert(k.clone(), v.clone());
                }
                Value::Object(obj)
            }
            DataType::Enum(e) => {
                let full_name = e.full_name();
                if !seen.insert(full_name.clone()) {
                    return full_name.into();
                }
                let mut obj = Map::new();
                named_header(
                    &mut obj,
                    "enum",
                    &e.name,
                    &e.namespace,
                    enclosing_namespace,
                    &e.doc,
                    &e.aliases,
                );
                obj.insert("symbols".to_owned(), e.symbols.clone().into());
                if let Some(default) = &e.default {
                    obj.insert("default".to_owned(), default.as_str().into());
                }
                for (k, v) in e.attributes.iter() {
                    obj.insert(k.clone(), v.clone());
                }
                Value::Object(obj)
            }
        }
    }

    fn write_pcf(&self, seen: &mut HashSet<String>, out: &mut String) {
        fn quote(s: &str) -> String {
            serde_json::to_string(s).unwrap()
        }
        match self {
            DataType::Null => out.push_str("\"null\""),
            DataType::Boolean => out.push_str("\"boolean\""),
            DataType::Int => out.push_str("\"int\""),
            DataType::Long => out.push_str("\"long\""),
            DataType::Float => out.push_str("\"float\""),
            DataType::Double => out.push_str("\"double\""),
            DataType::String => out.push_str("\"string\""),
            DataType::Bytes => out.push_str("\"bytes\""),
            DataType::Array(items) => {
                out.push_str("{\"type\":\"array\",\"items\":");
                items.write_pcf(seen, out);
                out.push('}');
            }
            DataType::Map(values) => {
                out.push_str("{\"type\":\"map\",\"values\":");
                values.write_pcf(seen, out);
                out.push('}');
            }
            DataType::Optional { inner, null_first } => {
                out.push('[');
                if *null_first {
                    out.push_str("\"null\",");
                    inner.write_pcf(seen, out);
                } else {
                    inner.write_pcf(seen, out);
                    out.push_str(",\"null\"");
                }
                out.push(']');
            }
            DataType::Any | DataType::Image | DataType::Record(_) => {
                let full_name = self.full_name().unwrap();
                if !seen.insert(full_name.clone()) {
                    out.push_str(&quote(&full_name));
                    return;
                }
                out.push_str(&format!(
                    "{{\"name\":{},\"type\":\"record\",\"fields\":[",
                    quote(&full_name)
                ));
                if let DataType::Record(r) = self {
                    for (i, f) in r.fields.iter().enumerate() {
                        if i > 0 {
                            out.push(',');
                        }
                        out.push_str(&format!("{{\"name\":{},\"type\":", quote(&f.name)));
                        f.data_type.write_pcf(seen, out);
                        out.push('}');
                    }
                }
                out.push_str("]}");
            }
            DataType::Enum(e) => {
                let full_name = e.full_name();
                if !seen.insert(full_name.clone()) {
                    out.push_str(&quote(&full_name));
                    return;
                }
                let symbols: Vec<String> = e.symbols.iter().map(|s| quote(s)).collect();
                out.push_str(&format!(
                    "{{\"name\":{},\"type\":\"enum\",\"symbols\":[{}]}}",
                    quote(&full_name),
                    symbols.join(",")
                ));
            }
        }
    }
}

/// PCF の文字列から指紋を作る
pub fn fingerprint_of_canonical_form(pcf: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(pcf.as_bytes());
    let mut s = String::with_capacity(7 + 64);
    s.push_str("sha256:");
    for b in digest.iter() {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

const PRIMITIVES: [&str; 8] = [
    "null", "boolean", "int", "long", "float", "double", "string", "bytes",
];

fn primitive(name: &str) -> Option<DataType> {
    Some(match name {
        "null" => DataType::Null,
        "boolean" => DataType::Boolean,
        "int" => DataType::Int,
        "long" => DataType::Long,
        "float" => DataType::Float,
        "double" => DataType::Double,
        "string" => DataType::String,
        "bytes" => DataType::Bytes,
        _ => return None,
    })
}

#[derive(Default)]
struct Parser {
    defined: HashMap<String, DataType>,
    in_progress: HashSet<String>,
}

fn opt_string(obj: &Map<String, Value>, key: &str, path: &str) -> Result<Option<String>> {
    match obj.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(schema_invalid(path, format!("'{key}' must be a string"))),
    }
}

fn string_list(obj: &Map<String, Value>, key: &str, path: &str) -> Result<Vec<String>> {
    match obj.get(key) {
        None => Ok(Vec::new()),
        Some(Value::Array(arr)) => arr
            .iter()
            .map(|v| {
                v.as_str().map(|s| s.to_owned()).ok_or_else(|| {
                    schema_invalid(path, format!("'{key}' must be a list of strings"))
                })
            })
            .collect(),
        Some(_) => Err(schema_invalid(
            path,
            format!("'{key}' must be a list of strings"),
        )),
    }
}

fn other_attributes(obj: &Map<String, Value>, known: &[&str]) -> Map<String, Value> {
    obj.iter()
        .filter(|(k, _)| !known.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

impl Parser {
    fn parse(
        &mut self,
        v: &Value,
        enclosing_namespace: &str,
        path: &str,
        top: bool,
    ) -> Result<DataType> {
        let ty = match v {
            Value::String(name) => self.parse_name(name, enclosing_namespace, path)?,
            Value::Array(branches) => self.parse_union(branches, enclosing_namespace, path)?,
            Value::Object(obj) => self.parse_object(obj, enclosing_namespace, path, top)?,
            _ => return Err(schema_invalid(path, format!("{v} is not a schema"))),
        };
        if ty == DataType::Image && !top {
            return Err(schema_invalid(
                path,
                "image can be used only as the type of an argument or a return value itself",
            ));
        }
        Ok(ty)
    }

    fn parse_name(&self, name: &str, enclosing_namespace: &str, path: &str) -> Result<DataType> {
        if let Some(p) = primitive(name) {
            return Ok(p);
        }
        match name {
            "record" | "enum" | "array" | "map" => {
                return Err(schema_invalid(
                    path,
                    format!("'{name}' must be written as an object"),
                ))
            }
            "fixed" | "error" => {
                return Err(schema_invalid(path, format!("'{name}' is not supported")))
            }
            _ => {}
        }
        let mut candidates = vec![];
        if !name.contains('.') && !enclosing_namespace.is_empty() {
            candidates.push(join_full_name(enclosing_namespace, name));
        }
        candidates.push(name.to_owned());
        for full_name in candidates.iter() {
            if self.in_progress.contains(full_name) {
                return Err(schema_invalid(
                    path,
                    format!("recursive type '{full_name}' is not supported"),
                ));
            }
            if let Some(t) = self.defined.get(full_name) {
                return Ok(t.clone());
            }
            if full_name == ANY_FULL_NAME {
                return Ok(DataType::Any);
            }
            if full_name == IMAGE_FULL_NAME {
                return Ok(DataType::Image);
            }
        }
        Err(schema_invalid(path, format!("unknown type '{name}'")))
    }

    fn parse_union(
        &mut self,
        branches: &[Value],
        enclosing_namespace: &str,
        path: &str,
    ) -> Result<DataType> {
        let is_null = |v: &Value| match v {
            Value::String(s) => s == "null",
            Value::Object(o) => o.len() == 1 && o.get("type") == Some(&Value::from("null")),
            _ => false,
        };
        let err = || {
            schema_invalid(
                path,
                "only a union of null and one other type (T?) is supported",
            )
        };
        if branches.len() != 2 {
            return Err(err());
        }
        let (null_first, other) = match (is_null(&branches[0]), is_null(&branches[1])) {
            (true, false) => (true, &branches[1]),
            (false, true) => (false, &branches[0]),
            _ => return Err(err()),
        };
        if other.is_array() {
            return Err(err());
        }
        let inner = self.parse(other, enclosing_namespace, path, false)?;
        Ok(DataType::Optional {
            inner: Box::new(inner),
            null_first,
        })
    }

    fn parse_object(
        &mut self,
        obj: &Map<String, Value>,
        enclosing_namespace: &str,
        path: &str,
        top: bool,
    ) -> Result<DataType> {
        if obj.contains_key("logicalType") {
            return Err(schema_invalid(path, "logical types are not supported"));
        }
        let type_value = obj
            .get("type")
            .ok_or_else(|| schema_invalid(path, "'type' is missing"))?;
        let type_name = match type_value {
            Value::String(s) => s.as_str(),
            // {"type": {...}} は中の型として読む
            other => return self.parse(other, enclosing_namespace, path, top),
        };
        match type_name {
            "record" => self.parse_record(obj, enclosing_namespace, path, top),
            "enum" => self.parse_enum(obj, enclosing_namespace, path),
            "array" => {
                let items = obj
                    .get("items")
                    .ok_or_else(|| schema_invalid(path, "'items' is missing"))?;
                Ok(DataType::array(self.parse(
                    items,
                    enclosing_namespace,
                    &format!("{path}[]"),
                    false,
                )?))
            }
            "map" => {
                let values = obj
                    .get("values")
                    .ok_or_else(|| schema_invalid(path, "'values' is missing"))?;
                Ok(DataType::map(self.parse(
                    values,
                    enclosing_namespace,
                    &format!("{path}{{}}"),
                    false,
                )?))
            }
            "fixed" | "error" => Err(schema_invalid(
                path,
                format!("'{type_name}' is not supported"),
            )),
            // {"type": "long", ...} の制約の属性 (仕様 §10) は今は持たない (Step 13 #201 で持つ)
            name if PRIMITIVES.contains(&name) => Ok(primitive(name).unwrap()),
            name => self.parse_name(name, enclosing_namespace, path),
        }
    }

    /// 名前付き型の (完全名, 名前空間, 短い名前)
    fn named(
        &self,
        obj: &Map<String, Value>,
        enclosing_namespace: &str,
        path: &str,
    ) -> Result<(String, String, String)> {
        let name = opt_string(obj, "name", path)?
            .ok_or_else(|| schema_invalid(path, "'name' is missing"))?;
        let (namespace, short) = if let Some((ns, short)) = name.rsplit_once('.') {
            (ns.to_owned(), short.to_owned())
        } else {
            match opt_string(obj, "namespace", path)? {
                Some(ns) => (ns, name.clone()),
                None => (enclosing_namespace.to_owned(), name.clone()),
            }
        };
        validate_type_name(&short, path)?;
        validate_namespace_str(&namespace, path)?;
        let full_name = join_full_name(&namespace, &short);
        if self.defined.contains_key(&full_name) || self.in_progress.contains(&full_name) {
            return Err(schema_invalid(
                path,
                format!("named type '{full_name}' is defined twice"),
            ));
        }
        Ok((full_name, namespace, short))
    }

    fn parse_record(
        &mut self,
        obj: &Map<String, Value>,
        enclosing_namespace: &str,
        path: &str,
        top: bool,
    ) -> Result<DataType> {
        let (full_name, namespace, name) = self.named(obj, enclosing_namespace, path)?;
        let path = if path.is_empty() {
            name.clone()
        } else {
            path.to_owned()
        };
        let path = path.as_str();
        let reserved = namespace == RESERVED_NAMESPACE
            || namespace.starts_with(&format!("{RESERVED_NAMESPACE}."));
        if reserved {
            let t = match full_name.as_str() {
                ANY_FULL_NAME => DataType::Any,
                IMAGE_FULL_NAME => DataType::Image,
                _ => {
                    return Err(schema_invalid(
                        path,
                        format!("namespace 'juiz' is reserved ('{full_name}')"),
                    ))
                }
            };
            if obj
                .get("fields")
                .and_then(|f| f.as_array())
                .map(|f| f.is_empty())
                != Some(true)
            {
                return Err(schema_invalid(
                    path,
                    format!("'{full_name}' must have no fields"),
                ));
            }
            if t == DataType::Image && !top {
                return Err(schema_invalid(
                    path,
                    "image can be used only as the type of an argument or a return value itself",
                ));
            }
            self.defined.insert(full_name, t.clone());
            return Ok(t);
        }
        let fields_value = obj
            .get("fields")
            .and_then(|f| f.as_array())
            .ok_or_else(|| schema_invalid(path, "'fields' must be a list"))?;
        self.in_progress.insert(full_name.clone());
        let mut fields = Vec::new();
        for fv in fields_value.iter() {
            let fobj = fv
                .as_object()
                .ok_or_else(|| schema_invalid(path, "a field must be an object"))?;
            let fname = opt_string(fobj, "name", path)?
                .ok_or_else(|| schema_invalid(path, "a field has no 'name'"))?;
            let fpath = format!("{path}.{fname}");
            validate_type_name(&fname, &fpath)?;
            if fields.iter().any(|f: &RecordField| f.name == fname) {
                return Err(schema_invalid(
                    &fpath,
                    format!("field '{fname}' is defined twice"),
                ));
            }
            let ftype = fobj
                .get("type")
                .ok_or_else(|| schema_invalid(&fpath, "'type' is missing"))?;
            let data_type = self.parse(ftype, &namespace, &fpath, false)?;
            let default = fobj.get("default").cloned();
            if let Some(d) = default.as_ref() {
                check_avro_default(&data_type, d, &fpath)?;
            }
            fields.push(RecordField {
                name: fname,
                doc: opt_string(fobj, "doc", &fpath)?,
                aliases: string_list(fobj, "aliases", &fpath)?,
                default,
                data_type,
                attributes: other_attributes(fobj, &["name", "type", "doc", "default", "aliases"]),
            });
        }
        self.in_progress.remove(&full_name);
        let t = DataType::Record(RecordType {
            name,
            namespace,
            doc: opt_string(obj, "doc", path)?,
            aliases: string_list(obj, "aliases", path)?,
            fields,
            attributes: other_attributes(
                obj,
                &["type", "name", "namespace", "doc", "aliases", "fields"],
            ),
        });
        self.defined.insert(full_name, t.clone());
        Ok(t)
    }

    fn parse_enum(
        &mut self,
        obj: &Map<String, Value>,
        enclosing_namespace: &str,
        path: &str,
    ) -> Result<DataType> {
        let (full_name, namespace, name) = self.named(obj, enclosing_namespace, path)?;
        let path = if path.is_empty() {
            name.clone()
        } else {
            path.to_owned()
        };
        let path = path.as_str();
        if namespace == RESERVED_NAMESPACE
            || namespace.starts_with(&format!("{RESERVED_NAMESPACE}."))
        {
            return Err(schema_invalid(
                path,
                format!("namespace 'juiz' is reserved ('{full_name}')"),
            ));
        }
        let symbols = string_list(obj, "symbols", path)?;
        if symbols.is_empty() {
            return Err(schema_invalid(path, "'symbols' must not be empty"));
        }
        let mut uniq = HashSet::new();
        for s in symbols.iter() {
            validate_type_name(s, path)?;
            if !uniq.insert(s) {
                return Err(schema_invalid(
                    path,
                    format!("symbol '{s}' is defined twice"),
                ));
            }
        }
        let default = opt_string(obj, "default", path)?;
        if let Some(d) = default.as_ref() {
            if !symbols.contains(d) {
                return Err(schema_invalid(
                    path,
                    format!("default '{d}' is not a symbol"),
                ));
            }
        }
        let t = DataType::Enum(EnumType {
            name,
            namespace,
            doc: opt_string(obj, "doc", path)?,
            aliases: string_list(obj, "aliases", path)?,
            symbols,
            default,
            attributes: other_attributes(
                obj,
                &[
                    "type",
                    "name",
                    "namespace",
                    "doc",
                    "aliases",
                    "symbols",
                    "default",
                ],
            ),
        });
        self.defined.insert(full_name, t.clone());
        Ok(t)
    }
}

/// フィールドの既定値 (Avro の JSON 表記) を検査する
fn check_avro_default(t: &DataType, v: &Value, path: &str) -> Result<()> {
    let bad = || {
        schema_invalid(
            path,
            format!("default {v} does not match {}", t.display_name()),
        )
    };
    let ok = match t {
        DataType::Null => v.is_null(),
        DataType::Boolean => v.is_boolean(),
        DataType::Int => v
            .as_i64()
            .map(|i| i32::try_from(i).is_ok())
            .unwrap_or(false),
        DataType::Long => v.is_i64(),
        DataType::Float | DataType::Double => v.is_number(),
        DataType::String => v.is_string(),
        DataType::Bytes => v
            .as_str()
            .map(|s| s.chars().all(|c| (c as u32) <= 0xFF))
            .unwrap_or(false),
        DataType::Array(items) => match v.as_array() {
            Some(arr) => {
                for (i, e) in arr.iter().enumerate() {
                    check_avro_default(items, e, &format!("{path}[{i}]"))?;
                }
                true
            }
            None => false,
        },
        DataType::Map(values) => match v.as_object() {
            Some(m) => {
                for (k, e) in m.iter() {
                    check_avro_default(values, e, &format!("{path}.{k}"))?;
                }
                true
            }
            None => false,
        },
        DataType::Optional { inner, null_first } => {
            // Avro の union の既定値は最初の型で書く
            if *null_first {
                if !v.is_null() {
                    return Err(schema_invalid(
                        path,
                        format!(
                            "default of [\"null\", T] must be null (write [T, \"null\"] for {v})"
                        ),
                    ));
                }
                true
            } else {
                check_avro_default(inner, v, path)?;
                true
            }
        }
        DataType::Record(r) => match v.as_object() {
            Some(m) => {
                for f in r.fields.iter() {
                    let fpath = format!("{path}.{}", f.name);
                    match m.get(&f.name) {
                        Some(e) => check_avro_default(&f.data_type, e, &fpath)?,
                        None if f.default.is_some() => {}
                        None => {
                            return Err(schema_invalid(
                                &fpath,
                                "default has no value for this field",
                            ))
                        }
                    }
                }
                true
            }
            None => false,
        },
        DataType::Enum(e) => v
            .as_str()
            .map(|s| e.symbols.iter().any(|x| x == s))
            .unwrap_or(false),
        DataType::Any => true,
        DataType::Image => false,
    };
    if ok {
        Ok(())
    } else {
        Err(bad())
    }
}

/// Avro の JSON 表記の既定値を辞書の表記 (仕様 §4) にする。検査済みの値を渡すこと
pub(crate) fn avro_default_to_dict(t: &DataType, v: &Value) -> Value {
    match (t, v) {
        (DataType::Bytes, Value::String(s)) => {
            let bytes: Vec<u8> = s.chars().map(|c| c as u32 as u8).collect();
            base64::engine::general_purpose::STANDARD
                .encode(bytes)
                .into()
        }
        (DataType::Array(items), Value::Array(arr)) => {
            arr.iter().map(|e| avro_default_to_dict(items, e)).collect()
        }
        (DataType::Map(values), Value::Object(m)) => Value::Object(
            m.iter()
                .map(|(k, e)| (k.clone(), avro_default_to_dict(values, e)))
                .collect(),
        ),
        (DataType::Optional { inner, .. }, v) if !v.is_null() => avro_default_to_dict(inner, v),
        (DataType::Record(r), Value::Object(m)) => Value::Object(
            m.iter()
                .map(|(k, e)| match r.get_field(k) {
                    Some(f) => (k.clone(), avro_default_to_dict(&f.data_type, e)),
                    None => (k.clone(), e.clone()),
                })
                .collect(),
        ),
        _ => v.clone(),
    }
}

/// 辞書の表記 (仕様 §4) の既定値を Avro の JSON 表記にする
pub(crate) fn dict_to_avro_default(t: &DataType, v: &Value, path: &str) -> Result<Value> {
    crate::value_check::read_value(t, v, path)?;
    Ok(match (t, v) {
        (DataType::Bytes, Value::String(s)) => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(s)
                .map_err(|e| schema_invalid(path, format!("bytes default is not base64: {e}")))?;
            bytes
                .into_iter()
                .map(|b| b as char)
                .collect::<String>()
                .into()
        }
        (DataType::Array(items), Value::Array(arr)) => arr
            .iter()
            .enumerate()
            .map(|(i, e)| dict_to_avro_default(items, e, &format!("{path}[{i}]")))
            .collect::<Result<Vec<Value>>>()?
            .into(),
        (DataType::Map(values), Value::Object(m)) => {
            let mut out = Map::new();
            for (k, e) in m.iter() {
                out.insert(
                    k.clone(),
                    dict_to_avro_default(values, e, &format!("{path}.{k}"))?,
                );
            }
            Value::Object(out)
        }
        (DataType::Optional { inner, null_first }, v) => {
            if v.is_null() {
                Value::Null
            } else if *null_first {
                return Err(schema_invalid(
                    path,
                    format!("default of [\"null\", T] must be null (write [T, \"null\"] for {v})"),
                ));
            } else {
                dict_to_avro_default(inner, v, path)?
            }
        }
        (DataType::Record(r), Value::Object(m)) => {
            let mut out = Map::new();
            for f in r.fields.iter() {
                if let Some(e) = m.get(&f.name) {
                    out.insert(
                        f.name.clone(),
                        dict_to_avro_default(&f.data_type, e, &format!("{path}.{}", f.name))?,
                    );
                }
            }
            Value::Object(out)
        }
        _ => v.clone(),
    })
}
