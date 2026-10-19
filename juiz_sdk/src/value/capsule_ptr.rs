//pub type CapsulePtr = Arc<Mutex<Capsule>>;

use anyhow::anyhow;
use image::DynamicImage;
use std::{
    collections::HashMap,
    fmt::Display,
    sync::{Arc, Mutex},
};
// #[cfg(feature="opencv4")]
// pub use opencv::core::Mat;

use serde_json::Map;

use crate::prelude::*;

#[repr(C)]
#[derive(Debug)]
pub struct CapsulePtr {
    value: Arc<Mutex<Capsule>>,
}

impl Display for CapsulePtr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("CapsulePtr({})", self.value.lock().unwrap()))
    }
}
impl CapsulePtr {
    pub fn new() -> Self {
        Self {
            value: Arc::new(Mutex::new(Capsule::empty())),
        }
    }

    pub(crate) fn value_mut(&mut self) -> &mut Arc<Mutex<Capsule>> {
        &mut self.value
    }

    pub fn replace(&self, capsule: CapsulePtr) -> () {
        match self.value.lock() {
            Ok(mut c) => {
                c.replace_value(capsule.value.lock().unwrap().take_value());
            }
            Err(_) => todo!(),
        }
    }

    pub fn replace_with_value(&mut self, value: Value) -> () {
        self.value = Arc::new(Mutex::new(value.into()));
    }

    pub fn is_empty(&self) -> JuizResult<bool> {
        match self.value.lock() {
            Ok(c) => Ok(c.is_empty()),
            Err(_e) => Err(anyhow::Error::from(JuizError::MutexLockFailedError {
                error: "CapsulePtr.is_empty() lock error.".to_owned(),
            })),
        }
    }

    // #[cfg(feature="opencv4")]
    // pub fn is_mat(&self) -> JuizResult<bool> {
    //     match self.value.lock() {
    //         Ok(c) => {
    //             Ok(c.is_mat())
    //         },
    //         Err(_e) => Err(anyhow::Error::from(JuizError::MutexLockFailedError { error: "CapsulePtr.is_empty() lock error.".to_owned() })),
    //     }
    // }

    // #[cfg(not(feature="opencv4"))]
    pub fn is_image(&self) -> JuizResult<bool> {
        match self.value.lock() {
            Ok(c) => Ok(c.is_image()),
            Err(_e) => Err(anyhow::Error::from(JuizError::MutexLockFailedError {
                error: "CapsulePtr.is_empty() lock error.".to_owned(),
            })),
        }
    }

    pub fn is_value(&self) -> JuizResult<bool> {
        match self.value.lock() {
            Ok(c) => Ok(c.is_value()),
            Err(_e) => Err(anyhow::Error::from(JuizError::MutexLockFailedError {
                error: "CapsulePtr.is_empty() lock error.".to_owned(),
            })),
        }
    }

    pub fn set_option(&mut self, key: &str, value: &str) -> JuizResult<&mut Self> {
        match self.value.lock() {
            Ok(mut c) => {
                c.set_option(key, value);
            }
            Err(_e) => {
                return Err(anyhow::Error::from(JuizError::MutexLockFailedError {
                    error: "CapsulePtr.set_option() lock error.".to_owned(),
                }));
            }
        }
        Ok(self)
    }

    pub fn get_option(&self, key: &str) -> JuizResult<String> {
        match self.value.lock() {
            Ok(c) => match c.get_option(key) {
                Some(s) => Ok(s.clone()),
                None => Err(anyhow::Error::from(
                    JuizError::CapsuleDoesNotIncludeParamError {
                        name: key.to_owned(),
                    },
                )),
            },
            Err(_e) => Err(anyhow::Error::from(JuizError::MutexLockFailedError {
                error: "CapsulePtr.get_potion() lock error.".to_owned(),
            })),
        }
    }

    pub fn lock_as_value_and_opt<T, F>(&self, func: F) -> JuizResult<T>
    where
        F: FnOnce(&Value, &HashMap<String, String>) -> T,
    {
        match self.value.lock() {
            Ok(c) => match c.as_value() {
                Some(v) => Ok(func(v, c.get_options())),
                None => todo!(),
            },
            Err(_e) => Err(anyhow::Error::from(JuizError::MutexLockFailedError {
                error: "CapsulePtr.lock_as_value() lock error.".to_owned(),
            })),
        }
    }

    pub fn extract_image(self) -> JuizResult<DynamicImage> {
        match Arc::try_unwrap(self.value) {
            Ok(v) => match v.into_inner() {
                Ok(vv) => return Ok(vv.to_image().unwrap()),
                Err(v) => return Ok(v.get_ref().as_image().unwrap().clone()),
            },
            Err(e) => e
                .lock()
                .or_else(|_| {
                    Err(anyhow::Error::from(JuizError::MutexLockFailedError {
                        error: "".to_owned(),
                    }))
                })
                .and_then(|v| {
                    if let Some(img) = v.as_image() {
                        Ok(img.clone())
                    } else {
                        Err(anyhow!(JuizError::ValueTypeError {
                            message: format!("value must be image, but {:?}", v)
                        }))
                    }
                }),
        }
    }

    /// 値型の中身を取り出す。ほかに参照が無ければ複製せずに移し、あれば複製する。
    pub fn extract_value(self) -> JuizResult<Value> {
        let not_value = || {
            anyhow!(JuizError::ValueTypeError {
                message: "extract_value() failed. Value is not value-type".to_owned()
            })
        };
        match Arc::try_unwrap(self.value) {
            Ok(v) => match v.into_inner() {
                Ok(vv) => vv.to_value().ok_or_else(not_value),
                Err(v) => v.get_ref().as_value().cloned().ok_or_else(not_value),
            },
            Err(e) => e
                .lock()
                // or(Err(..)) だと成功時にもエラー値（バックトレース付き）を作るので、失敗時だけ作る
                .or_else(|_| {
                    Err(anyhow::Error::from(JuizError::MutexLockFailedError {
                        error: "CapsulePtr.extract_value() lock error.".to_owned(),
                    }))
                })
                .and_then(|v| v.as_value().cloned().ok_or_else(not_value)),
        }
    }

    pub fn lock_as_value<T, F>(&self, func: F) -> JuizResult<T>
    where
        F: FnOnce(&Value) -> T,
    {
        // println!("lock_as_value() called");
        match self.value.lock() {
            Ok(c) => {
                // println!("locked");
                match c.as_value() {
                    Some(v) => {
                        // println!("do");
                        Ok(func(v))
                    }
                    None => {
                        Err(anyhow::Error::from(JuizError::ValueTypeError {
                            message: format!("lock_as_value() failed. Value is not value-type"),
                        }))
                        //todo!()
                    }
                }
            }
            Err(_e) => Err(anyhow::Error::from(JuizError::MutexLockFailedError {
                error: "CapsulePtr.lock_as_value() lock error.".to_owned(),
            })),
        }
    }

    pub fn lock_as_str<T, F>(&self, func: F) -> JuizResult<T>
    where
        F: FnOnce(&str) -> T,
    {
        match self.value.lock() {
            Ok(c) => match c.as_value() {
                Some(v) => match v.as_str() {
                    Some(s) => Ok(func(s)),
                    None => Err(anyhow::Error::from(JuizError::ValueTypeError {
                        message: "CapsulePtr.lock_as_str() value is not str type.".to_owned(),
                    })),
                },
                None => Err(anyhow::Error::from(JuizError::ValueTypeError {
                    message: "CapsulePtr.lock_as_value() data is not Value type.".to_owned(),
                })),
            },
            Err(_e) => Err(anyhow::Error::from(JuizError::MutexLockFailedError {
                error: "CapsulePtr.lock_as_str() lock error.".to_owned(),
            })),
        }
    }

    pub fn lock_as_map<T, F>(&self, func: F) -> JuizResult<T>
    where
        F: FnOnce(&Map<String, Value>) -> T,
    {
        match self.value.lock() {
            Ok(c) => match c.as_value() {
                Some(v) => match v.as_object() {
                    Some(s) => Ok(func(s)),
                    None => Err(anyhow::Error::from(JuizError::ValueTypeError {
                        message: "CapsulePtr.lock_as_map() value is not Map type.".to_owned(),
                    })),
                },
                None => Err(anyhow::Error::from(JuizError::ValueTypeError {
                    message: "CapsulePtr.lock_as_map() data is not Value type.".to_owned(),
                })),
            },
            Err(_e) => Err(anyhow::Error::from(JuizError::MutexLockFailedError {
                error: "CapsulePtr.lock_as_map() lock error.".to_owned(),
            })),
        }
    }

    pub fn lock_modify_as_value<F, T>(&self, func: F) -> JuizResult<T>
    where
        F: FnOnce(&mut Value) -> T,
    {
        match self.value.lock() {
            Ok(mut c) => match c.as_value_mut() {
                Some(v) => Ok(func(v)),
                None => todo!(),
            },
            Err(_e) => Err(anyhow::Error::from(JuizError::MutexLockFailedError {
                error: "CapsulePtr.lock_as_value() lock error.".to_owned(),
            })),
        }
    }

    // #[cfg(feature="opencv4")]
    // pub fn lock_as_mat<T, F>(&self, func: F) -> JuizResult<T> where F: FnOnce(&opencv::prelude::Mat) -> T{
    //     match self.value.lock() {
    //         Ok(c) => {
    //             match c.as_mat() {
    //                 Some(v) => Ok(func(v)),
    //                 None => todo!(),
    //             }
    //         }
    //         Err(_e) => Err(anyhow::Error::from(JuizError::MutexLockFailedError { error: "CapsulePtr.lock_as_value() lock error.".to_owned() })),
    //     }
    // }

    // #[cfg(not(feature="opencv4"))]
    pub fn lock_as_image<T, F>(&self, func: F) -> JuizResult<T>
    where
        F: FnOnce(&DynamicImage) -> T,
    {
        match self.value.lock() {
            Ok(c) => match c.as_image() {
                Some(v) => Ok(func(v)),
                None => todo!(),
            },
            Err(_e) => Err(anyhow::Error::from(JuizError::MutexLockFailedError {
                error: "CapsulePtr.lock_as_value() lock error.".to_owned(),
            })),
        }
    }

    pub fn set_function_name(&mut self, name: &str) -> JuizResult<()> {
        self.set_option("function_name", name)?;
        Ok(())
    }

    pub fn get_function_name(&self) -> JuizResult<String> {
        self.get_option("function_name")
    }

    pub fn set_class_name(&mut self, name: &str) -> JuizResult<()> {
        self.set_option("class_name", name)?;
        Ok(())
    }

    #[allow(unused)]
    pub fn get_class_name(&self) -> JuizResult<String> {
        self.get_option("class_name")
    }
}

impl From<Value> for CapsulePtr {
    fn from(mut value: Value) -> Self {
        // log::trace!("From<Value>({value:?}) -> CapsulePtr called");
        match value.as_object_mut() {
            None => Self {
                value: Arc::new(Mutex::new(value.into())),
            },
            Some(obj) => match obj.remove_entry("__value__") {
                None => Self {
                    value: Arc::new(Mutex::new(value.into())),
                },
                Some((_k, v)) => Self {
                    value: Arc::new(Mutex::new(v.into())),
                },
            },
        }
    }
}

// #[cfg(feature="opencv4")]
// impl From<Mat> for CapsulePtr {
//     fn from(value: Mat) -> Self {
//         Self{value: Arc::new(Mutex::new(value.into()))}
//     }
// }

// #[cfg(not(feature="opencv4"))]
impl From<DynamicImage> for CapsulePtr {
    fn from(value: DynamicImage) -> Self {
        Self {
            value: Arc::new(Mutex::new(value.into())),
        }
    }
}

impl From<Capsule> for CapsulePtr {
    fn from(value: Capsule) -> Self {
        Self {
            value: Arc::new(Mutex::new(value)),
        }
    }
}

impl TryInto<Value> for CapsulePtr {
    fn try_into(self) -> Result<Value, Self::Error> {
        self.lock_as_value_and_opt(|v, opt| {
            jvalue!({
                "__value__": v,
                "__option__": jvalue!(opt)
            })
        })
    }
    type Error = anyhow::Error;
}

impl Clone for CapsulePtr {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
        }
    }
}

pub fn capsule_to_value(capsule: CapsulePtr) -> JuizResult<Value> {
    log::trace!("capsule_to_value(capsule: {capsule:?}) called");
    capsule.lock_as_value_and_opt(|v, opt| {
        jvalue!({
            "__value__": v,
            "__option__": jvalue!(opt)
        })
    })
}

pub fn value_to_capsule(value: Value) -> CapsulePtr {
    value.into()
}

impl TryInto<i64> for CapsulePtr {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<i64, Self::Error> {
        self.lock_as_value(|v| v.as_i64())?.ok_or_else(|| {
            anyhow!(JuizError::ValueTypeError {
                message: "From<i64> for CapsulePtr failed.".to_owned()
            })
        })
    }
}

impl TryInto<f64> for CapsulePtr {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<f64, Self::Error> {
        self.lock_as_value(|v| v.as_f64())?.ok_or_else(|| {
            anyhow!(JuizError::ValueTypeError {
                message: "From<f64> for CapsulePtr failed.".to_owned()
            })
        })
    }
}

impl TryInto<String> for CapsulePtr {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<String, Self::Error> {
        match self.extract_value()? {
            Value::String(s) => Ok(s),
            _ => Err(anyhow!(JuizError::ValueTypeError {
                message: "From<String> for CapsulePtr failed.".to_owned()
            })),
        }
    }
}

impl TryInto<bool> for CapsulePtr {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<bool, Self::Error> {
        self.lock_as_value(|v| v.as_bool())?.ok_or_else(|| {
            anyhow!(JuizError::ValueTypeError {
                message: "From<bool> for CapsulePtr failed.".to_owned()
            })
        })
    }
}

impl TryInto<u64> for CapsulePtr {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<u64, Self::Error> {
        self.lock_as_value(|v| v.as_u64())?.ok_or_else(|| {
            anyhow!(JuizError::ValueTypeError {
                message: "From<u64> for CapsulePtr failed.".to_owned()
            })
        })
    }
}

impl TryInto<Vec<Value>> for CapsulePtr {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<Vec<Value>, Self::Error> {
        match self.extract_value()? {
            Value::Array(a) => Ok(a),
            _ => Err(anyhow!(JuizError::ValueTypeError {
                message: "TryInto<Vec<Value>> for CapsulePtr failed.".to_owned()
            })),
        }
    }
}
impl TryInto<Vec<i64>> for CapsulePtr {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<Vec<i64>, Self::Error> {
        let val: Vec<Value> = self.try_into()?;
        val.into_iter()
            .map(|v| {
                v.as_i64().ok_or_else(|| {
                    anyhow!(JuizError::ValueTypeError {
                        message: "From<Vec<i64>> for CapsulePtr failed.".to_owned()
                    })
                })
            })
            .collect::<JuizResult<Vec<i64>>>()
    }
}
impl TryInto<Vec<f64>> for CapsulePtr {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<Vec<f64>, Self::Error> {
        let val: Vec<Value> = self.try_into()?;
        val.into_iter()
            .map(|v| {
                v.as_f64().ok_or_else(|| {
                    anyhow!(JuizError::ValueTypeError {
                        message: "From<Vec<f64>> for CapsulePtr failed.".to_owned()
                    })
                })
            })
            .collect::<JuizResult<Vec<f64>>>()
    }
}

impl TryInto<DynamicImage> for CapsulePtr {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<DynamicImage, Self::Error> {
        self.extract_image()
    }
}
/// Process::callが返すレスポンスを表す型。実体はCapsulePtrです。
pub type ResponsePtr = CapsulePtr;
