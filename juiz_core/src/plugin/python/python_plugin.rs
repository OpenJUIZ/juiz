
use std::{collections::HashMap, fs, io::{BufWriter, Cursor}, path::PathBuf, sync::Arc};
use image::ImageFormat;
use pyo3::{prelude::*, types::{PyBool, PyBytes, PyDict, PyFloat, PyInt, PyList, PyNone, PySet, PyString, PyTuple}};
use juiz_sdk::serde_json::Map;
use juiz_sdk::anyhow::{self, anyhow};
use crate::{containers::{bind_container_function, container_factory_create, container_process_factory_create_from_trait}, prelude::*, processes::process_factory_create_from_trait};

// #[cfg(feature="opencv4")]
// use crate::opencv::prelude::*;

pub struct PythonPlugin {
    path: PathBuf,
    pythonpaths: Option<Vec<PathBuf>>,
}


impl std::fmt::Debug for PythonPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PythonPlugin").field("path", &self.path).field("pythonpaths", &self.pythonpaths).finish()
    }
}

pub struct PythonContainerStruct {
    pub pyobj: Py<PyAny>
}

/*
pub type Symbol<'lib, T> = libloading::Symbol<'lib, libloading::Symbol<'lib, T>>;
*/
impl PythonPlugin {

    pub fn profile_full(&self) -> JuizResult<Value> {
        Ok(jvalue!({
            "path": self.path,
        }))
    }

    pub fn load(path: PathBuf, pythonpaths: Option<Vec<PathBuf>>) -> JuizResult<PythonPlugin> {
        log::trace!("【呼出】PythonPlugin::load({:?})", path);
        Ok(PythonPlugin{path, pythonpaths})
    }

    fn init_path(&self, working_dir: Option<PathBuf>) -> JuizResult<()> {
        let fullpath = working_dir.clone().unwrap_or(env!("CARGO_MANIFEST_DIR").into()).join(self.path.clone());
        let pythonpaths = self.pythonpaths.clone();
        log::debug!("【デバッグ情報】pythonpaths:{pythonpaths:?}");
        Python::with_gil(|py| -> PyResult<()> {
            log::trace!("in get_manifest_with_name(), Python:with_gil called (fullpath={:?}", fullpath.clone());
            log::debug!("【デバッグ情報】プラグインが使用するPythonのバージョン情報 ({:?})", py.version_info());
            // サブモジュールのためにルートディレクトリをpathに入れとく。
            let parent = fullpath.parent().unwrap().to_str().unwrap();
            let _ = PyModule::from_code_bound(py, &format!(r#"
import sys
if not "{parent:}" in sys.path:
    sys.path.append("{parent:}")
            "#), "", "");
            if let Some(paths) = pythonpaths {
                paths.iter().for_each(|p| {
                    let path = if p.is_absolute() { p.clone() } else { fullpath.parent().unwrap().join(p) };
                    let path_str = path.to_str().unwrap();
                    let _ = PyModule::from_code_bound(py, &format!(r#"
import sys
if not "{path_str:}" in sys.path:
    sys.path.append("{path_str:}")
            "#), "", "");
                });
            }
            Ok(())
        })?;
        Ok(())
    }

    #[allow(unused)]
    fn get_manifest_with_name(&self, working_dir: Option<PathBuf>, symbol_name: &str) -> JuizResult<Value> {
        let fullpath = working_dir.clone().unwrap_or(env!("CARGO_MANIFEST_DIR").into()).join(self.path.clone());
        let pythonpaths = self.pythonpaths.clone();
        match Python::with_gil(|py| -> PyResult<Value> {
            log::trace!("in get_manifest_with_name(), Python:with_gil called (fullpath={:?}", fullpath.clone());
            log::debug!("PythonPlugin uses python version={:?}", py.version_info());
            // サブモジュールのためにルートディレクトリをpathに入れとく。
            let parent = fullpath.parent().unwrap().to_str().unwrap();
            let _ = PyModule::from_code_bound(py, &format!(r#"
import sys
if not "{parent:}" in sys.path:
    sys.path.append("{parent:}")
            "#), "", "");
            if let Some(paths) = pythonpaths {
                paths.iter().for_each(|p| {
                    let path = if p.is_absolute() { p.clone() } else { fullpath.join(p) };
                    let path_str = path.to_str().unwrap();
                    let _ = PyModule::from_code_bound(py, &format!(r#"
import sys
if not "{path_str:}" in sys.path:
    sys.path.append("{path_str:}")
            "#), "", "");
                });
            }
            
            let py_app = fs::read_to_string(fullpath.clone())?;
            let module = PyModule::from_code_bound(py, &py_app, "", "")?;
            match module.getattr(symbol_name) {
                Ok(manifest_bound) => {
                    let manifest_func: Py<PyAny> = manifest_bound.into();
                    Ok(pydict_to_value(manifest_func.call0(py)?.extract::<&PyDict>(py)?)?) // 関数コールしてPyDictを抽出してvalueに変換する

                },
                Err(e) => {
                    Err(e)
                },
            }
//            let manifest_func: Py<PyAny> = module.getattr(symbol_name)?.into();
        }) {
            Ok(manifest) => { Ok(manifest) }
            Err(e) => {
                log::error!("get_manifest_with_name() failed. {e:}");
                Err(anyhow::Error::from(e))
            }
        }
    }

    fn get_component_manifest_with_name(&self, working_dir: Option<PathBuf>, symbol_name: &str) -> JuizResult<Value> {
        let fullpath = working_dir.clone().unwrap_or(env!("CARGO_MANIFEST_DIR").into()).join(self.path.clone());
        let pythonpaths = self.pythonpaths.clone();
        match Python::with_gil(|py| -> anyhow::Result<Value> {
            log::trace!("in get_manifest_with_name(), Python:with_gil called (fullpath={:?}", fullpath.clone());
            log::debug!("PythonPlugin uses python version={:?}", py.version_info());
            // サブモジュールのためにルートディレクトリをpathに入れとく。
            let parent = fullpath.parent().unwrap().to_str().unwrap();
            let _ = PyModule::from_code_bound(py, &format!(r#"
import sys
if not "{parent:}" in sys.path:
    sys.path.append("{parent:}")
            "#), "", "");
            if let Some(paths) = pythonpaths {
                paths.iter().for_each(|p| {
                    let path = if p.is_absolute() { p.clone() } else { fullpath.join(p) };
                    let path_str = path.to_str().unwrap();
                    let _ = PyModule::from_code_bound(py, &format!(r#"
import sys
if not "{path_str:}" in sys.path:
    sys.path.append("{path_str:}")
            "#), "", "");
                });
            }
            
            let py_app = fs::read_to_string(fullpath.clone())?;
            let module = PyModule::from_code_bound(py, &py_app, "", "")?;
            match module.getattr(symbol_name) {
                Ok(manifest_bound) => {
                    let manifest_func: Py<PyAny> = manifest_bound.into();
                    Ok(pydict_to_value(manifest_func.call0(py)?.extract::<&PyDict>(py)?)?) // 関数コールしてPyDictを抽出してvalueに変換する

                },
                Err(_e) => {
                    let attrs = module.dir()?;
                    // println!("attrs: {attrs:?}");
                    if attrs.is_instance_of::<PyList>() {
                        let attr_list = attrs.extract::<&PyList>()?;
                        let attr_name_list = attr_list.iter().map(|attr| {Ok(attr.str()?.to_string())}).filter(|s| { s.is_ok() && !s.as_ref().unwrap().starts_with("_") }).collect::<anyhow::Result<Vec<String>>>().unwrap();
                        let attrs = attr_name_list.iter().map(|attr_name| { 
                            Ok((attr_name.clone(), Into::<Py<PyAny>>::into(module.getattr(attr_name.as_str())?)))
                        }).collect::<anyhow::Result<Vec<(String, Py<PyAny>)>>>();
                        //println!("attrs: {attrs:?}");
                        let procs = attrs.as_ref().unwrap().iter().filter(|(_attr_name, attr)| {
                            attr.to_string().contains("juiz.decorators.JuizProcess object")
                        }).collect::<Vec<&(String, Py<PyAny>)>>();
                        //println!("procs: {procs:?}");
                        let conts = attrs.as_ref().unwrap().iter().filter(|(_attr_name, attr)| {
                            attr.to_string().contains("juiz.decorators.JuizContainer object")
                        }).collect::<Vec<&(String, Py<PyAny>)>>();
                        //println!("conts: {conts:?}");
                        let cont_procs = attrs.as_ref().unwrap().iter().filter(|(_attr_name, attr)| {
                            attr.to_string().contains("juiz.decorators.JuizContainerProcess object")
                        }).collect::<Vec<&(String, Py<PyAny>)>>();
                        //println!("cont_procs: {cont_procs:?}");
                        let procs_vec = procs.iter().map(|(_proc_name, proc_obj)| {
                            let manifest_function = proc_obj.getattr(py, "manifest")?;
                            let manifest_pyobj = manifest_function.call0(py)?;
                            pyany_to_value(manifest_pyobj.extract::<&PyAny>(py)?).or_else(|e| {
                                Err(anyhow!(e))
                            })
                        }).collect::<anyhow::Result<Vec<Value>>>()?;
                        let mut conts_vec = conts.iter().map(|(_cont_name, cont_obj)| {
                            let manifest_function = cont_obj.getattr(py, "manifest")?;
                            let manifest_pyobj = manifest_function.call0(py)?;
                            pyany_to_value(manifest_pyobj.extract::<&PyAny>(py)?).or_else(|e| {
                                Err(anyhow!(e))
                            })
                        }).collect::<anyhow::Result<Vec<Value>>>()?;
                        let _cont_procs_vec = cont_procs.iter().map(|(_cont_proc_name, cont_proc_obj)| {
                            let manifest_function = cont_proc_obj.getattr(py, "manifest")?;
                            let manifest_pyobj = manifest_function.call0(py)?;
                            let manif_value = pyany_to_value(manifest_pyobj.extract::<&PyAny>(py)?).or_else(|e| {
                                Err(anyhow!(e))
                            })?;
                            let container_type = manif_value.as_object().unwrap().get("container_type").unwrap().as_str().unwrap().to_owned();
                            for cont in conts_vec.iter_mut() {
                                let cont_obj = cont.as_object_mut().unwrap();
                                let cont_type_name = cont_obj.get("type_name").unwrap().as_str().unwrap().to_owned();
                                if cont_type_name == container_type {
                                    let proc_obj = cont_obj.get_mut("processes").unwrap().as_array_mut().unwrap();
                                    proc_obj.push(manif_value.clone());
                                }
                            }
                            // println!("manif_value: {manif_value:}");
                            Ok(manif_value)
                        }).collect::<anyhow::Result<Vec<Value>>>()?;
                        let type_name = fullpath.file_stem().unwrap().to_str().unwrap();
                        let manual_manifest_dic = serde_json::json!({
                            "type_name": type_name,
                            "description": "",
                            "language": "python",
                            "processes": procs_vec,
                            "containers": conts_vec,
                        });
                        //println!("manual_manifest: {manual_manifest_dic:}");
                        Ok(manual_manifest_dic)
                    } else {
                        todo!("Listじゃないのはまずい。")
                    }
                    //todo!("ここでcomponent_manifestを自動生成したい")
                },
            }
//            let manifest_func: Py<PyAny> = module.getattr(symbol_name)?.into();
        }) {
            Ok(manifest) => { Ok(manifest) }
            Err(e) => {
                log::error!("get_manifest_with_name() failed. {e:}");
                Err(anyhow::Error::from(e))
            }
        }
    }

    pub fn load_process_factory(&self, working_dir: Option<PathBuf>, symbol_name: &str, type_name_opt: Option<&str>) -> JuizResult<ProcessFactoryPtr> {
        log::trace!("【呼出】PythonPlugin({:?})::load_process_factory(working_dir={working_dir:?}, symbol_name='{symbol_name}, type_name_opt={type_name_opt:?}')", self.path);
        self.init_path(working_dir.clone())?;
        // let type_name = self.path.file_stem().unwrap().to_str().unwrap();
        let type_name = match type_name_opt {
            Some(v) => v,
            None => {
                log::trace!("type_name_optが与えられませんでした。{:?}より類推します。", self.path);
                self.path.file_stem().unwrap().to_str().unwrap()
            }
        };
        log::trace!("【load_process_factory】type_name = {type_name}");
        let fullpath = working_dir.unwrap_or(env!("CARGO_MANIFEST_DIR").into()).join(self.path.clone());
        log::trace!("【load_process_factory】fullpath = {type_name}");
        let mut manifest = jvalue!({});
        let py_app = fs::read_to_string(fullpath.clone()).unwrap();
        let pyfunc2 = Python::with_gil(|py| -> PyResult<Py<PyAny>> {
            let module = PyModule::from_code_bound(py, &py_app.to_string(), fullpath.clone().to_str().unwrap(), "")?;
            //let tuple = module.getattr(symbol_name)?.into_py(py).call0(py)?;
            // manifest = pyany_to_value(pytuple.get_item(0)?)?;
            let proc_object = module.getattr(type_name)?.into_py(py);// .call0(py)?;
            //println!("tuple: {:?}", tuple.to_string());
            //let pytuple = tuple.extract::<&PyTuple>(py)?;
            let manifest_object = proc_object.getattr(py, "manifest")?.into_py(py).call0(py)?;
            manifest = pyany_to_value(manifest_object.extract::<&PyAny>(py)?)?;
            let pyfunc = proc_object;
            //Ok(tuple)
            Ok(pyfunc.to_object(py))
        })?;
        let signature = get_python_function_signature(&pyfunc2)?;
        log::trace!("Python関数の読み込みに成功。宣言： {manifest:}, シグネチャ： {signature}");
        let function = move |argument: CapsuleMap| -> JuizResult<Capsule> {
            Python::with_gil(|py| {
                python_process_call(py, &pyfunc2, PyTuple::new_bound(py, capsulemap_to_pytuple(py, &argument, &signature, 0)?))
            }).or_else(|e| { Err(anyhow!(e)) })
        };
        let result_process_manifest = serde_json::from_value::<ProcessManifest>(manifest);
        if result_process_manifest.is_err() {
            log::error!("【load_process_factory】【失敗】エラー({result_process_manifest:?})");
        }
        process_factory_create_from_trait(result_process_manifest?, function)
    }
    
    // pub fn load_container_factory_with_manifest(&self, working_dir: Option<PathBuf>, manifest: Value) -> JuizResult<ContainerFactoryPtr> {
    //     Ok(ContainerFactoryPtr::new(PythonContainerFactoryImpl::new(
    //         manifest,
    //         working_dir.unwrap_or(env!("CARGO_MANIFEST_DIR").into()).join(self.path.clone())
    //     )?))
    // }

    pub fn load_container_process_factory(&self, working_dir: Option<PathBuf>, symbol_name: &str, type_name_opt: Option<&str>) -> JuizResult<ContainerProcessFactoryPtr> {
        log::trace!("PythonPlugin({:?})::load_container_process_factory(symbol_name='{symbol_name}') called", self.path);
        self.init_path(working_dir.clone())?;
        let type_name = match type_name_opt {
            Some(v) => v,
            None => self.path.file_stem().unwrap().to_str().unwrap()
        };
        // self.load_container_process_factory_with_manifest(working_dir.clone(), cp_profile.clone(), symbol_name)
        let fullpath = working_dir.unwrap_or(env!("CARGO_MANIFEST_DIR").into()).join(self.path.clone());
        let mut manifest = jvalue!({});
        let py_app = fs::read_to_string(fullpath.clone()).unwrap();
        let pyfunc2 = match Python::with_gil(|py| -> PyResult<Py<PyAny>> {
            log::debug!("Python::with_gil({py_app}) for load_Container_process_factory({symbol_name}, {type_name_opt:?}), fullpath={fullpath:?}");
            let module = PyModule::from_code_bound(py, &py_app.to_owned(), fullpath.clone().to_str().unwrap(), "")?;
            let proc_object = module.getattr(type_name)?.into_py(py);
            let manifest_object = proc_object.getattr(py, "manifest")?.into_py(py).call0(py)?;
            manifest = pyany_to_value(manifest_object.extract::<&PyAny>(py)?)?;
            let pyfunc = proc_object;

            Ok(pyfunc.to_object(py))
        }) {
            Ok(v) => Ok(v),
            Err(e) => {
                Python::with_gil(|py| { 
                    log::error!("Python::with_gil() failed. Error({e:?})");
                    if let Some(tb) = e.traceback_bound(py) {
                        log::error!("Traceback: {}", tb);
                    }
                });
                Err(e)
            }
        }?;

        let signature = get_python_function_signature(&pyfunc2)?;
        let function = move |container: &mut ContainerImpl<PythonContainerStruct>, argument: CapsuleMap| -> JuizResult<Capsule> {
            // println!("container process impl called: {argument:?}");
            Python::with_gil(|py| {
                let start_index = 1;
                let v  = capsulemap_to_pytuple(py, &argument, &signature, start_index)?;
                let elements = arg_to_pyargs(container, &v);
                let return_value = python_process_call(py, &pyfunc2, PyTuple::new_bound(py, elements));
                // println!("return_value : {return_value:?}");
                return_value
            }).or_else(|e| { Err(anyhow!(e)) })
        };
    
        container_process_factory_create_from_trait(serde_json::from_value(manifest)?, bind_container_function(function)).or_else(|e| {
            log::error!("container_process_factory_create_from_trait() failed.");
            Err(e)
        })
    
    }

    // fn load_container_process_factory_with_manifest(&self, working_dir: Option<PathBuf>, manifest: Value, symbol_name: &str) -> JuizResult<ContainerProcessFactoryPtr> {
    //     Ok(ContainerProcessFactoryPtr::new(PythonContainerProcessFactoryImpl::new(
    //         manifest,
    //         working_dir.unwrap_or(env!("CARGO_MANIFEST_DIR").into()).join(self.path.clone()),
    //         symbol_name
    //     )?))
    // }


    pub fn load_container_factory(&self, working_dir: Option<PathBuf>, symbol_name: &str, type_name_opt: Option<&str>) -> JuizResult<ContainerFactoryPtr> {
        log::trace!("【呼出】PythonPlugin({:?})::load_container_factory(symbol_name='{symbol_name}') called", self.path);
        
        let type_name = match type_name_opt {
            Some(v) => v,
            None => self.path.file_stem().unwrap().to_str().unwrap()
        };
        //let type_name = ;
        let fullpath = working_dir.unwrap_or(env!("CARGO_MANIFEST_DIR").into()).join(self.path.clone());
        let mut manifest = jvalue!({});
        let py_app = fs::read_to_string(fullpath.clone()).unwrap();
        let pyfunc2 = Python::with_gil(|py| -> PyResult<Py<PyAny>> {
            let module = PyModule::from_code_bound(py, &py_app.to_string(), fullpath.clone().to_str().unwrap(), "")?;
            //let tuple = module.getattr(symbol_name)?.into_py(py).call0(py)?;
            //let pytuple = tuple.extract::<&PyTuple>(py)?;
            //manifest = pyany_to_value(pytuple.get_item(0)?)?;
            //let pyfunc = pytuple.get_item(1)?.extract::<&PyFunction>()?;
            
            //let tuple = module.getattr(symbol_name)?.into_py(py).call0(py)?;
            

            let proc_object = module.getattr(type_name)?.into_py(py);// .call0(py)?;
            // println!("loaded proc_object: {proc_object:?}");
            //println!("tuple: {:?}", tuple.to_string());
            //let pytuple = tuple.extract::<&PyTuple>(py)?;
            let manifest_object = proc_object.getattr(py, "manifest")?.into_py(py).call0(py)?;
            manifest = pyany_to_value(manifest_object.extract::<&PyAny>(py)?)?;
            let pyfunc = proc_object;
            
            Ok(pyfunc.to_object(py))
        })?;
        let signature = get_python_function_signature(&pyfunc2)?;
        let constructor = move |cm: ContainerManifest, argument: CapsuleMap| -> JuizResult<ContainerPtr> {
            let pyobj = Python::with_gil(|py| {
            //    let v: Value = arg.into();
                let start_index = 0;
                let v  = capsulemap_to_pytuple(py, &argument, &signature, start_index).unwrap();
                //let elements = arg_to_pyargs(container, &v);
                //let return_value = python_process_call(py, &pyfunc2, PyTuple::new_bound(py, v));
                //return_value
                pyfunc2.call1(py, PyTuple::new_bound(py,  v))
            })?;
            Ok(ContainerPtr::new(ContainerImpl::new(cm, Box::new(PythonContainerStruct{
                pyobj,
            }))?))
        };
        
        container_factory_create(serde_json::from_value(manifest)?, Arc::new(constructor))
    }

    pub fn load_component_manifest(&self, working_dir: Option<PathBuf>) -> JuizResult<ComponentManifest> {
        log::trace!("【呼出】load_component_manifest()");
        match self.get_component_manifest_with_name(working_dir, "component_manifest") {
            Ok(manif) => {
                serde_json::from_value(manif).or_else(|e| {
                    Err(anyhow!(e))
                })
            },
            Err(_e) => {
                // 自動でコンポーネントのマニフェストを生成できるかどうかやってみる場合
                todo!("自動でコンポーネントのマニフェストを生成できるかどうかやってみる場合")
            }
        }
    }
}

fn arg_to_pyargs<'a>(c: &'a mut ContainerImpl<PythonContainerStruct>, arg: &'a Vec<Py<PyAny>> ) -> Vec<&'a Py<PyAny>> {
    let mut vec_arg: Vec<&Py<PyAny>> = Vec::new();
    vec_arg.push(&c.t.pyobj);
    vec_arg.extend(arg.iter());
    vec_arg
}

pub fn get_python_function_signature(func: &Py<PyAny>) -> PyResult<Value> {
    Python::with_gil(|py| {
        let py_app = r#"
from inspect import signature
def argument_info(func):
    sig = signature(func)
    sig_dict = []
    for k, v in sig.parameters.items():
        sig_dict.append( {
            "key": k,
            "kind": str(v.kind),
            "name": v.name,
            "annotation": str(v.annotation.__name__) if v.annotation is not v.empty else "empty",
            "default": str(v.default) if v.default is not v.empty else "empty",
        } )
    return sig_dict
        "#;
        let module = PyModule::from_code_bound(py, &py_app.to_string(), "", "")?;
        let value = module.getattr("argument_info")?.into_py(py).call1(py, PyTuple::new_bound(py,[func.clone_ref(py)]))?;
        pyany_to_value(value.extract::<&PyAny>(py)?)
    })
}

// pub fn get_entry_point(fullpath: &PathBuf, symbol_name: &str) -> PyResult<Py<PyAny>> {
//     Python::with_gil(|py| -> PyResult<Py<PyAny>> {
//         let py_app = fs::read_to_string(fullpath).unwrap();
//         let module = PyModule::from_code_bound(py, &py_app.to_string(), fullpath.to_str().unwrap(), "")?;
//         module.getattr(symbol_name)?.into_py(py).call0(py)
//     })
// }



pub fn check_object_is_ndarray(_py: &Python, value: &PyAny) -> bool {
    value.get_type().to_string() == "<class 'numpy.ndarray'>".to_owned()
}

fn function_argument_name_list(signature: &Value) -> JuizResult<Vec<String>> {
    Ok(get_array(signature)?.iter().map(|v| { obj_get_str(v, "name").unwrap().to_owned() }).collect::<Vec<String>>())
}

pub fn capsulemap_to_pytuple<'a>(py: Python, value: &'a CapsuleMap, signature: &Value, start_index: usize) -> JuizResult<Vec<Py<PyAny>>> {
    
    let arg_name_list = function_argument_name_list(signature)?;
    let mut index = start_index;
    let mut vec: Vec<Py<PyAny>> = Vec::new();
    for (i, arg_name) in arg_name_list.iter().enumerate() {
        if i < index { continue; };
        let v = value.get(arg_name.as_str())?;
        vec.push(capsuleptr_to_pyany(py, &v));
        index += 1;
    }
    return Ok(vec);
    // value.iter().map(|(_k, v)| { 
    //     let arg_name = arg_name_list.get(index);
    //     capsuleptr_to_pyany(py, v)

    // } ).collect::<Vec<Py<PyAny>>>()
}
#[allow(unused)]
pub fn value_to_pytuple<'a>(py: Python, value: &'a Value) -> Vec<Py<PyAny>> {
    vec!(value_to_pyany(py, value))
}

fn valuearray_to_pyany(py: Python, arr: &Vec<Value>) -> Py<PyAny> {
    arr.iter().map(|v| { value_to_pyany(py, v) }).collect::<Vec<Py<PyAny>>>().into_py(py)
}

fn valueobj_to_pyany(py: Python, map: &Map<String, Value>) -> Py<PyAny> {
    map.iter().map(|(k, v)| { (k.clone(), value_to_pyany(py, v)) }).collect::<HashMap<String, Py<PyAny>>>().into_py(py)
}

fn value_to_pyany(py: Python, value: &Value) -> Py<PyAny> {
    if value.is_i64() {
        return (value.as_i64().unwrap()).into_py(py);
    } else if value.is_f64() {
        return (value.as_f64().unwrap()).into_py(py);
    } else if value.is_boolean() {
        return (value.as_bool().unwrap()).into_py(py);
    } else if value.is_string() {
        return (value.as_str().unwrap()).into_py(py);
    } else if value.is_null() {
        return (value.as_null().unwrap()).into_py(py);
    } else if value.is_u64() { 
        return (value.as_u64().unwrap()).into_py(py);
    } else if value.is_array() { 
        return valuearray_to_pyany(py, value.as_array().unwrap());
    } else if value.is_object() {
        return valueobj_to_pyany(py, value.as_object().unwrap());
    }
    todo!()
}

fn capsuleptr_to_pyany(py: Python, value: &CapsulePtr) -> Py<PyAny> {
    if value.is_value().unwrap() {
        return value.lock_as_value(|v| {
            value_to_pyany(py, v)
        }).unwrap();
    } else if value.is_image().unwrap() {
        return value.lock_as_image(|img| {
            image_to_pyany(py, img)
        }).unwrap()
    }
    todo!("capsuleptr_to_pyany failed. CapsulePtr type is not available yet. Value is {value:?}")
}

fn image_to_pyany(py: Python, image: &DynamicImage) -> Py<PyAny> {
    let mut buffer = BufWriter::new(Cursor::new(Vec::new()));
    image.write_to(&mut buffer, ImageFormat::Bmp).unwrap();
    //image.as_bytes()
    let w = image.width();
    let h = image.height();
    unsafe {
        let py_app = r"
import io
from PIL import Image
def image_from_bytes(w, h, image_data):
    # print('image_data', image_data, type(image_data))
    return Image.frombytes('RGB', (w,h), image_data)
    # return Image.open(io.BytesIO(image_data))
";
        let b = PyBytes::bound_from_ptr(py, image.as_bytes().as_ptr(), image.as_bytes().len());
        let module = PyModule::from_code_bound(py, &py_app.to_string(), "", "").unwrap();
        module.getattr("image_from_bytes").unwrap().into_py(py).call1(py, PyTuple::new_bound(py, [w.into_py(py), h.into_py(py), b.into()])).unwrap()
    }
}

// pub fn python_process_call(py: Python, entry_point: &Py<PyAny>, arguments: Vec<Py<PyAny>>) -> JuizResult<Capsule> {
//     match entry_point.call1(py, PyTuple::new_bound(py, arguments)) {
//         Ok(result) => {
//             Ok(pyany_to_value(result.extract::<&PyAny>(py)?)?.into())
//         }
//         Err(e) => {
//             log::error!("Error calling python call. {e:}");
//             Err(anyhow::Error::from(e))
//         }
//     }
// }


// #[cfg(feature="opencv4")]
// pub fn python_process_call(py: Python, entry_point: &Py<PyAny>, pytuple: pyo3::Bound<PyTuple>) -> JuizResult<Capsule> {
//     match entry_point.call1(py, pytuple) {
//         Ok(v) => {
//             let object = v.extract::<&PyAny>(py)?;
//             Ok(if check_object_is_ndarray(&py, object) {
//                 pyany_to_mat(py, object).unwrap()
//             } else {
//                 pyany_to_value(object)?.into()
//             })
//         },
//         Err(e) => {
//             let trace_str = e.traceback_bound(py).and_then(|trace| { Some(format!("{:}", trace.format().unwrap())) }).or(Some("".to_owned())).unwrap();
//             log::error!("Error in Python::with_gil for ContainerProcess.call(). Error({e:}). Traceback: {trace_str:}");
//             Err(anyhow!(e))
//         },
//     }

// }

// #[cfg(not(feature="opencv4"))]
pub fn python_process_call(py: Python, entry_point: &Py<PyAny>, pytuple: pyo3::Bound<PyTuple>) -> JuizResult<Capsule> {
    log::trace!("【呼出】python_process_call()");
    match entry_point.call1(py, pytuple) {
        Ok(v) => {
            let object = v.extract::<&PyAny>(py)?;
            Ok(if check_object_is_ndarray(&py, object) {
                //pyany_to_mat(py, object).unwrap()
                todo!()
            } else {
                // println!("pyany_to_value: {object:?}");
                pyany_to_capsule(object)?.into()
            })
        },
        Err(e) => {
            let trace_str = e.traceback_bound(py).and_then(|trace| { Some(format!("{:}", trace.format().unwrap())) }).or(Some("".to_owned())).unwrap();
            log::error!("Error in Python::with_gil for ContainerProcess.call(). Error({e:}). Traceback: {trace_str:}");
            Err(anyhow!(e))
        },
    }

}


// #[cfg(feature="opencv4")]
// fn pytuple_to_mat(pytuple: &PyTuple) -> JuizResult<Capsule> {
//     let shape = pytuple.get_item(0)?.extract::<&PyTuple>()?.into_iter().map(|v|{v.extract::<i32>().unwrap()}).collect::<Vec<i32>>();
//     Ok(opencv::core::Mat::new_rows_cols_with_data(
//         shape[0] * shape[2],
//         shape[1],
//         pytuple.get_item(1)?.extract::<&PyBytes>()?.as_bytes()
//     )?.reshape(3, shape[0])?.try_clone()?.into())
// }


// // #[cfg(feature="opencv4")]
// pub fn pyany_to_mat(py: Python, object: &PyAny) -> JuizResult<Capsule> {
//     let py_app = r#"
// import cv2
// def to_tuple(mat):
//     return (mat.shape, mat.data.tobytes())
//     "#;
//     let symbol_name = "to_tuple";
//     let module = PyModule::from_code_bound(py, &py_app, "", "").or_else(|e| { Err(anyhow!(e))})?;
//     let func: Py<PyAny> = module.getattr(symbol_name)?.into();
//     let result = func.call1(py, PyTuple::new_bound(py,[object]))?;
//     //log::error!("pyany_to_mat, resultis {result:?}/{:}", result.to_string());
//     pytuple_to_mat(result.extract::<&PyTuple>(py)?)
// }

pub fn pyany_to_value(value: &PyAny) -> PyResult<Value> {
    if value.is_instance_of::<PyBool>() {
        Ok(Value::from(value.extract::<bool>()?))
    } else if value.is_instance_of::<PyString>() {
        Ok(Value::from(value.extract::<String>()?))
    } else if value.is_instance_of::<PyFloat>() {
        Ok(Value::from(value.extract::<f64>()?))
    } else if value.is_instance_of::<PyInt>() {
        Ok(Value::from(value.extract::<i64>()?))
    } else if value.is_instance_of::<PyList>() {
        pylist_to_value(value.extract::<&PyList>()?)
    } else if value.is_instance_of::<PyTuple>() {
        pytuple_to_value(value.extract::<&PyTuple>()?)
    } else if value.is_instance_of::<PySet>() {
        pyset_to_value(value.extract::<&PySet>()?)
    } else if value.is_instance_of::<PyDict>() {
        pydict_to_value(value.extract::<&PyDict>()?)
    } else if value.is_instance_of::<PyNone>() {
        Ok(Value::Null)
    } else {
        let pytype = value.get_type();
        if pytype.to_string() == "PIL.Image.Iage" {
            todo!()
        } else {
            log::error!("Error for pyany_to_value. Error({pytype:?} is not available)");
            todo!("PythonのProcessの値としてjuizが対応していないタイプが渡されました。")
        }
    }
}


pub fn pyany_to_capsule(value: &PyAny) -> PyResult<Capsule> {
    if value.is_instance_of::<PyString>() {
        Ok(Value::from(value.extract::<String>()?).into())
    } else if value.is_instance_of::<PyFloat>() {
        Ok(Value::from(value.extract::<f64>()?).into())
    } else if value.is_instance_of::<PyInt>() {
        Ok(Value::from(value.extract::<i64>()?).into())
    } else if value.is_instance_of::<PyList>() {
        pylist_to_capsule(value.extract::<&PyList>()?)
    } else if value.is_instance_of::<PyTuple>() {
        pytuple_to_capsule(value.extract::<&PyTuple>()?)
    } else if value.is_instance_of::<PySet>() {
        pyset_to_capsule(value.extract::<&PySet>()?)
    } else if value.is_instance_of::<PyDict>() {
        pydict_to_capsule(value.extract::<&PyDict>()?)
    } else if value.is_instance_of::<PyNone>() {
        Ok(Value::Null.into())
    } else {
        let pytype = value.get_type();
        if pytype.to_string() == "<class 'PIL.Image.Image'>" {
            let image = Python::with_gil(|py| -> PyResult<DynamicImage> {
                let app_code = r"
import io

def convert_img(img):
    output = io.BytesIO()
    img.save(output, format='PNG')
    return output.getvalue() # Hex Data
";
                let module = PyModule::from_code_bound(py, &app_code.to_owned(), "", "")?;
                let byte_output = module.getattr("convert_img")?.into_py(py).call1(py, PyTuple::new_bound(py, vec![value]))?.extract::<&PyBytes>(py)?;
                match image::load_from_memory_with_format(byte_output.as_bytes(), ImageFormat::Png) {
                    Ok(i) => Ok(i),
                    Err(e) =>  {
                        log::error!("Image load from memmory error. Error: {e:?}");
                        panic!()
                    }
                }
            })?;

            
            // println!("pytype: {pytype:?}");
            //log::error!("Error for pyany_to_capsule. Error({pytype:?} is not available)");
            //todo!("PythonのProcessの値として画像型が渡されましたが、未対応です。juizが対応していないタイプが渡されました。")
            Ok(image.into())
        } else {
            // println!("pytype: {:?}", pytype.get_type());
            log::error!("Error for pyany_to_capsule. Error({pytype:?} is not available)");
            todo!("PythonのProcessの値としてjuizが対応していないタイプが渡されました。")
        }
    }
}


fn pylist_to_value(pylist: &PyList) -> PyResult<Value> {
    let mut vec: Vec<Value> = Vec::new();
    for value in pylist.into_iter() {
        vec.push(pyany_to_value(value)?);
    }
    Ok(vec.into())
}
fn pytuple_to_value(pytuple: &PyTuple) -> PyResult<Value> {
    let mut vec: Vec<Value> = Vec::new();
    for value in pytuple.into_iter() {
        vec.push(pyany_to_value(value)?);
    }
    Ok(vec.into())
}
fn pyset_to_value(pyset: &PySet) -> PyResult<Value> {
    let mut vec: Vec<Value> = Vec::new();
    for value in pyset.into_iter() {
        vec.push(pyany_to_value(value)?);
    }
    Ok(vec.into())
}

pub fn pydict_to_value(pydict: &PyDict) -> PyResult<Value> {
    let mut map: HashMap<String, Value> = HashMap::new();
    for (key, value) in pydict.into_iter() {
        map.insert(key.extract::<String>()?, pyany_to_value(value)?);
    }
    Ok(jvalue!(map))
}


fn pylist_to_capsule(pylist: &PyList) -> PyResult<Capsule> {
    let mut vec: Vec<Value> = Vec::new();
    for value in pylist.into_iter() {
        vec.push(pyany_to_value(value)?);
    }
    Ok(vec.into())
}
fn pytuple_to_capsule(pytuple: &PyTuple) -> PyResult<Capsule> {
    let mut vec: Vec<Value> = Vec::new();
    for value in pytuple.into_iter() {
        vec.push(pyany_to_value(value)?);
    }
    Ok(vec.into())
}
fn pyset_to_capsule(pyset: &PySet) -> PyResult<Capsule> {
    let mut vec: Vec<Value> = Vec::new();
    for value in pyset.into_iter() {
        vec.push(pyany_to_value(value)?);
    }
    Ok(vec.into())
}

pub fn pydict_to_capsule(pydict: &PyDict) -> PyResult<Capsule> {
    let mut map: HashMap<String, Value> = HashMap::new();
    for (key, value) in pydict.into_iter() {
        map.insert(key.extract::<String>()?, pyany_to_value(value)?);
    }
    Ok(jvalue!(map).into())
}
