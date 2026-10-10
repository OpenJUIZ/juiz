use std::{path::PathBuf, rc::Rc};
use juiz_sdk::anyhow;

use crate::prelude::*;
use crate::{containers::{ContainerFactoryPtr, ContainerProcessFactoryPtr}, prelude::ProcessFactoryPtr};

use super::{cpp::CppPlugin, python::PythonPlugin, rust::RustPlugin};


#[derive(Clone, Debug)]
pub enum JuizObjectPlugin {
    Rust(Rc<RustPlugin>),
    Python(Rc<PythonPlugin>),
    Cpp(Rc<CppPlugin>),
}


/// 引数vからpathメンバの値を引き出し、nameと連結したPathを作成する
pub fn concat_dirname(v: &Value, name: String, working_dir: Option<PathBuf>) -> JuizResult<PathBuf> {
    match working_dir {
        Some(wd) => Ok(wd.join(obj_get_str(v, "path")?.to_owned()).join(name)),
        None => Ok(PathBuf::from(obj_get_str(v, "path")?.to_owned()).join(name)),
    }
    
}

#[cfg(target_os = "macos")]
pub fn plugin_name_to_file_name(name: &str) -> String {
    "lib".to_owned() + name + ".dylib"
}


#[cfg(target_os = "linux")]
pub fn plugin_name_to_file_name(name: &str) -> String {
    "lib".to_owned() + name + ".so"
}

#[cfg(target_os = "windows")]
pub fn plugin_name_to_file_name(name: &str) -> String {
    name.to_owned() + ".dll"
}

fn plugin_name_to_python_file_name(name: &str) -> String {
    name.to_owned() + ".py"
}


/// まずnameからpluginのファイル名に変換する。macだと.dylibをつける作業。そしてvの中のpathと連結させてpathを作る
fn plugin_path(name: &str, v: &Value, working_dir: Option<PathBuf>) -> JuizResult<std::path::PathBuf> {
    concat_dirname(v, plugin_name_to_file_name(name), working_dir)
}

/// まずnameからpluginのファイル名に変換する。macだと.dylibをつける作業。そしてvの中のpathと連結させてpathを作る
fn python_plugin_path(name: &str, v: &Value, working_dir: Option<PathBuf>) -> JuizResult<std::path::PathBuf> {
    concat_dirname(v, plugin_name_to_python_file_name(name), working_dir)
}

/// まずnameからpluginのファイル名に変換する。macだと.dylibをつける作業。そしてvの中のpathと連結させてpathを作る
fn cpp_plugin_path(name: &str, v: &Value, working_dir: Option<PathBuf>) -> JuizResult<std::path::PathBuf> {
    concat_dirname(v, plugin_name_to_file_name(name), working_dir)
}


impl JuizObjectPlugin {

    pub fn new(language: &str, name: &str, v: &Value, working_dir: Option<PathBuf>, manifest_entry_point: &str, option: &Value) -> JuizResult<JuizObjectPlugin> {
        log::trace!("【呼出】JuizObjectPlugin::new({language}, {name}, {v}, {manifest_entry_point}, {option}");
        match language {
            "rust" => Ok(JuizObjectPlugin::Rust(Rc::new(RustPlugin::load(plugin_path(name, v, working_dir)?)?))),
            "python" => {
                let pythonpaths = match obj_get_array(option, "pythonpath") {
                    Ok(arr_value) => {
                        Some(arr_value.iter().map(|v| { Into::<PathBuf>::into(v.as_str().unwrap()) } ).collect())
                    },
                    Err(_) => None,
                };
                Ok( JuizObjectPlugin::Python(Rc::new(PythonPlugin::load(python_plugin_path(name, v, working_dir)?, pythonpaths)?)))
            },
            "c++" => Ok(JuizObjectPlugin::Cpp(Rc::new(CppPlugin::new(cpp_plugin_path(name, v, working_dir)?, manifest_entry_point)?))),
            _ => {
                log::error!("In setup_container_factories() function, unknown language option ({:}) detected", language);
                Err(anyhow::Error::from(JuizError::InvalidSettingError{message: format!("In setup_container_factories() function, unknown language option ({:}) detected", language)}))
            } 
        }
    }

    pub fn new_rust(filepath: PathBuf) -> JuizResult<JuizObjectPlugin> {
        Ok(JuizObjectPlugin::Rust(Rc::new(RustPlugin::load(filepath)?)))
    }

    pub fn new_python(filepath: PathBuf) -> JuizResult<JuizObjectPlugin> {
        Ok(JuizObjectPlugin::Python(Rc::new(PythonPlugin::load(filepath, None)?)))
    }

    pub fn new_cpp(filepath: PathBuf, manifest_entry_point: &str) -> JuizResult<JuizObjectPlugin> {
        Ok(JuizObjectPlugin::Cpp(Rc::new(CppPlugin::new(filepath, manifest_entry_point)?)))
    }

    pub fn profile_full(&self) -> JuizResult<Value> {
        match self {
            JuizObjectPlugin::Rust(p) => p.profile_full(),
            JuizObjectPlugin::Python(p) => p.profile_full(),
            JuizObjectPlugin::Cpp(p) => p.profile_full(),
        }
    }

    pub fn load_process_factory(&self, working_dir: Option<PathBuf>, symbol_name: &str, type_name_opt: Option<&str>) -> JuizResult<ProcessFactoryPtr> {
        match self {
            JuizObjectPlugin::Rust(p) => {
                p.load_process_factory(working_dir, symbol_name)
            },
            JuizObjectPlugin::Python(p) => {
                p.load_process_factory(working_dir, symbol_name, type_name_opt)
            },
            JuizObjectPlugin::Cpp(p) => {
                p.load_process_factory(working_dir, symbol_name, type_name_opt)
            },
        }
    }

    pub fn load_container_factory(&self, working_dir: Option<PathBuf>, symbol_name: &str, type_name_opt: Option<&str>) -> JuizResult<ContainerFactoryPtr> {
        log::trace!("load_container_factory({working_dir:?}, {symbol_name}, {type_name_opt:?}) called");
        match self {
            JuizObjectPlugin::Rust(p) => {
                p.load_container_factory(working_dir, symbol_name)
                
            },
            JuizObjectPlugin::Python(p) => {
                p.load_container_factory(working_dir, symbol_name, type_name_opt)
            },
            JuizObjectPlugin::Cpp(p) => {
                p.load_container_factory(working_dir, symbol_name, type_name_opt)
                //Ok(Arc::new(Mutex::new(CppContainerFactoryImpl::new_with_manifest(p.clone(), container_profile)?)))
            },
        }
    }

    pub fn load_container_process_factory(&self, working_dir: Option<PathBuf>, symbol_name: &str, type_name_opt: Option<&str>) -> JuizResult<ContainerProcessFactoryPtr> {
        log::trace!("load_container_process_factory({working_dir:?}, {symbol_name}) called");
        match self {
            JuizObjectPlugin::Rust(p) => {
                p.load_container_process_factory(working_dir, symbol_name)
            },
            JuizObjectPlugin::Python(p) => {
                p.load_container_process_factory(working_dir, symbol_name, type_name_opt)
            },
            JuizObjectPlugin::Cpp(p) => {
                p.load_container_process_factory(working_dir, symbol_name, type_name_opt)
                //Ok(Arc::new(Mutex::new(CppContainerProcessFactoryImpl::new_with_manifest(p.clone(), symbol_name, manifest)?)))
            },
        }
    }

    pub fn load_component_manifest(&self, working_dir: Option<PathBuf>) -> JuizResult<ComponentManifest> {
        match self {
            JuizObjectPlugin::Rust(p) => p.load_component_manifest(),
            JuizObjectPlugin::Python(p) => p.load_component_manifest(working_dir),
            JuizObjectPlugin::Cpp(p) => p.load_component_manifest(working_dir),
        }
    }
}

pub trait Plugin {
    fn profile_full(&self) -> JuizResult<Value>;
}