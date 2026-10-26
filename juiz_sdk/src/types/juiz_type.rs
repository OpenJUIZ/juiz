//! 型付きの値の Rust の型 (`JuizType`、仕様 §11)

use std::collections::HashMap;

use anyhow::anyhow;
use base64::Engine;

use super::data_type::DataType;
use super::type_schema::TypeSchema;
use crate::prelude::*;

/// 遅延評価 (#3) でネイティブ値をそのまま渡してよいかの判定に使う型タグ (仕様 §12)
///
/// 3 つすべてが一致するときだけネイティブ値のまま渡す。`abi` は中身を解釈せず完全一致で比べる。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TypeTag {
    pub name: String,
    pub fingerprint: String,
    pub abi: String,
}

fn conversion_error(t: &DataType, reason: String) -> anyhow::Error {
    anyhow!(JuizError::ValueDoesNotMatchTypeError {
        type_name: t.display_name(),
        path: t.path_root(),
        reason,
    })
}

/// juiz の型を持つ Rust の型
///
/// 辞書 (仕様 §4) との変換、スキーマ、型の識別子を持つ。`#[derive(JuizType)]` で struct / enum に実装できる。
pub trait JuizType: Sized {
    /// この型のモデル
    fn data_type() -> DataType;

    /// 辞書の表記 (仕様 §4) にする。`to_value` はすべてのフィールドを出す
    fn to_value(&self) -> JuizResult<Value>;

    /// 辞書の表記 (仕様 §4) から読む
    fn from_value(value: &Value) -> JuizResult<Self>;

    /// manifest に載せる型参照
    fn type_schema() -> JuizResult<TypeSchema> {
        TypeSchema::from_data_type(&Self::data_type())
    }

    /// 型の識別子 `<表示名>@sha256:<64 桁>` (仕様 §5.2)
    fn type_identifier() -> String {
        let t = Self::data_type();
        format!("{}@{}", t.display_name(), t.fingerprint())
    }

    /// ABI タグ (仕様 §12)。ネイティブ値で渡せる型だけが返す。既定は `None` (いつも辞書を通す)
    fn abi_tag() -> Option<String> {
        None
    }

    /// 遅延評価の型タグ (仕様 §12)。[`JuizType::abi_tag`] が `None` なら `None`
    fn type_tag() -> Option<TypeTag> {
        Self::abi_tag().map(|abi| {
            let t = Self::data_type();
            TypeTag {
                name: t.display_name(),
                fingerprint: t.fingerprint(),
                abi,
            }
        })
    }

    /// Process の戻り値にする
    fn into_capsule(self) -> JuizResult<Capsule> {
        Ok(self.to_value()?.into())
    }

    /// Process の引数から取り出す
    fn from_capsule_ptr(capsule: &CapsulePtr) -> JuizResult<Self> {
        capsule.lock_as_value(|v| Self::from_value(v))?
    }
}

macro_rules! impl_juiz_type_primitive {
    ($t:ty, $variant:ident, $read:expr) => {
        impl JuizType for $t {
            fn data_type() -> DataType {
                DataType::$variant
            }
            fn to_value(&self) -> JuizResult<Value> {
                Ok(jvalue!(*self))
            }
            fn from_value(value: &Value) -> JuizResult<Self> {
                super::check_value(&DataType::$variant, value)?;
                let read: fn(&Value) -> Option<$t> = $read;
                read(value).ok_or_else(|| {
                    conversion_error(&DataType::$variant, format!("cannot read {value}"))
                })
            }
        }
    };
}

impl_juiz_type_primitive!(bool, Boolean, |v| v.as_bool());
impl_juiz_type_primitive!(i32, Int, |v| v.as_i64().map(|i| i as i32));
impl_juiz_type_primitive!(i64, Long, |v| v.as_i64());
impl_juiz_type_primitive!(f32, Float, |v| v.as_f64().map(|f| f as f32));
impl_juiz_type_primitive!(f64, Double, |v| v.as_f64());

impl JuizType for String {
    fn data_type() -> DataType {
        DataType::String
    }
    fn to_value(&self) -> JuizResult<Value> {
        Ok(self.as_str().into())
    }
    fn from_value(value: &Value) -> JuizResult<Self> {
        super::check_value(&DataType::String, value)?;
        Ok(value.as_str().unwrap().to_owned())
    }
}

/// `null` (JIDL の `void`)
impl JuizType for () {
    fn data_type() -> DataType {
        DataType::Null
    }
    fn to_value(&self) -> JuizResult<Value> {
        Ok(Value::Null)
    }
    fn from_value(value: &Value) -> JuizResult<Self> {
        super::check_value(&DataType::Null, value)
    }
    fn into_capsule(self) -> JuizResult<Capsule> {
        Ok(Capsule::empty())
    }
}

/// `any`
impl JuizType for Value {
    fn data_type() -> DataType {
        DataType::Any
    }
    fn to_value(&self) -> JuizResult<Value> {
        Ok(self.clone())
    }
    fn from_value(value: &Value) -> JuizResult<Self> {
        Ok(value.clone())
    }
}

/// `bytes` (辞書では base64 の文字列)
impl JuizType for Vec<u8> {
    fn data_type() -> DataType {
        DataType::Bytes
    }
    fn to_value(&self) -> JuizResult<Value> {
        Ok(base64::engine::general_purpose::STANDARD
            .encode(self)
            .into())
    }
    fn from_value(value: &Value) -> JuizResult<Self> {
        let s = value.as_str().ok_or_else(|| {
            conversion_error(
                &DataType::Bytes,
                format!("expected a base64 string, found {value}"),
            )
        })?;
        base64::engine::general_purpose::STANDARD
            .decode(s)
            .map_err(|e| conversion_error(&DataType::Bytes, format!("not base64 ({e})")))
    }
}

/// `image`。辞書には入らないので、`Capsule::Image` で渡す
impl JuizType for DynamicImage {
    fn data_type() -> DataType {
        DataType::Image
    }
    fn to_value(&self) -> JuizResult<Value> {
        Err(conversion_error(
            &DataType::Image,
            "an image is not a dictionary value".to_owned(),
        ))
    }
    fn from_value(_value: &Value) -> JuizResult<Self> {
        Err(conversion_error(
            &DataType::Image,
            "an image is not a dictionary value".to_owned(),
        ))
    }
    fn into_capsule(self) -> JuizResult<Capsule> {
        Ok(self.into())
    }
    fn from_capsule_ptr(capsule: &CapsulePtr) -> JuizResult<Self> {
        capsule.lock_as_image(|img| img.clone())
    }
}

/// `array<T>` (`Vec<u8>` は `bytes` なので、`u8` は `JuizType` にしない)
impl<T: JuizType> JuizType for Vec<T> {
    fn data_type() -> DataType {
        DataType::array(T::data_type())
    }
    fn to_value(&self) -> JuizResult<Value> {
        self.iter()
            .map(|e| e.to_value())
            .collect::<JuizResult<Vec<Value>>>()
            .map(Value::from)
    }
    fn from_value(value: &Value) -> JuizResult<Self> {
        let arr = value.as_array().ok_or_else(|| {
            conversion_error(
                &Self::data_type(),
                format!("expected an array, found {value}"),
            )
        })?;
        arr.iter().map(T::from_value).collect()
    }
}

/// `map<T>`
impl<T: JuizType> JuizType for HashMap<String, T> {
    fn data_type() -> DataType {
        DataType::map(T::data_type())
    }
    fn to_value(&self) -> JuizResult<Value> {
        let mut m = serde_json::Map::new();
        for (k, v) in self.iter() {
            m.insert(k.clone(), v.to_value()?);
        }
        Ok(Value::Object(m))
    }
    fn from_value(value: &Value) -> JuizResult<Self> {
        let m = value.as_object().ok_or_else(|| {
            conversion_error(
                &Self::data_type(),
                format!("expected an object, found {value}"),
            )
        })?;
        m.iter()
            .map(|(k, v)| Ok((k.clone(), T::from_value(v)?)))
            .collect()
    }
}

/// `T?`
impl<T: JuizType> JuizType for Option<T> {
    fn data_type() -> DataType {
        DataType::optional(T::data_type())
    }
    fn to_value(&self) -> JuizResult<Value> {
        match self {
            Some(v) => v.to_value(),
            None => Ok(Value::Null),
        }
    }
    fn from_value(value: &Value) -> JuizResult<Self> {
        if value.is_null() {
            Ok(None)
        } else {
            T::from_value(value).map(Some)
        }
    }
}
