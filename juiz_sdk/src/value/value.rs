use std::collections::HashMap;

pub use serde_json::json as jvalue;
use serde_json::Map;

use crate::prelude::*;
use crate::utils::{
    get_array, get_hashmap,
    manifest_util::{get_array_mut, get_hashmap_mut},
};

#[allow(unused)]
#[repr(transparent)]
//#[repr(C)]
pub struct JValue(pub serde_json::Value);

pub type Value = serde_json::Value;

pub fn load_str(json_str: &str) -> JuizResult<Value> {
    serde_json::from_str(json_str).or_else(|e| Err(anyhow::Error::from(e)))
}

pub fn as_obj<'a>(value: &'a Value) -> JuizResult<&'a serde_json::Map<String, Value>> {
    match value.as_object() {
        None => {
            return Err(anyhow::Error::from(JuizError::ValueIsNotObjectError {
                value: value.clone(),
            }))
        }
        Some(hashmap) => Ok(hashmap),
    }
}

pub fn obj_get<'a>(value: &'a Value, key: &str) -> JuizResult<&'a Value> {
    match value.as_object() {
        None => {
            return Err(anyhow::Error::from(
                JuizError::ValueWithKeyIsNotObjectError {
                    value: value.clone(),
                    key: key.to_string(),
                },
            ))
        }
        Some(hashmap) => match hashmap.get(key) {
            None => {
                return Err(anyhow::Error::from(JuizError::ValueWithKeyNotFoundError {
                    value: value.clone(),
                    key: key.to_string(),
                }))
            }
            Some(value_for_key) => Ok(value_for_key),
        },
    }
}

// pub fn obj_get_into(mut value: Value, key: &str) -> JuizResult<Value> {
//     match value.as_object_mut() {
//         None => return Err(anyhow::Error::from(JuizError::ValueWithKeyIsNotObjectError{value: value.clone(), key: key.to_string()})),
//         Some(hashmap) => {
//             match hashmap.remove(key) {
//                 None => return Err(anyhow::Error::from(JuizError::ValueWithKeyNotFoundError{value: value.clone(), key: key.to_string()})),
//                 Some(value_for_key) => Ok(value_for_key)
//             }
//         }
//     }
// }

pub fn obj_insert(value: &mut Value, key: &str, data: Value) -> JuizResult<()> {
    get_hashmap_mut(value)?.insert(key.to_string(), data);
    Ok(())
}

pub fn obj_merge_mut(value: &mut Value, data: &Value) -> JuizResult<()> {
    let mut_map = get_hashmap_mut(value)?;
    let data_map = get_hashmap(data)?;
    for (k, v) in data_map.iter() {
        mut_map.insert(k.clone(), v.clone());
    }
    Ok(())
}

pub fn value_merge(value: Value, data: &Value) -> JuizResult<Value> {
    if value.is_object() && data.is_object() {
        obj_merge(value, data)
    } else if value.is_array() && data.is_array() {
        array_merge(value, data)
    } else {
        Err(anyhow::Error::from(JuizError::ValueMergeError {
            message: "Two values have different data type.".to_owned(),
        }))
    }
}

pub fn obj_merge(mut value: Value, data: &Value) -> JuizResult<Value> {
    let mut_map = get_hashmap_mut(&mut value)?;
    let data_map = get_hashmap(data)?;
    for (k, v) in data_map.iter() {
        if mut_map.contains_key(k) {
            let vv = mut_map.remove(k).unwrap();
            mut_map.insert(k.clone(), value_merge(vv, v)?);
        } else {
            mut_map.insert(k.clone(), v.clone());
        }
    }
    Ok(value)
}

pub fn array_merge(mut value: Value, data: &Value) -> JuizResult<Value> {
    let mut_vec = get_array_mut(&mut value)?;
    let data_vec = get_array(data)?;
    for v in data_vec.iter() {
        mut_vec.push(v.clone());
    }
    Ok(value)
}

pub fn obj_get_mut<'a>(value: &'a mut Value, key: &str) -> JuizResult<&'a mut Value> {
    {
        if !value.is_object() {
            return Err(anyhow::Error::from(
                JuizError::ValueWithKeyIsNotObjectError {
                    value: value.clone(),
                    key: key.to_string(),
                },
            ));
        }

        {
            let hashmap = value.as_object().unwrap();
            if hashmap.contains_key(key) {
                return Ok(value.as_object_mut().unwrap().get_mut(key).unwrap());
            }
        }
    }
    Err(anyhow::Error::from(JuizError::ValueWithKeyNotFoundError {
        value: value.clone(),
        key: key.to_string(),
    }))
}

pub fn obj_get_str<'a>(value: &'a Value, key: &str) -> JuizResult<&'a str> {
    let obj = obj_get(value, key)?;
    match obj.as_str() {
        None => {
            log::error!("obj_get_str(value='{value:}', key='{key:}') called, but failed.");
            Err(anyhow::Error::from(
                JuizError::ValueWithKeyIsNotStringError {
                    value: value.clone(),
                    key: key.to_string(),
                },
            ))
        }
        Some(s) => return Ok(s),
    }
}

pub fn obj_get_i64<'a>(value: &'a Value, key: &str) -> JuizResult<i64> {
    let obj = obj_get(value, key)?;
    match obj.as_i64() {
        None => Err(anyhow::Error::from(JuizError::ValueWithKeyIsNotNumError {
            value: value.clone(),
            key: key.to_string(),
        })),
        Some(s) => return Ok(s),
    }
}

pub fn obj_get_f64<'a>(value: &'a Value, key: &str) -> JuizResult<f64> {
    let obj = obj_get(value, key)?;
    match obj.as_f64() {
        None => match obj.as_i64() {
            None => Err(anyhow::Error::from(JuizError::ValueWithKeyIsNotNumError {
                value: value.clone(),
                key: key.to_string(),
            })),
            Some(i) => return Ok(i as f64),
        },
        Some(s) => return Ok(s),
    }
}

pub fn obj_get_bool(value: &Value, key: &str) -> JuizResult<bool> {
    let obj = obj_get(value, key)?;
    match obj.as_bool() {
        None => Err(anyhow::Error::from(JuizError::ValueWithKeyIsNotBoolError {
            value: value.clone(),
            key: key.to_string(),
        })),
        Some(s) => return Ok(s),
    }
}
pub fn obj_get_obj<'a>(value: &'a Value, key: &str) -> JuizResult<&'a Map<String, Value>> {
    let obj = obj_get(value, key)?;
    match obj.as_object() {
        None => Err(anyhow::Error::from(
            JuizError::ValueWithKeyIsNotObjectError {
                value: value.clone(),
                key: key.to_string(),
            },
        )),
        Some(s) => return Ok(s),
    }
}

pub fn obj_get_array<'a>(value: &'a Value, key: &str) -> JuizResult<&'a Vec<Value>> {
    let obj = obj_get(value, key)?;
    match obj.as_array() {
        None => Err(anyhow::Error::from(
            JuizError::ValueWithKeyIsNotArrayError {
                value: value.clone(),
                key: key.to_string(),
            },
        )),
        Some(s) => return Ok(s),
    }
}

pub fn obj_get_hashmap<'a>(value: &'a Value, key: &str) -> JuizResult<HashMap<String, Value>> {
    let map = obj_get_obj(&value, key)?;
    let mut ret_map: HashMap<String, Value> = HashMap::with_capacity(map.len());
    for (k, v) in map.iter() {
        ret_map.insert(k.clone(), v.clone());
    }
    Ok(ret_map)
}
