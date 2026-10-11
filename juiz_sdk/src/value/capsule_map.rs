use serde_json::Map;
use std::collections::HashMap;
use std::fmt::Display;

use crate::prelude::*;
use crate::utils::get_hashmap;

#[repr(C)]
#[derive(Debug)]
pub struct CapsuleMap {
    map: HashMap<String, CapsulePtr>,
    param: HashMap<String, String>,
}

impl Display for CapsuleMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "(map: {:?}, param: {:?})",
            self.map, self.param
        ))
    }
}

impl CapsuleMap {
    pub fn new() -> Self {
        CapsuleMap {
            map: HashMap::new(),
            param: HashMap::new(),
        }
    }

    pub fn set_param(&mut self, key: &str, value: &str) -> &mut Self {
        self.param.insert(key.to_owned(), value.to_owned());
        self
    }

    pub fn get_param(&self, key: &str) -> Option<&String> {
        self.param.get(key)
    }

    pub fn get(&self, key: &str) -> JuizResult<CapsulePtr> {
        match self.map.get(key) {
            Some(v) => Ok(v.clone()),
            None => Err(anyhow::Error::from(
                JuizError::CapsuleMapDoesNotContainValueError {
                    key: key.to_owned(),
                },
            )),
        }
    }

    /*
    pub(crate) fn get_ptrref(&self, key: &str) -> JuizResult<&CapsulePtr> {
        match self.map.get(key) {
            Some(v) => Ok(v),
            None => Err(anyhow::Error::from(JuizError::CapsuleMapDoesNotContainValueError{key: key.to_owned()}))
        }
    }
    */

    pub(crate) fn get_mutref(&mut self, key: &str) -> JuizResult<&mut CapsulePtr> {
        match self.map.get_mut(key) {
            Some(v) => Ok(v),
            None => Err(anyhow::Error::from(
                JuizError::CapsuleMapDoesNotContainValueError {
                    key: key.to_owned(),
                },
            )),
        }
    }

    pub fn get_map<'a>(&'a self) -> &'a HashMap<String, CapsulePtr> {
        &self.map
    }

    pub fn get_params<'a>(&'a self) -> &'a HashMap<String, String> {
        &self.param
    }

    pub fn insert(&mut self, key: String, value: CapsulePtr) -> &Self {
        self.map.insert(key, value);
        self
    }

    pub fn iter(&self) -> std::collections::hash_map::Iter<String, CapsulePtr> {
        self.map.iter()
    }

    pub fn get_int(&self, key: &str) -> JuizResult<i64> {
        Ok(self
            .get(key)?
            .lock_as_value(|value| value.as_i64().unwrap())?)
    }

    pub fn get_str(&self, key: &str) -> JuizResult<String> {
        Ok(self
            .get(key)?
            .lock_as_value(|value| value.as_str().unwrap().to_owned())?)
    }

    pub fn get_str_then<T, F>(&self, key: &str, f: F) -> JuizResult<T>
    where
        F: Fn(&str) -> T,
    {
        Ok(self.get(key)?.lock_as_str(|value| f(value))?)
    }

    pub fn get_map_then<T, F>(&self, key: &str, f: F) -> JuizResult<T>
    where
        F: Fn(&Map<String, Value>) -> T,
    {
        Ok(self.get(key)?.lock_as_map(|value| f(value))?)
    }

    pub fn put(&mut self, key: &str, value: CapsulePtr) -> Option<CapsulePtr> {
        self.map.insert(key.to_owned(), value)
    }
}

impl TryFrom<Value> for CapsuleMap {
    type Error = anyhow::Error;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let mut c = CapsuleMap::new();
        let c_map = get_hashmap(&value)?;
        if c_map.contains_key("__map__") {
            let map_v = c_map.get("__map__").unwrap();
            for (key, value) in get_hashmap(map_v)?.iter() {
                c.insert(key.to_owned(), (*value).clone().into());
            }
            if c_map.contains_key("__param__") {
                let param_v = c_map.get("__param__").unwrap();
                for (key, value) in get_hashmap(param_v)?.iter() {
                    c.set_param(key.as_str(), (*value).as_str().unwrap());
                }
            }
        } else {
            for (k, v) in get_hashmap(&value)?.iter() {
                c.insert(k.to_owned(), v.clone().into());
            }
        }
        Ok(c)
    }
}

impl From<CapsuleMap> for Value {
    fn from(capsule_map: CapsuleMap) -> Self {
        // log::trace!("Value From CapusleMap ({capsule_map:?}) called");
        // let mut v1 = jvalue!({});
        // let map = v1.as_object_mut().unwrap();
        // for (key, value) in capsule_map.map.iter() {
        //     let _ = value.lock_as_value(|vv| -> () {
        //         map.insert(key.clone(), vv.clone());
        //     });
        //     //map.insert(k, Value::try_from(v.lock().unwrap().clone()).unwrap());
        // }
        // let mut v2 = jvalue!({});
        // let param = v2.as_object_mut().unwrap();
        // for (key, value) in capsule_map.param.iter() {
        //     param.insert(key.clone(), jvalue!(value));
        // }

        let map_map = capsule_map
            .map
            .into_iter()
            .map(|(k, v)| -> (String, Value) { (k, v.extract_value().unwrap()) })
            .collect::<Map<String, Value>>();

        return jvalue!({
            "__map__": map_map,
            "__param___": capsule_map.param,
        });
    }
}

impl From<Vec<(std::string::String, CapsulePtr)>> for CapsuleMap {
    fn from(value: Vec<(std::string::String, CapsulePtr)>) -> Self {
        let mut map = CapsuleMap::new();
        for v in value {
            map.insert(v.0, v.1);
        }
        return map;
    }
}

impl From<Vec<(&str, Value)>> for CapsuleMap {
    fn from(value: Vec<(&str, Value)>) -> Self {
        let mut c = CapsuleMap::new();
        for (k, v) in value {
            c.insert(k.to_owned(), v.into());
        }
        c
    }
}

impl From<&[(&str, Value)]> for CapsuleMap {
    fn from(value: &[(&str, Value)]) -> Self {
        let mut c = CapsuleMap::new();
        for (k, v) in value {
            c.insert((*k).to_owned(), (*v).clone().into());
        }
        c
    }
}

/// Process::callに渡すリクエスト（引数の集合）を表す型。実体はCapsuleMapです。
pub type RequestMap = CapsuleMap;
