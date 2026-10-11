use crate::prelude::*;
use serde_json::Map;
use std::{collections::HashMap, fs};
use yaml_rust2::{yaml::Hash, Yaml, YamlLoader};

// use hashlink::LinkedHashMap;

fn yaml_to_value(yv: &Yaml) -> JuizResult<Value> {
    match yv {
        Yaml::Real(v) => Ok(jvalue!(v.parse::<f64>().or_else(|_| {
            Err(anyhow::Error::from(
                JuizError::ConfigFileIsInvalidFormatError {},
            ))
        })?)),
        Yaml::Integer(v) => Ok(jvalue!(v)),
        Yaml::String(v) => Ok(jvalue!(v)),
        Yaml::Boolean(v) => Ok(jvalue!(v)),
        Yaml::Null => Ok(jvalue!({})),
        Yaml::Array(v) => yaml_array_to_value(&v),
        Yaml::Hash(v) => yaml_hash_to_value(v),
        _ => Err(anyhow::Error::from(
            JuizError::ConfigFileIsInvalidFormatError {},
        )),
    }
}

fn yaml_hash_to_value(yv: &Hash) -> Result<serde_json::Value, anyhow::Error> {
    let mut vec: Vec<(String, Value)> = Vec::new();
    for (k, v) in yv {
        match k {
            Yaml::String(kstr) => vec.push((kstr.to_string(), yaml_to_value(v)?)),
            _ => {
                return Err(anyhow::Error::from(
                    JuizError::ConfigFileIsInvalidFormatError {},
                ));
            }
        }
    }
    Ok(jvalue!(Map::from_iter(vec)))
}

fn yaml_array_to_value(vec_yaml: &Vec<Yaml>) -> JuizResult<Value> {
    let mut vec: Vec<Value> = Vec::new();
    for yv in vec_yaml.iter() {
        vec.push(yaml_to_value(yv)?);
    }
    Ok(jvalue!(vec))
}

fn yaml_vec_to_value(yv: Vec<Yaml>) -> JuizResult<Value> {
    // println!("yaml_to_value({yv:?}) called");
    if yv.len() == 1 {
        return yaml_to_value(yv.get(0).unwrap());
    } else if yv.len() == 0 {
        return Ok(jvalue!({}));
    }
    yaml_array_to_value(&yv)
}

pub fn yaml_conf_load(filepath: String) -> JuizResult<Value> {
    yaml_conf_load_inner(filepath).or::<anyhow::Error>(Ok(serde_json::json!({})))
}

fn yaml_conf_load_inner(filepath: String) -> JuizResult<Value> {
    let yaml_string = fs::read_to_string(filepath)?;
    let yaml_value = YamlLoader::load_from_str(&yaml_string)?;
    yaml_vec_to_value(yaml_value)
}

pub fn yaml_conf_load_with(filepath: String, map: HashMap<&str, String>) -> JuizResult<Value> {
    let mut yaml_string = fs::read_to_string(filepath)?;
    for (k, v) in map.into_iter() {
        yaml_string = yaml_string.replace(k, v.as_str());
    }
    let yaml_value = YamlLoader::load_from_str(&yaml_string)?;
    yaml_vec_to_value(yaml_value)
}
