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
                        Some(e) => read_value(&f.data_type, e, &fpath)?,
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

/// 辞書の値 `v` が型 `t` に合うかを検査する (仕様 §4)
pub fn check_value(t: &DataType, v: &Value) -> Result<()> {
    read_value(t, v, &t.path_root()).map(|_| ())
}
