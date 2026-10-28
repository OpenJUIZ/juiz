//! 辞書の値を型で読む (仕様 §4)

use base64::Engine;
use serde_json::Map;

use crate::data_type::DataType;
use crate::{Result, TypeError};
use serde_json::Value;

fn mismatch(t: &DataType, path: &str, reason: impl Into<String>) -> TypeError {
    TypeError::ValueDoesNotMatch {
        type_name: t.display_name(),
        path: path.to_owned(),
        reason: reason.into(),
    }
}

/// reader の型 `t` で辞書の値 `v` を読み、reader から見た値を返す (仕様 §4)。
///
/// レコードの足りないフィールドは既定値で埋め、型にないキーは落とす。列挙の知らない記号は、既定値があれば既定値にする。
/// 合わなければ `ValueDoesNotMatchTypeError`。
pub fn read_value(t: &DataType, v: &Value, path: &str) -> Result<Value> {
    let expect = |what: &str| mismatch(t, path, format!("expected {what}, found {v}"));
    match t {
        DataType::Null => {
            if v.is_null() {
                Ok(Value::Null)
            } else {
                Err(expect("null"))
            }
        }
        DataType::Boolean => {
            if v.is_boolean() {
                Ok(v.clone())
            } else {
                Err(expect("a boolean"))
            }
        }
        DataType::Int => match v.as_i64() {
            Some(i) if v.is_i64() && i32::try_from(i).is_ok() => Ok(v.clone()),
            Some(_) => Err(mismatch(t, path, format!("{v} is out of the range of int"))),
            None => Err(expect("an integer")),
        },
        DataType::Long => {
            if v.is_i64() {
                Ok(v.clone())
            } else if v.is_u64() {
                Err(mismatch(
                    t,
                    path,
                    format!("{v} is out of the range of long"),
                ))
            } else {
                Err(expect("an integer"))
            }
        }
        DataType::Float => match v.as_f64() {
            Some(f) if f.abs() <= f32::MAX as f64 => Ok(v.clone()),
            Some(_) => Err(mismatch(
                t,
                path,
                format!("{v} is out of the range of float"),
            )),
            None => Err(expect("a number")),
        },
        DataType::Double => {
            if v.is_number() {
                Ok(v.clone())
            } else {
                Err(expect("a number"))
            }
        }
        DataType::String => {
            if v.is_string() {
                Ok(v.clone())
            } else {
                Err(expect("a string"))
            }
        }
        DataType::Bytes => match v.as_str() {
            Some(s) => base64::engine::general_purpose::STANDARD
                .decode(s)
                .map(|_| v.clone())
                .map_err(|e| mismatch(t, path, format!("bytes must be a base64 string ({e})"))),
            None => Err(expect("a base64 string")),
        },
        DataType::Array(items) => match v.as_array() {
            Some(arr) => arr
                .iter()
                .enumerate()
                .map(|(i, e)| read_value(items, e, &format!("{path}[{i}]")))
                .collect::<Result<Vec<Value>>>()
                .map(Value::from),
            None => Err(expect("an array")),
        },
        DataType::Map(values) => match v.as_object() {
            Some(m) => {
                let mut out = Map::new();
                for (k, e) in m.iter() {
                    out.insert(k.clone(), read_value(values, e, &format!("{path}.{k}"))?);
                }
                Ok(Value::Object(out))
            }
            None => Err(expect("an object")),
        },
        DataType::Optional { inner, .. } => {
            if v.is_null() {
                Ok(Value::Null)
            } else {
                read_value(inner, v, path)
            }
        }
        DataType::Record(r) => match v.as_object() {
            Some(m) => {
                let mut out = Map::new();
                for f in r.fields.iter() {
                    let fpath = format!("{path}.{}", f.name);
                    let fv = match m.get(&f.name) {
                        Some(e) => {
                            let fv = read_value(&f.data_type, e, &fpath)?;
                            check_constraints(&f.attributes, &f.data_type, &fv, &fpath)?;
                            fv
                        }
                        None => match f.default_as_dict() {
                            Some(d) => d,
                            None => {
                                return Err(mismatch(
                                    t,
                                    &fpath,
                                    format!("field '{}' is missing and has no default", f.name),
                                ))
                            }
                        },
                    };
                    out.insert(f.name.clone(), fv);
                }
                Ok(Value::Object(out))
            }
            None => Err(expect("an object")),
        },
        DataType::Enum(e) => match v.as_str() {
            Some(s) if e.symbols.iter().any(|x| x == s) => Ok(v.clone()),
            Some(s) => match e.default.as_ref() {
                Some(d) => Ok(d.as_str().into()),
                None => Err(mismatch(
                    t,
                    path,
                    format!("'{s}' is not a symbol of {}", e.full_name()),
                )),
            },
            None => Err(expect("a symbol string")),
        },
        DataType::Any => Ok(v.clone()),
        DataType::Image => Err(mismatch(
            t,
            path,
            "an image is not a dictionary value (pass it as Capsule::Image)",
        )),
    }
}

/// フィールドの値の範囲 (`juiz.minimum` / `juiz.maximum`)
pub const ATTR_MINIMUM: &str = "juiz.minimum";
pub const ATTR_MAXIMUM: &str = "juiz.maximum";
/// 文字列の長さ (文字数) の上限 (`juiz.maxLength`)
pub const ATTR_MAX_LENGTH: &str = "juiz.maxLength";
/// 配列の要素の数・バイト列のバイト数 (`juiz.minItems` / `juiz.maxItems`)
pub const ATTR_MIN_ITEMS: &str = "juiz.minItems";
pub const ATTR_MAX_ITEMS: &str = "juiz.maxItems";

/// フィールドの属性にある制約 (ROS 2 の型の取り込みで付く、仕様 §10) で値を検査する。
///
/// `juiz.minItems` / `juiz.maxItems` は配列の要素の数 (`bytes` ならバイト数) に、
/// `juiz.minimum` / `juiz.maximum` / `juiz.maxLength` は値そのもの (配列なら各要素) に使う。
/// 制約は互換性の判定と指紋には使わない (仕様 §7.2)
fn check_constraints(
    attributes: &Map<String, Value>,
    t: &DataType,
    v: &Value,
    path: &str,
) -> Result<()> {
    if attributes.is_empty() || v.is_null() {
        return Ok(());
    }
    let count = match (t, v) {
        (DataType::Array(_), Value::Array(a)) => Some(a.len()),
        (DataType::Bytes, Value::String(s)) => base64::engine::general_purpose::STANDARD
            .decode(s)
            .ok()
            .map(|b| b.len()),
        _ => None,
    };
    if let Some(n) = count {
        if let Some(min) = attributes.get(ATTR_MIN_ITEMS).and_then(|m| m.as_u64()) {
            if (n as u64) < min {
                return Err(mismatch(
                    t,
                    path,
                    format!("{n} items is fewer than {ATTR_MIN_ITEMS} {min}"),
                ));
            }
        }
        if let Some(max) = attributes.get(ATTR_MAX_ITEMS).and_then(|m| m.as_u64()) {
            if (n as u64) > max {
                return Err(mismatch(
                    t,
                    path,
                    format!("{n} items is more than {ATTR_MAX_ITEMS} {max}"),
                ));
            }
        }
    }
    let (items, values): (&DataType, Vec<(String, &Value)>) = match (t, v) {
        (DataType::Array(items), Value::Array(a)) => (
            items,
            a.iter()
                .enumerate()
                .map(|(i, e)| (format!("{path}[{i}]"), e))
                .collect(),
        ),
        (DataType::Optional { inner, .. }, _) => (inner, vec![(path.to_owned(), v)]),
        _ => (t, vec![(path.to_owned(), v)]),
    };
    for (p, e) in values {
        if let (Some(x), Some(min)) = (
            e.as_f64(),
            attributes.get(ATTR_MINIMUM).and_then(|m| m.as_f64()),
        ) {
            if x < min {
                return Err(mismatch(
                    items,
                    &p,
                    format!("{e} is less than {ATTR_MINIMUM} {min}"),
                ));
            }
        }
        if let (Some(x), Some(max)) = (
            e.as_f64(),
            attributes.get(ATTR_MAXIMUM).and_then(|m| m.as_f64()),
        ) {
            if x > max {
                return Err(mismatch(
                    items,
                    &p,
                    format!("{e} is more than {ATTR_MAXIMUM} {max}"),
                ));
            }
        }
        if let (Some(s), Some(max)) = (
            e.as_str(),
            attributes.get(ATTR_MAX_LENGTH).and_then(|m| m.as_u64()),
        ) {
            let n = s.chars().count() as u64;
            if n > max {
                return Err(mismatch(
                    items,
                    &p,
                    format!("{n} characters is more than {ATTR_MAX_LENGTH} {max}"),
                ));
            }
        }
    }
    Ok(())
}

/// 辞書の値 `v` が型 `t` に合うかを検査する (仕様 §4)
pub fn check_value(t: &DataType, v: &Value) -> Result<()> {
    read_value(t, v, &t.path_root()).map(|_| ())
}
