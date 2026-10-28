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

    /// 値を取り除いて返す。get と違い参照を残さないので、呼び出し側が中身を複製せずに取り出せる。
    pub fn take(&mut self, key: &str) -> JuizResult<CapsulePtr> {
        self.map.remove(key).ok_or_else(|| {
            anyhow::Error::from(JuizError::CapsuleMapDoesNotContainValueError {
                key: key.to_owned(),
            })
        })
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
        // 値は複製せずに取り出す。画像のような大きい引数では、ここでの複製が 1 回分のコピーになる。
        get_hashmap(&value)?;
        let Value::Object(mut c_map) = value else {
            unreachable!()
        };
        let mut c = CapsuleMap::new();
        if let Some(map_v) = c_map.remove("__map__") {
            get_hashmap(&map_v)?;
            if let Value::Object(map) = map_v {
                for (key, value) in map {
                    c.insert(key, value.into());
                }
            }
            if let Some(param_v) = c_map.get("__param__") {
                for (key, value) in get_hashmap(param_v)?.iter() {
                    c.set_param(key.as_str(), (*value).as_str().unwrap());
                }
            }
        } else {
            for (k, v) in c_map {
                c.insert(k, v.into());
            }
        }
        Ok(c)
    }
}

/// 値 (Value) だけの CapsuleMap を `{"__map__": ..., "__param__": ...}` にする。
/// 画像などの Value にできない値があればエラー (以前は panic していた、issue #233)
impl TryFrom<CapsuleMap> for Value {
    type Error = anyhow::Error;

    fn try_from(capsule_map: CapsuleMap) -> Result<Self, Self::Error> {
        let map_map = capsule_map
            .map
            .into_iter()
            .map(|(k, v)| -> JuizResult<(String, Value)> {
                let v = v.extract_value().map_err(|e| {
                    anyhow::Error::from(JuizError::ValueTypeError {
                        message: format!("argument '{k}' can not be converted to a value ({e})"),
                    })
                })?;
                Ok((k, v))
            })
            .collect::<JuizResult<Map<String, Value>>>()?;

        // jvalue! は map_map を複製する。複製しない形も試したが、HTTP ブローカーの呼び出し側で
        // glibc のヒープ末尾の縮小と再拡張が毎回起き（1 MB の引数で 1 要求あたり約 650 回の
        // ページフォールト）、複製 1 回より遅くなったので、この形のままにしている。
        Ok(jvalue!({
            "__map__": map_map,
            "__param__": capsule_map.param,
        }))
    }
}

impl CapsuleMap {
    /// 値が Value (と Value の Native) だけか。画像があれば false
    pub fn is_all_value(&self) -> JuizResult<bool> {
        for v in self.map.values() {
            if !v.is_value()? && !v.is_empty()? {
                return Ok(false);
            }
        }
        Ok(true)
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
