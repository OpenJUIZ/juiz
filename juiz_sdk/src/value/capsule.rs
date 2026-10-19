use std::{collections::HashMap, fmt::Display, mem::swap};

// #[cfg(feature="opencv4")]
// use opencv::core::Mat;

use crate::prelude::*;
use image::DynamicImage;
use serde_json::Map;

#[derive(Clone, Debug)]
pub enum CapsuleValue {
    Empty(()),
    Value(Value),

    // #[cfg(feature="opencv4")]
    // Mat(Mat),
    Image(DynamicImage),
}

impl Display for CapsuleValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CapsuleValue::Empty(_) => f.write_fmt(format_args!("Empty")),
            CapsuleValue::Value(value) => f.write_fmt(format_args!("Value({})", value)),
            CapsuleValue::Image(_dynamic_image) => f.write_fmt(format_args!("DynamicImage")),
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

impl CapsuleValue {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Empty(_) => return true,
            _ => return false,
        }
    }

    pub fn is_value(&self) -> bool {
        match self {
            Self::Value(_) => return true,
            _ => return false,
        }
    }

    pub fn as_value(&self) -> Option<&Value> {
        match self {
            Self::Value(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_value_mut(&mut self) -> Option<&mut Value> {
        match self {
            Self::Value(v) => Some(v),
            _ => None,
        }
    }

    pub fn to_value(self) -> Option<Value> {
        match self {
            Self::Value(v) => Some(v),
            _ => None,
        }
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

    pub fn is_image(&self) -> bool {
        match self {
            Self::Image(_) => return true,
            _ => return false,
        }
    }

    pub fn as_image(&self) -> Option<&DynamicImage> {
        match self {
            Self::Image(v) => return Some(v),
            _ => return None,
        }
    }

    pub fn to_image(self) -> Option<DynamicImage> {
        match self {
            Self::Image(v) => return Some(v),
            _ => return None,
        }
    }
    /*
    pub fn to_mat(&self) -> Option<Mat> {
        match self {
            Self::Mat(v) => Some(v.clone()),
            _ => None
        }
    }*/
}

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
        match value.value {
            CapsuleValue::Value(mut _v) => Ok(jvalue!({
                "__value__": _v,
                "__option__": jvalue!(value.option)
            })),
            _ => Err(anyhow::Error::from(
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

    pub fn as_value_mut(&mut self) -> Option<&mut Value> {
        self.value.as_value_mut()
    }

    pub fn to_value(self) -> Option<Value> {
        self.value.to_value()
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
