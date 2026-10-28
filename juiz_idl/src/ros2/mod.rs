//! ROS 2 の型 (`.msg` / `.srv` / `.idl`) を JIDL に取り込む (issue #201、仕様 docs/spec/datatype.md §10・§25)
//!
//! 型ごとに 1 つの .jidl を `ros2/<package>/msg/<Name>.jidl`・`ros2/<package>/srv/<Name>.jidl` に書き出す。
//! 名前空間は `ros2.<package>.msg` / `ros2.<package>.srv`。参照している型は相対パスで import する。
//! juiz の型にない性質は近い型に寄せ、制約は属性 (`@juiz.maxLength(…)` など、指紋に入らない) として残す。
//!
//! 辞書の形が juiz_std と同じ標準メッセージ (`std_msgs/Header`、`geometry_msgs/Pose` など) は、
//! 既定で juiz_std の型に置き換える ([`STD_MAP`]、`std_map: false` で置き換えない)。

mod idl;
mod msg;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use base64::Engine;
use juiz_types::{
    DataType, RecordField, RecordType, ATTR_MAXIMUM, ATTR_MAX_ITEMS, ATTR_MAX_LENGTH, ATTR_MINIMUM,
    ATTR_MIN_ITEMS,
};
use serde_json::{json, Map, Value};

use crate::constants::ATTR_CONSTANTS;
use crate::jidl::{declaration, doc_comment, jidl_name, type_expr};
use crate::{load_files, GeneratedFile, IdlError, IdlModel};

/// 元の ROS 2 の型の名前を残す属性
pub const ATTR_ROS_TYPE: &str = "juiz.ros_type";
/// 取り込んだ型の名前空間の先頭 (`ros2.<package>.msg`)
pub const ROS2_NAMESPACE: &str = "ros2";

/// juiz_std に置き換える ROS 2 の型 (パッケージ、名前、組み込みの JIDL、juiz_std の完全名)。
/// フィールドの名前と辞書の形が同じものだけ (整数の幅は違ってもよい)
pub const STD_MAP: [(&str, &str, &str, &str); 12] = [
    (
        "builtin_interfaces",
        "Time",
        "juiz_std/common.jidl",
        "juiz_std.common.Time",
    ),
    (
        "builtin_interfaces",
        "Duration",
        "juiz_std/common.jidl",
        "juiz_std.common.Duration",
    ),
    (
        "std_msgs",
        "Header",
        "juiz_std/common.jidl",
        "juiz_std.common.Header",
    ),
    (
        "geometry_msgs",
        "Point",
        "juiz_std/geometry.jidl",
        "juiz_std.geometry.Point3D",
    ),
    (
        "geometry_msgs",
        "Vector3",
        "juiz_std/geometry.jidl",
        "juiz_std.geometry.Vector3",
    ),
    (
        "geometry_msgs",
        "Quaternion",
        "juiz_std/geometry.jidl",
        "juiz_std.geometry.Quaternion",
    ),
    (
        "geometry_msgs",
        "Pose",
        "juiz_std/geometry.jidl",
        "juiz_std.geometry.Pose",
    ),
    (
        "geometry_msgs",
        "Twist",
        "juiz_std/geometry.jidl",
        "juiz_std.geometry.Twist",
    ),
    (
        "geometry_msgs",
        "PoseStamped",
        "juiz_std/geometry.jidl",
        "juiz_std.geometry.PoseStamped",
    ),
    (
        "sensor_msgs",
        "Imu",
        "juiz_std/sensor.jidl",
        "juiz_std.sensor.Imu",
    ),
    (
        "sensor_msgs",
        "LaserScan",
        "juiz_std/sensor.jidl",
        "juiz_std.sensor.LaserScan",
    ),
    (
        "sensor_msgs",
        "Range",
        "juiz_std/sensor.jidl",
        "juiz_std.sensor.Range",
    ),
];

/// ROS 2 の基本型 (`.msg` の書き方。`long double` は `.idl` だけ)
const PRIMITIVES: [&str; 15] = [
    "bool",
    "byte",
    "char",
    "wchar",
    "float32",
    "float64",
    "int8",
    "uint8",
    "int16",
    "uint16",
    "int32",
    "uint32",
    "int64",
    "uint64",
    "long double",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SubFolder {
    Msg,
    Srv,
}

impl SubFolder {
    fn as_str(&self) -> &'static str {
        match self {
            SubFolder::Msg => "msg",
            SubFolder::Srv => "srv",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Base {
    Primitive(String),
    String { wide: bool, bound: Option<u64> },
    Named { package: String, name: String },
}

#[derive(Clone, Debug, PartialEq)]
enum ArrayKind {
    None,
    Unbounded,
    Fixed(u64),
    Bounded(u64),
}

#[derive(Clone, Debug, PartialEq)]
struct FieldType {
    base: Base,
    array: ArrayKind,
}

#[derive(Clone, Debug)]
struct RosField {
    name: String,
    ty: FieldType,
    default: Option<Value>,
    doc: Option<String>,
}

#[derive(Clone, Debug)]
struct Constant {
    name: String,
    ty: FieldType,
    value: Value,
    doc: Option<String>,
}

#[derive(Clone, Debug)]
struct Message {
    package: String,
    sub: SubFolder,
    name: String,
    doc: Option<String>,
    fields: Vec<RosField>,
    constants: Vec<Constant>,
}

#[derive(Clone, Debug)]
struct Service {
    package: String,
    name: String,
    doc: Option<String>,
    request: Message,
    response: Message,
}

#[derive(Clone, Debug)]
enum Interface {
    Message(Message),
    Service(Service),
}

/// 取り込み方
#[derive(Clone, Debug)]
pub struct Ros2ImportOptions {
    /// 参照しているメッセージの探し先。`<dir>/<package>/msg/<Name>.msg` (なければ `.idl`) を探す
    pub search_dirs: Vec<PathBuf>,
    /// 辞書の形が同じ標準メッセージを juiz_std の型に置き換える ([`STD_MAP`])
    pub std_map: bool,
}

impl Default for Ros2ImportOptions {
    fn default() -> Self {
        Self {
            search_dirs: Vec::new(),
            std_map: true,
        }
    }
}

/// ROS 2 の環境の場所を足す環境変数 (ament のインストール先の並び。`<prefix>/share` を探す)
pub const AMENT_PREFIX_PATH_ENV: &str = "AMENT_PREFIX_PATH";

impl Ros2ImportOptions {
    /// 環境変数 `AMENT_PREFIX_PATH` の各インストール先の `share` を探し先の後ろに足す
    pub fn with_ament_prefix_path(mut self) -> Self {
        if let Some(v) = std::env::var_os(AMENT_PREFIX_PATH_ENV) {
            self.search_dirs.extend(
                std::env::split_paths(&v)
                    .filter(|p| !p.as_os_str().is_empty())
                    .map(|p| p.join("share")),
            );
        }
        self
    }
}

/// 取り込みの結果
#[derive(Clone, Debug, Default)]
pub struct Ros2Import {
    /// 書き出す .jidl (出力先のディレクトリからの相対パス)。入力と、参照していて探し先から読んだ型
    pub files: Vec<GeneratedFile>,
    /// 警告 (`uint64`、juiz_std に置き換えたので書き出さない入力など)
    pub warnings: Vec<String>,
}

type Key = (String, String);

#[derive(Default)]
struct Importer {
    options: Ros2ImportOptions,
    /// 読んだメッセージ ((package, name) → メッセージ)
    messages: HashMap<Key, Message>,
    services: Vec<Service>,
    /// 型にしたメッセージ
    types: HashMap<Key, DataType>,
    /// 型にしている途中 (再帰を見つける)
    visiting: Vec<Key>,
    /// 書き出すメッセージ (読んだ順)
    order: Vec<Key>,
    std_model: Option<IdlModel>,
    warnings: Vec<String>,
}

/// ROS 2 の `.msg` / `.srv` / `.idl` を読み、JIDL のファイルにする。
///
/// `.msg` / `.srv` はパッケージ名を置き場所 (`<package>/msg/<Name>.msg`) から決める。
/// `.idl` は中の `module <package> { module msg { … } }` から決める
pub fn import_ros2<P: AsRef<Path>>(
    paths: &[P],
    options: &Ros2ImportOptions,
) -> Result<Ros2Import, IdlError> {
    let mut im = Importer {
        options: options.clone(),
        ..Default::default()
    };
    let mut inputs: Vec<Key> = Vec::new();
    // 入力の .msg / .srv と同じパッケージの並び (<dir>/<package>/msg/) も探す
    let mut implicit = Vec::new();
    for p in paths {
        let p = p.as_ref();
        if matches!(p.extension().and_then(|e| e.to_str()), Some("msg" | "srv")) {
            if let Some(d) = p.parent().and_then(|d| d.parent()).and_then(|d| d.parent()) {
                let d = if d.as_os_str().is_empty() {
                    Path::new(".")
                } else {
                    d
                };
                if !implicit.iter().any(|x: &PathBuf| x == d) {
                    implicit.push(d.to_path_buf());
                }
            }
        }
    }
    im.options.search_dirs = implicit
        .into_iter()
        .chain(options.search_dirs.iter().cloned())
        .collect();
    for p in paths {
        for i in read_file(p.as_ref())? {
            match i {
                Interface::Message(m) => {
                    let key = (m.package.clone(), m.name.clone());
                    if im.std_type(&key.0, &key.1).is_some() {
                        let (_, _, _, full) = std_entry(&key.0, &key.1).unwrap();
                        im.warnings.push(format!(
                            "{}/msg/{} is not written because it is replaced with {full} (use --no-std-map to import it)",
                            key.0, key.1
                        ));
                        continue;
                    }
                    im.messages.insert(key.clone(), m);
                    inputs.push(key);
                }
                Interface::Service(s) => im.services.push(s),
            }
        }
    }
    for key in inputs.iter() {
        im.message_type(key)?;
    }
    let services = std::mem::take(&mut im.services);
    let mut files = Vec::new();
    for key in im.order.clone().iter() {
        files.push(im.message_file(key)?);
    }
    for s in services.iter() {
        files.push(im.service_file(s)?);
    }
    Ok(Ros2Import {
        files,
        warnings: im.warnings,
    })
}

fn io_error(path: &Path, e: impl std::fmt::Display) -> IdlError {
    IdlError::Io {
        path: path.display().to_string(),
        message: e.to_string(),
    }
}

fn read_file(path: &Path) -> Result<Vec<Interface>, IdlError> {
    let text = std::fs::read_to_string(path).map_err(|e| io_error(path, e))?;
    let file = path.display().to_string();
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext == "idl" {
        return idl::parse_ros_idl(&text, &file);
    }
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| io_error(path, "invalid file name"))?;
    let parent = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|p| p.to_str());
    let package = path
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .and_then(|p| p.to_str());
    let (Some(sub), Some(package)) = (parent, package) else {
        return Err(io_error(
            path,
            "put the file at <package>/msg/<Name>.msg or <package>/srv/<Name>.srv",
        ));
    };
    match (ext, sub) {
        ("msg", "msg") => Ok(vec![Interface::Message(msg::parse_msg(
            &text, package, name, &file,
        )?)]),
        ("srv", "srv") => Ok(vec![Interface::Service(msg::parse_srv(
            &text, package, name, &file,
        )?)]),
        ("msg" | "srv", _) => Err(io_error(
            path,
            "put the file at <package>/msg/<Name>.msg or <package>/srv/<Name>.srv",
        )),
        ("action", _) => Err(io_error(path, "ROS 2 actions are not supported")),
        _ => Err(io_error(path, "expected a .msg, .srv or .idl file")),
    }
}

fn std_entry(
    package: &str,
    name: &str,
) -> Option<(&'static str, &'static str, &'static str, &'static str)> {
    STD_MAP
        .iter()
        .find(|(p, n, _, _)| *p == package && *n == name)
        .copied()
}

fn namespace_of(package: &str, sub: SubFolder) -> String {
    format!("{ROS2_NAMESPACE}.{package}.{}", sub.as_str())
}

/// `AddTwoInts` → `add_two_ints`、`GetURL` → `get_url`
fn snake_case(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::new();
    for (i, c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() {
            let prev = i.checked_sub(1).map(|j| chars[j]);
            let next = chars.get(i + 1);
            let boundary = match prev {
                Some(p) if p.is_ascii_lowercase() || p.is_ascii_digit() => true,
                Some(p) if p.is_ascii_uppercase() => next.is_some_and(|n| n.is_ascii_lowercase()),
                _ => false,
            };
            if boundary && !out.ends_with('_') {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(*c);
        }
    }
    out
}

/// 基本型の juiz の型・範囲・元の型名を残すか
fn primitive(p: &str) -> (DataType, Option<(i128, i128)>, bool) {
    match p {
        "bool" => (DataType::Boolean, None, false),
        "byte" | "char" | "uint8" => (DataType::Int, Some((0, 255)), true),
        "int8" => (DataType::Int, Some((-128, 127)), true),
        "int16" => (DataType::Int, Some((-32768, 32767)), true),
        "uint16" | "wchar" => (DataType::Int, Some((0, 65535)), true),
        "int32" => (DataType::Int, None, false),
        "uint32" => (DataType::Long, Some((0, u32::MAX as i128)), true),
        "int64" => (DataType::Long, None, true),
        "uint64" => (DataType::Long, Some((0, i64::MAX as i128)), true), // 上限は long の上限と同じ (書かない)
        "float32" => (DataType::Float, None, false),
        "long double" => (DataType::Double, None, true),
        _ => (DataType::Double, None, false),
    }
}

fn zero_of(t: &DataType) -> Value {
    match t {
        DataType::Boolean => Value::Bool(false),
        DataType::Int | DataType::Long => json!(0),
        DataType::Float | DataType::Double => json!(0.0),
        DataType::String | DataType::Bytes => json!(""),
        DataType::Array(_) => json!([]),
        DataType::Record(r) => record_default(r),
        _ => Value::Null,
    }
}

/// レコードの既定値 (辞書の表し方)。全フィールドに既定値がある前提
fn record_default(r: &RecordType) -> Value {
    Value::Object(
        r.fields
            .iter()
            .map(|f| (f.name.clone(), f.default_as_dict().unwrap_or(Value::Null)))
            .collect(),
    )
}

fn type_name_of(t: &DataType) -> &'static str {
    match t {
        DataType::Boolean => "boolean",
        DataType::Int => "int",
        DataType::Long => "long",
        DataType::Float => "float",
        DataType::Double => "double",
        _ => "string",
    }
}

impl Importer {
    fn std_type(&mut self, package: &str, name: &str) -> Option<DataType> {
        if !self.options.std_map {
            return None;
        }
        let (_, _, _, full) = std_entry(package, name)?;
        if self.std_model.is_none() {
            let files: Vec<&str> = BUILTIN_STD_FILES.to_vec();
            self.std_model =
                Some(load_files(&files).expect("the builtin juiz_std types must load"));
        }
        self.std_model.as_ref().unwrap().get_type(full).cloned()
    }

    /// 探し先から `<package>/msg/<name>` を読む
    fn find_message(&mut self, key: &Key, from: &str) -> Result<(), IdlError> {
        for dir in self.options.search_dirs.clone() {
            for ext in ["msg", "idl"] {
                let path = dir
                    .join(&key.0)
                    .join("msg")
                    .join(format!("{}.{ext}", key.1));
                if !path.is_file() {
                    continue;
                }
                for i in read_file(&path)? {
                    if let Interface::Message(m) = i {
                        if m.package == key.0 && m.name == key.1 {
                            self.messages.insert(key.clone(), m);
                            return Ok(());
                        }
                    }
                }
            }
        }
        Err(IdlError::Generate {
            message: format!(
                "{from}: message {}/msg/{} is not found. Give its file as an input, \
                 add the directory that has {}/msg/ with -I, or set {AMENT_PREFIX_PATH_ENV} (source the ROS 2 setup script)",
                key.0, key.1, key.0
            ),
        })
    }

    fn message_type(&mut self, key: &Key) -> Result<DataType, IdlError> {
        if let Some(t) = self.types.get(key) {
            return Ok(t.clone());
        }
        if self.visiting.contains(key) {
            return Err(IdlError::Generate {
                message: format!(
                    "message {}/msg/{} refers to itself (recursive types are not supported)",
                    key.0, key.1
                ),
            });
        }
        let m = self.messages.get(key).cloned().expect("loaded before");
        self.visiting.push(key.clone());
        let t = self.record_of(&m)?;
        self.visiting.pop();
        self.types.insert(key.clone(), t.clone());
        self.order.push(key.clone());
        Ok(t)
    }

    /// フィールドの型が参照するメッセージの型 (juiz_std に置き換えたものも)
    fn named_type(&mut self, package: &str, name: &str, from: &str) -> Result<DataType, IdlError> {
        if let Some(t) = self.std_type(package, name) {
            return Ok(t);
        }
        let key = (package.to_owned(), name.to_owned());
        if !self.messages.contains_key(&key) {
            self.find_message(&key, from)?;
        }
        self.message_type(&key)
    }

    fn record_of(&mut self, m: &Message) -> Result<DataType, IdlError> {
        let display = format!("{}/{}/{}", m.package, m.sub.as_str(), m.name);
        let mut record = RecordType::new(&namespace_of(&m.package, m.sub), &m.name);
        record.doc = m.doc.clone();
        for f in m.fields.iter() {
            let field = self.field_of(f, &display)?;
            record = record.field(field);
        }
        if !m.constants.is_empty() {
            let mut constants = Vec::new();
            for c in m.constants.iter() {
                let mut obj = Map::new();
                obj.insert("name".into(), c.name.clone().into());
                let (t, ros_type) = match &c.ty.base {
                    Base::Primitive(p) => (primitive(p).0, p.clone()),
                    Base::String { wide, .. } => (
                        DataType::String,
                        if *wide { "wstring" } else { "string" }.to_owned(),
                    ),
                    Base::Named { .. } => {
                        return Err(IdlError::Generate {
                            message: format!(
                                "{display}: constant {} must have a primitive type",
                                c.name
                            ),
                        })
                    }
                };
                if c.ty.array != ArrayKind::None {
                    return Err(IdlError::Generate {
                        message: format!("{display}: constant {} cannot be an array", c.name),
                    });
                }
                obj.insert("type".into(), type_name_of(&t).into());
                obj.insert("ros_type".into(), ros_type.into());
                obj.insert("value".into(), c.value.clone());
                if let Some(d) = &c.doc {
                    obj.insert("doc".into(), d.clone().into());
                }
                constants.push(Value::Object(obj));
            }
            record
                .attributes
                .insert(ATTR_CONSTANTS.into(), Value::Array(constants));
        }
        Ok(DataType::Record(record))
    }

    fn field_of(&mut self, f: &RosField, display: &str) -> Result<RecordField, IdlError> {
        let at = format!("{display}.{}", f.name);
        let err = |message: String| IdlError::Generate {
            message: format!("{at}: {message}"),
        };
        if !juiz_types::is_valid_type_name(&f.name) {
            return Err(err(
                "the field name must match [A-Za-z_][A-Za-z0-9_]*".to_owned()
            ));
        }
        let mut attributes = Map::new();
        let mut range = None;
        let is_byte_seq = matches!(&f.ty.base, Base::Primitive(p) if p == "uint8" || p == "byte")
            && f.ty.array != ArrayKind::None;
        let element = match &f.ty.base {
            Base::Primitive(p) => {
                let (t, r, keep) = primitive(p);
                if keep {
                    attributes.insert(ATTR_ROS_TYPE.into(), p.clone().into());
                }
                if p == "uint64" {
                    self.warnings.push(format!(
                        "{at}: uint64 is imported as long, so values of 2^63 or more cannot be used"
                    ));
                }
                if p == "long double" {
                    self.warnings
                        .push(format!("{at}: long double is imported as double"));
                }
                if !is_byte_seq {
                    range = r;
                }
                t
            }
            Base::String { wide, bound } => {
                if *wide {
                    attributes.insert(ATTR_ROS_TYPE.into(), "wstring".into());
                }
                if let Some(n) = bound {
                    attributes.insert(ATTR_MAX_LENGTH.into(), (*n).into());
                }
                DataType::String
            }
            Base::Named { package, name } => self.named_type(package, name, &at)?,
        };
        if let Some((min, max)) = range {
            attributes.insert(ATTR_MINIMUM.into(), (min as i64).into());
            if max < i64::MAX as i128 {
                attributes.insert(ATTR_MAXIMUM.into(), (max as i64).into());
            }
        }
        match f.ty.array {
            ArrayKind::Fixed(n) => {
                attributes.insert(ATTR_MIN_ITEMS.into(), n.into());
                attributes.insert(ATTR_MAX_ITEMS.into(), n.into());
            }
            ArrayKind::Bounded(n) => {
                attributes.insert(ATTR_MAX_ITEMS.into(), n.into());
            }
            _ => {}
        }
        let data_type = if is_byte_seq {
            DataType::Bytes
        } else if f.ty.array == ArrayKind::None {
            element.clone()
        } else {
            DataType::array(element.clone())
        };
        // 既定値: 書かれていなければ ROS 2 と同じゼロ値 (固定長の配列は要素の数だけ並べる)
        let default = match &f.default {
            Some(v) => {
                if let Some((min, max)) = range {
                    let items: Vec<&Value> = match v {
                        Value::Array(a) => a.iter().collect(),
                        other => vec![other],
                    };
                    for i in items {
                        let x = i
                            .as_i64()
                            .map(|x| x as i128)
                            .or(i.as_u64().map(|x| x as i128));
                        if x.is_some_and(|x| x < min || x > max) {
                            return Err(err(format!(
                                "default {i} is out of the range {min}..={max}"
                            )));
                        }
                    }
                }
                match (&f.ty.array, v) {
                    (ArrayKind::Fixed(n), Value::Array(a)) if a.len() as u64 != *n => {
                        return Err(err(format!("default must have {n} items")))
                    }
                    (ArrayKind::Bounded(n), Value::Array(a)) if a.len() as u64 > *n => {
                        return Err(err(format!("default must have at most {n} items")))
                    }
                    _ => {}
                }
                if is_byte_seq {
                    let bytes: Option<Vec<u8>> = v
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .map(|b| b.as_u64().and_then(|b| u8::try_from(b).ok()))
                                .collect()
                        })
                        .unwrap_or(None);
                    let bytes = bytes.ok_or_else(|| {
                        err("a byte array default must be a list of 0..=255".to_owned())
                    })?;
                    Value::String(base64::engine::general_purpose::STANDARD.encode(bytes))
                } else {
                    v.clone()
                }
            }
            None => match (&f.ty.array, is_byte_seq) {
                (ArrayKind::Fixed(n), true) => Value::String(
                    base64::engine::general_purpose::STANDARD.encode(vec![0u8; *n as usize]),
                ),
                (ArrayKind::Fixed(n), false) => Value::Array(vec![zero_of(&element); *n as usize]),
                _ => zero_of(&data_type),
            },
        };
        let mut field = RecordField::new(&f.name, data_type);
        field.doc = f.doc.clone();
        field.attributes = attributes;
        field.default_value(default).map_err(|e| err(e.to_string()))
    }

    /// 型が直接参照している名前付き型
    fn direct_refs(t: &DataType, out: &mut Vec<DataType>) {
        match t {
            DataType::Array(i) | DataType::Map(i) => Self::direct_refs(i, out),
            DataType::Optional { inner, .. } => Self::direct_refs(inner, out),
            DataType::Record(_) | DataType::Enum(_) => {
                if !out.contains(t) {
                    out.push(t.clone());
                }
            }
            _ => {}
        }
    }

    /// `from` (ros2/<package>/<sub>/) から型 `t` の .jidl を import するパス
    fn import_path(t: &DataType, from_package: &str, from_sub: SubFolder) -> String {
        let full = t.full_name().unwrap_or_default();
        if let Some((_, _, file, _)) = STD_MAP.iter().find(|(_, _, _, f)| *f == full) {
            return (*file).to_owned();
        }
        let DataType::Record(r) = t else {
            unreachable!("only records are imported")
        };
        let parts: Vec<&str> = r.namespace.split('.').collect();
        let (package, sub) = (parts[1], parts[2]);
        if package == from_package && sub == from_sub.as_str() {
            format!("{}.jidl", r.name)
        } else {
            format!("../../{package}/{sub}/{}.jidl", r.name)
        }
    }

    fn header(source: &str, namespace: &str, imports: &[String]) -> String {
        let mut out = format!(
            "// ROS 2 の {source} から juiz idl import-ros2 で作った (docs/spec/datatype.md §25)\nnamespace {namespace};\n"
        );
        if !imports.is_empty() {
            out.push('\n');
            for i in imports {
                out.push_str(&format!(
                    "import idl {};\n",
                    serde_json::to_string(i).unwrap()
                ));
            }
        }
        out
    }

    fn imports_of(
        types: &[&DataType],
        package: &str,
        sub: SubFolder,
        own: &[String],
    ) -> Vec<String> {
        let mut refs = Vec::new();
        for t in types {
            if let DataType::Record(r) = t {
                for f in r.fields.iter() {
                    Self::direct_refs(&f.data_type, &mut refs);
                }
            }
        }
        let mut imports = Vec::new();
        let mut seen = HashSet::new();
        for r in refs {
            let full = r.full_name().unwrap_or_default();
            if own.contains(&full) {
                continue;
            }
            let p = Self::import_path(&r, package, sub);
            if seen.insert(p.clone()) {
                imports.push(p);
            }
        }
        imports
    }

    fn message_file(&mut self, key: &Key) -> Result<GeneratedFile, IdlError> {
        let t = self.types[key].clone();
        let ns = namespace_of(&key.0, SubFolder::Msg);
        let imports = Self::imports_of(&[&t], &key.0, SubFolder::Msg, &[]);
        let mut contents = Self::header(&format!("{}/msg/{}", key.0, key.1), &ns, &imports);
        contents.push('\n');
        contents.push_str(&declaration(&t, &ns));
        Ok(GeneratedFile {
            path: PathBuf::from(ROS2_NAMESPACE)
                .join(&key.0)
                .join("msg")
                .join(format!("{}.jidl", key.1)),
            contents,
        })
    }

    fn service_file(&mut self, s: &Service) -> Result<GeneratedFile, IdlError> {
        let request = self.record_of(&s.request)?;
        let response = self.record_of(&s.response)?;
        let ns = namespace_of(&s.package, SubFolder::Srv);
        let own = vec![
            request.full_name().unwrap_or_default(),
            response.full_name().unwrap_or_default(),
        ];
        let imports = Self::imports_of(&[&request, &response], &s.package, SubFolder::Srv, &own);
        let mut contents = Self::header(&format!("{}/srv/{}", s.package, s.name), &ns, &imports);
        contents.push('\n');
        contents.push_str(&declaration(&request, &ns));
        contents.push('\n');
        contents.push_str(&declaration(&response, &ns));
        contents.push('\n');
        // Request のフィールドを引数、Response を戻り値にした process 宣言
        contents.push_str(&doc_comment(&s.doc, ""));
        let DataType::Record(req) = &request else {
            unreachable!()
        };
        let params: Vec<String> = req
            .fields
            .iter()
            .map(|f| {
                let default = f.default_as_dict();
                let default_is_null = matches!(default, None | Some(Value::Null));
                let mut p = format!(
                    "{} {}",
                    type_expr(&f.data_type, &ns, default_is_null),
                    jidl_name(&f.name)
                );
                if let Some(v) = default {
                    p.push_str(&format!(" = {}", serde_json::to_string(&v).unwrap()));
                }
                p
            })
            .collect();
        contents.push_str(&format!(
            "process {} {}({});\n",
            type_expr(&response, &ns, true),
            jidl_name(&snake_case(&s.name)),
            params.join(", ")
        ));
        Ok(GeneratedFile {
            path: PathBuf::from(ROS2_NAMESPACE)
                .join(&s.package)
                .join("srv")
                .join(format!("{}.jidl", s.name)),
            contents,
        })
    }
}

const BUILTIN_STD_FILES: [&str; 3] = [
    "juiz_std/common.jidl",
    "juiz_std/geometry.jidl",
    "juiz_std/sensor.jidl",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_names() {
        assert_eq!(snake_case("AddTwoInts"), "add_two_ints");
        assert_eq!(snake_case("GetURL"), "get_url");
        assert_eq!(snake_case("SetCameraInfo"), "set_camera_info");
        assert_eq!(snake_case("Trigger"), "trigger");
        assert_eq!(snake_case("GetHTTPPage2"), "get_http_page2");
    }
}
