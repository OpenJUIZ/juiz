use std::{collections::HashMap, fmt::Display, mem::swap};

// #[cfg(feature="opencv4")]
// use opencv::core::Mat;

use crate::prelude::*;
use super::native::*;
use image::DynamicImage;
use serde_json::Map;

#[derive(Clone, Debug)]
pub enum CapsuleValue {
    Empty(()),
    Value(Value),

    // #[cfg(feature="opencv4")]
    // Mat(Mat),
    Image(DynamicImage),

    /// 言語のネイティブ値。共通形式が要るときだけ encode する (docs/spec/lazy-evaluation.md)
    Native(NativeCell),
}

impl Display for CapsuleValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CapsuleValue::Empty(_) => f.write_fmt(format_args!("Empty")),
            CapsuleValue::Value(value) => f.write_fmt(format_args!("Value({})", value)),
            CapsuleValue::Image(_dynamic_image) => f.write_fmt(format_args!("DynamicImage")),
            CapsuleValue::Native(cell) => f.write_fmt(format_args!("Native({})", cell.describe())),
        }
    }
}
impl From<Value> for CapsuleValue {
    fn from(value: Value) -> Self {
        Self::Value(value)
    }
}

// #[cfg(feature="opencv4")]
// impl From<Mat> for CapsuleValue {
//     fn from(value: Mat) -> Self { Self::Mat( value ) }
// }

impl From<DynamicImage> for CapsuleValue {
    fn from(value: DynamicImage) -> Self {
        Self::Image(value)
    }
}

impl From<NativeCell> for CapsuleValue {
    fn from(value: NativeCell) -> Self {
        Self::Native(value)
    }
}

impl From<EncodedValue> for CapsuleValue {
    fn from(value: EncodedValue) -> Self {
        match value {
            EncodedValue::Value(v) => Self::Value(v),
            EncodedValue::Image(i) => Self::Image(i),
        }
    }
}

impl CapsuleValue {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Empty(_) => return true,
            _ => return false,
        }
    }

    /// Native は kind() で答え、encode しない
    pub fn is_value(&self) -> bool {
        match self {
            Self::Value(_) => return true,
            Self::Native(cell) => cell.kind() == NativeKind::Value,
            _ => return false,
        }
    }

    pub fn is_native(&self) -> bool {
        match self {
            Self::Native(_) => true,
            _ => false,
        }
    }

    pub fn as_native(&self) -> Option<&NativeCell> {
        match self {
            Self::Native(cell) => Some(cell),
            _ => None,
        }
    }

    /// Native なら encode (初回だけ) してキャッシュへの参照を返す。encode の失敗はエラーで返す
    pub fn try_as_value(&self) -> JuizResult<Option<&Value>> {
        match self {
            Self::Value(v) => Ok(Some(v)),
            Self::Native(cell) if cell.kind() == NativeKind::Value => cell.try_as_value(),
            _ => Ok(None),
        }
    }

    /// try_as_value と同じだが、encode の失敗は None にする (ログには出る)
    pub fn as_value(&self) -> Option<&Value> {
        self.try_as_value().ok().flatten()
    }

    /// Native なら encode して中身を Value に置き換えてから返す
    /// (書き換えるとネイティブ値とずれるので、ネイティブ値は捨てる)
    pub fn as_value_mut(&mut self) -> Option<&mut Value> {
        if let Self::Native(cell) = self {
            if cell.kind() != NativeKind::Value {
                return None;
            }
            match cell.clone().into_encoded() {
                Ok(encoded) => *self = encoded.into(),
                Err(_) => return None,
            }
        }
        match self {
            Self::Value(v) => Some(v),
            _ => None,
        }
    }

    /// Native なら encode して取り出す。ほかに参照が無ければキャッシュを複製せずに移す
    pub fn try_to_value(self) -> JuizResult<Option<Value>> {
        match self {
            Self::Value(v) => Ok(Some(v)),
            Self::Native(cell) if cell.kind() == NativeKind::Value => match cell.into_encoded()? {
                EncodedValue::Value(v) => Ok(Some(v)),
                EncodedValue::Image(_) => Ok(None),
            },
            _ => Ok(None),
        }
    }

    pub fn to_value(self) -> Option<Value> {
        self.try_to_value().ok().flatten()
    }

    // #[cfg(feature="opencv4")]
    // pub fn is_mat(&self) -> bool {
    //     match self {
    //         Self::Mat(_) => return true,
    //         _ => return false
    //     }
    // }

    // #[cfg(feature="opencv4")]
    // pub fn as_mat(&self) -> Option<&Mat> {
    //     match self {
    //         Self::Mat(v) => Some(v),
    //         _ => None
    //     }
    // }

    /// Native は kind() で答え、encode しない
    pub fn is_image(&self) -> bool {
        match self {
            Self::Image(_) => return true,
            Self::Native(cell) => cell.kind() == NativeKind::Image,
            _ => return false,
        }
    }

    pub fn try_as_image(&self) -> JuizResult<Option<&DynamicImage>> {
        match self {
            Self::Image(v) => Ok(Some(v)),
            Self::Native(cell) if cell.kind() == NativeKind::Image => cell.try_as_image(),
            _ => Ok(None),
        }
    }

    pub fn as_image(&self) -> Option<&DynamicImage> {
        self.try_as_image().ok().flatten()
    }

    pub fn try_to_image(self) -> JuizResult<Option<DynamicImage>> {
        match self {
            Self::Image(v) => Ok(Some(v)),
            Self::Native(cell) if cell.kind() == NativeKind::Image => match cell.into_encoded()? {
                EncodedValue::Image(v) => Ok(Some(v)),
                EncodedValue::Value(_) => Ok(None),
            },
            _ => Ok(None),
        }
    }

    pub fn to_image(self) -> Option<DynamicImage> {
        self.try_to_image().ok().flatten()
    }
    /*
    pub fn to_mat(&self) -> Option<Mat> {
        match self {
            Self::Mat(v) => Some(v.clone()),
            _ => None
        }
    }*/
}

/// Process の関数が返す値を表す型。実体は Capsule です。
///
/// `fn talker() -> JuizResult<JuizResponse>` のように、関数の返り値であることを
/// わかりやすく書くための別名です。Capsule と同じように `jvalue!(...).into()` などで作れます。
pub type JuizResponse = Capsule;

#[derive(Clone, Debug)]
pub struct Capsule {
    value: CapsuleValue,
    option: HashMap<String, String>,
}

impl Display for Capsule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("(value:{}, option: [", self.value))?;
        for (k, v) in self.option.iter() {
            f.write_fmt(format_args!("{k}:{v},"))?;
        }
        f.write_str("])")
    }
}

impl From<Value> for Capsule {
    fn from(mut value: Value) -> Self {
        let mut option_map: HashMap<String, String> = HashMap::new();
        match value.as_object() {
            None => {
                return Self {
                    value: CapsuleValue::from(value),
                    option: option_map,
                };
            }
            Some(obj) => {
                if !(obj.contains_key("__value__") && obj.contains_key("__option__")) {
                    return Self {
                        value: CapsuleValue::from(value),
                        option: option_map,
                    };
                }
            }
        }
        for (k, v) in value
            .as_object()
            .unwrap()
            .get("__option__")
            .unwrap()
            .as_object()
            .unwrap()
            .iter()
        {
            option_map.insert(k.clone(), v.as_str().unwrap().to_owned());
        }
        return Self {
            value: CapsuleValue::from(value.as_object_mut().unwrap().remove("__value__").unwrap()),
            option: option_map,
        };
    }
}

impl TryFrom<Capsule> for Value {
    type Error = anyhow::Error;

    fn try_from(value: Capsule) -> Result<Self, Self::Error> {
        //match value.as_value() {
        //    Some(v) => {}, //Ok(v),
        //    None => return Err(anyhow::Error::from(JuizError::CapsuleIsNotValueTypeError{}))
        //};
        let option = value.option;
        match value.value.try_to_value()? {
            Some(_v) => Ok(jvalue!({
                "__value__": _v,
                "__option__": jvalue!(option)
            })),
            None => Err(anyhow::Error::from(
                JuizError::CapsuleIsNotValueTypeError {},
            )),
        }
    }
}

// #[cfg(feature="opencv4")]
// impl From<Mat> for Capsule {
//     fn from(mat_value: Mat) -> Self {
//         Self{
//             value: CapsuleValue::from(mat_value),
//             option: HashMap::new(),
//         }
//     }
// }

impl From<DynamicImage> for Capsule {
    fn from(img_value: DynamicImage) -> Self {
        Self {
            value: CapsuleValue::from(img_value),
            option: HashMap::new(),
        }
    }
}

/*
impl TryInto<Mat> for Capsule {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<Mat, Self::Error> {
        //match self.to_mat() {
        //    Some(v) => Ok(v),
        //    None => Err(anyhow::Error::from(JuizError::CapsuleIsNotValueTypeError{}))
        //}
        todo!()
    }
}*/

impl Capsule {
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    pub fn is_value(&self) -> bool {
        self.value.is_value()
    }

    pub fn as_value(&self) -> Option<&Value> {
        self.value.as_value()
    }

    pub fn try_as_value(&self) -> JuizResult<Option<&Value>> {
        self.value.try_as_value()
    }

    pub fn as_value_mut(&mut self) -> Option<&mut Value> {
        self.value.as_value_mut()
    }

    pub fn to_value(self) -> Option<Value> {
        self.value.to_value()
    }

    pub fn try_to_value(self) -> JuizResult<Option<Value>> {
        self.value.try_to_value()
    }

    /// ネイティブ値を包む (docs/spec/lazy-evaluation.md §3.2)
    pub fn from_native(value: impl NativeValue + 'static) -> Capsule {
        Self {
            value: CapsuleValue::Native(NativeCell::new(value)),
            option: HashMap::new(),
        }
    }

    /// 中身が Native か (encode しない)
    pub fn is_native(&self) -> bool {
        self.value.is_native()
    }

    /// 中身のネイティブ値。同じ言語の受け取り側が as_any().downcast_ref() で具体的な型にする
    pub fn as_native(&self) -> Option<&dyn NativeValue> {
        self.value.as_native().map(|c| c.value())
    }

    pub fn native_cell(&self) -> Option<&NativeCell> {
        self.value.as_native()
    }

    // #[cfg(feature="opencv4")]
    // pub fn is_mat(&self) -> bool { self.value.is_mat() }

    // #[cfg(feature="opencv4")]
    // pub fn as_mat(&self) -> Option<&opencv::core::Mat> { self.value.as_mat() }

    pub fn is_image(&self) -> bool {
        self.value.is_image()
    }

    pub fn as_image(&self) -> Option<&DynamicImage> {
        self.value.as_image()
    }

    pub fn try_as_image(&self) -> JuizResult<Option<&DynamicImage>> {
        self.value.try_as_image()
    }

    pub fn try_to_image(self) -> JuizResult<Option<DynamicImage>> {
        self.value.try_to_image()
    }
    //pub fn to_mat(&self) -> Option<opencv::core::Mat> { self.value.to_mat() }

    pub fn to_image(self) -> Option<DynamicImage> {
        self.value.to_image()
    }

    pub fn set_option(&mut self, key: &str, value: &str) -> &mut Self {
        self.option.insert(key.to_owned(), value.to_owned());
        self
    }

    pub fn get_option(&self, key: &str) -> Option<&String> {
        self.option.get(&key.to_owned())
    }

    pub fn get_options(&self) -> &HashMap<String, String> {
        &self.option
    }

    pub fn empty() -> Capsule {
        Self {
            value: CapsuleValue::Empty(()),
            option: HashMap::new(),
        }
    }

    pub fn set_function_name(&mut self, name: &str) -> () {
        self.set_option("function_name", name);
        //self
    }

    pub(crate) fn _replace(&mut self, capsule: Capsule) -> () {
        self.value = capsule.value;
        self.option = capsule.option;
    }

    pub(crate) fn replace_value(&mut self, value: CapsuleValue) -> () {
        self.value = value;
    }

    pub(crate) fn take_value(&mut self) -> CapsuleValue {
        let mut emp = CapsuleValue::Empty(());
        swap(&mut self.value, &mut emp);
        emp
    }
}

impl From<bool> for Capsule {
    fn from(value: bool) -> Self {
        jvalue!(value).into()
    }
}
impl From<u64> for Capsule {
    fn from(value: u64) -> Self {
        jvalue!(value).into()
    }
}
impl From<i64> for Capsule {
    fn from(value: i64) -> Self {
        jvalue!(value).into()
    }
}
impl From<f64> for Capsule {
    fn from(value: f64) -> Self {
        jvalue!(value).into()
    }
}
impl From<&str> for Capsule {
    fn from(value: &str) -> Self {
        jvalue!(value).into()
    }
}
// String・Map・Vec は jvalue! を通すと複製されるので、そのまま Value に包む。
impl From<String> for Capsule {
    fn from(value: String) -> Self {
        Value::String(value).into()
    }
}
impl From<Map<String, Value>> for Capsule {
    fn from(value: Map<String, Value>) -> Self {
        Value::Object(value).into()
    }
}
impl From<Vec<Value>> for Capsule {
    fn from(value: Vec<Value>) -> Self {
        Value::Array(value).into()
    }
}
