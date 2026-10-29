use crate::manifests::ArgumentProfile;
use crate::manifests::ArgumentType;
use crate::manifests::ProcessProfile;
use crate::prelude::*;
use crate::types::DataType;
use crate::value::*;

// pub fn check_process_factory_manifest(manifest: Value) -> JuizResult<Value> {
//     let _ = obj_get_str(&manifest, "type_name").context("check_process_factory_manifest failed.")?;
//     Ok(manifest)
// }

pub fn check_corebroker_manifest(manifest: Value) -> Result<Value, JuizError> {
    return Ok(manifest);
}

// fn default_value_type_str(def_value: &Value) -> Result<&str, JuizError> {
//     if def_value.is_i64() || def_value.is_u64(){
//         return Ok("int");
//     } else if def_value.is_f64() {
//         return Ok("float");
//     } else if def_value.is_string() {
//         return Ok("string");
//     } else if def_value.is_object() {
//         return Ok("object");
//     } else if def_value.is_array() {
//         return Ok("array");
//     }
//     return Err(JuizError::ManifestArgumentDefaultValueIsNotAvailableValueTypeError{value: def_value.clone()});
// }

// fn check_process_manifest_argument(arg_manifest: &mut Value) -> JuizResult<()> {

//     let def_value =get_value(arg_manifest, "default")?.clone();
//     // デフォルトの型を表す文字列を取得
//     let def_value_type = default_value_type_str(&def_value)?;

//     //let arg_map = get_hashmap_mut(arg_manifest)?;
//     match  obj_get_str(arg_manifest, "type") {
//         Err(_) => {},
//         Ok(type_str) => {
//             if type_str != def_value_type && !(type_str == "image" && def_value_type == "object" ){
//                 return Err(anyhow::Error::from(JuizError::ManifestDefaultValueIsNotExpectedTypeError{value: arg_manifest.clone(), key: "type".to_string(), set_type: type_str.to_string(), expected_type: def_value_type.to_string()}));
//             }
//         }
//     }
//     let arg_map = get_hashmap_mut(arg_manifest)?;
//     arg_map.insert("type".to_string(), jvalue!(def_value_type));
//     match arg_map.get("description") {
//         Some(desc_v) => {
//             if !desc_v.is_string() {
//                 return Err(anyhow::Error::from(JuizError::ManifestValueIsNotExpectedTypeError{value: arg_manifest.clone(), key: "description".to_string(), expected_type: "str".to_string()}));
//             }
//         },
//         None => {}
//     }
//     Ok(())

// }

// fn check_process_manifest_arguments(args_manifest: &mut Value) -> JuizResult<()> {
//     let args_map = get_hashmap_mut(args_manifest)?;
//     for (_arg_name, arg_manif) in args_map.iter_mut() {
//         let _ = check_process_manifest_argument(arg_manif).with_context(||format!("check_process_manifest_arguments(name={}, {:?}) function", _arg_name, arg_manif))?;
//     }
//     Ok(())
// }

pub fn check_connection_manifest(connection_manifest: Value) -> Result<Value, JuizError> {
    Ok(connection_manifest)
}

// pub fn check_process_manifest(mut process_manifest: Value) -> JuizResult<Value> {
//     let _ = obj_get_str(&process_manifest, "name").context("check_process_manifest failed.")?;
//     let _ = obj_get_str(&process_manifest, "type_name").context("check_process_manifest failed.")?;
//     let arg_v  = obj_get_mut(&mut process_manifest, "arguments").context("check_process_manifest failed.")?;
//     check_process_manifest_arguments(arg_v).with_context(||format!("check_process_manifest({})", process_manifest))?;
//     Ok(process_manifest)
// }

fn check_arguments(args_manifest: &Vec<ArgumentProfile>, argument: &CapsuleMap) -> JuizResult<()> {
    //let arg_map = get_hashmap(argument).context("check_arguments")?;
    for arg_manifest in args_manifest.iter() {
        match argument.get(arg_manifest.name.as_str()) {
            Err(_) => {
                log::error!("プロセスのマニフェストチェック中にエラーが発生しました。引数のマニフェストは('{arg_manifest:?}')ですが、この引数が渡されたカプセルの中に見つかりません。渡された引数の名前が間違っているかもしれません。");

                return Err(anyhow::Error::from(
                    JuizError::ArgumentMissingWhenCallingError {
                        process_manifest: args_manifest
                            .iter()
                            .map(|a| serde_json::to_value(a))
                            .collect::<Result<Vec<Value>, serde_json::Error>>()?
                            .into(),
                        missing_arg_name: arg_manifest.name.clone(),
                    },
                ));
            }
            Ok(_) => {}
        };
    }
    Ok(())
}

/// 省いた引数に補う既定値。無ければ None。
///
/// 画像の引数と、既定値が `null` の引数は既定値なしとして扱う。
/// ただし型付きで `T?` の引数は `null` が既定値になる (仕様 docs/spec/datatype.md §6)。
fn default_for_missing(arg_manifest: &ArgumentProfile) -> Option<Value> {
    if arg_manifest.type_name == ArgumentType::Image {
        return None;
    }
    let data_type = match arg_manifest.schema.as_ref() {
        Some(schema) => match schema.validate() {
            Ok(t) => Some(t),
            Err(_) => return None,
        },
        None => None,
    };
    if let Some(t) = data_type.as_ref() {
        if *t == DataType::Image {
            return None;
        }
    }
    if arg_manifest.default.is_null() {
        return match data_type {
            Some(t) if t.is_nullable() => Some(Value::Null),
            _ => None,
        };
    }
    Some(arg_manifest.default.clone())
}

/// call で省かれた引数を manifest の既定値で補う (issue #265)。
///
/// 渡された引数はそのまま使う。既定値の無い引数は補わないので、
/// そのあとの [`check_manifest_before_call`] で ArgumentMissingWhenCallingError になる。
/// p_apply で拘束した値は使わない (call は拘束値を見ない)。
pub fn fill_default_arguments(manifest: &ProcessProfile, mut argument: CapsuleMap) -> CapsuleMap {
    for arg_manifest in manifest.arguments.iter() {
        if argument.get_map().contains_key(arg_manifest.name.as_str()) {
            continue;
        }
        if let Some(default) = default_for_missing(arg_manifest) {
            argument.put(arg_manifest.name.as_str(), default.into());
        }
    }
    argument
}

pub fn check_manifest_before_call(
    manifest: &ProcessProfile,
    argument: &CapsuleMap,
) -> JuizResult<()> {
    check_arguments(&manifest.arguments, argument)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifests::{ArgumentManifest, ProcessManifest};
    use crate::types::TypeSchema;

    fn profile(manifest: ProcessManifest) -> ProcessProfile {
        manifest.name("p").try_into().unwrap()
    }

    fn value_of(args: &CapsuleMap, key: &str) -> Value {
        args.get(key).unwrap().lock_as_value(|v| v.clone()).unwrap()
    }

    #[test]
    fn fill_default_arguments_fills_missing_arguments() {
        let p = profile(
            ProcessManifest::new("t")
                .add_int_arg("a", "", 1)
                .add_string_arg("b", "", "Hello World"),
        );
        let args = fill_default_arguments(&p, vec![("a", jvalue!(5))].into());
        assert_eq!(value_of(&args, "a"), jvalue!(5));
        assert_eq!(value_of(&args, "b"), jvalue!("Hello World"));
        assert!(check_manifest_before_call(&p, &args).is_ok());
    }

    #[test]
    fn fill_default_arguments_keeps_missing_without_default() {
        let p = profile(
            ProcessManifest::new("t")
                .add_arg(ArgumentManifest::new(
                    ArgumentType::Int,
                    "a",
                    "".into(),
                    Value::Null,
                ))
                .add_image_arg("img", ""),
        );
        let args = fill_default_arguments(&p, CapsuleMap::new());
        assert!(args.get("a").is_err());
        assert!(args.get("img").is_err());
        let err = check_manifest_before_call(&p, &args).unwrap_err();
        assert!(matches!(
            err.downcast_ref::<JuizError>(),
            Some(JuizError::ArgumentMissingWhenCallingError { .. })
        ));
    }

    #[test]
    fn fill_default_arguments_uses_null_for_optional_typed_argument() {
        let optional = TypeSchema::from_data_type(&DataType::optional(DataType::Long)).unwrap();
        let required = TypeSchema::from_data_type(&DataType::Long).unwrap();
        let p = profile(
            ProcessManifest::new("t")
                .add_arg(ArgumentManifest::new_typed("opt", optional, Value::Null).unwrap())
                .add_arg(ArgumentManifest::new_typed("req", required, Value::Null).unwrap()),
        );
        let args = fill_default_arguments(&p, CapsuleMap::new());
        assert_eq!(value_of(&args, "opt"), Value::Null);
        assert!(args.get("req").is_err());
    }
}
