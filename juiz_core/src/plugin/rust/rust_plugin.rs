
use juiz_sdk::anyhow::{self, Context};
use libloading::Library;
use std::{path::PathBuf, sync::{Arc, Mutex}};

use crate::{containers::{container_factory_create, container_process_factory_create}, prelude::*};
use crate::plugin::Plugin;
// use super::plugin::Plugin;

pub trait PluginManager {

}

#[allow(unused)]
pub struct RustPlugin {
    path: PathBuf,
    plugin_manager: Option<Arc<Mutex<dyn PluginManager>>>,
    lib: Option<Library>,
}

impl std::fmt::Debug for RustPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RustPlugin").field("path", &self.path).field("plugin_manager", &"{}".to_owned()).field("lib", &self.lib).finish()
    }
}


impl RustPlugin {

    pub fn load(path: PathBuf) -> JuizResult<RustPlugin> {
        log::trace!("【呼出】RustPlugin::load({:?})", path);
        unsafe {
            match libloading::Library::new(path.clone()) {
                Ok(lib) => {
                    type FunctionType = libloading::Symbol<'static, unsafe extern "Rust" fn() -> Arc<Mutex<dyn PluginManager>> >;
                    let symbol_name = "plugin_manager";
                    let plugin_manager = match lib.get::<FunctionType>(symbol_name.as_bytes()) {
                        Ok(func) => {
                            log::info!("PluginManagerをロード完了");
                            Some(func())
                        },
                        Err(_) => None
                    };
                    log::info!("RustPlugin({:?})をロード完了", path);
                    Ok(RustPlugin{lib:Some(lib), 
                        plugin_manager,
                        path})
                },
                Err(e) => {
                    log::error!("【失敗】RustPlugin::load({:?}). エラー({e:})", path);
                    Err(anyhow::Error::from(JuizError::PluginLoadFailedError{plugin_path: path.display().to_string()}))
                }
            }
        }
    }

    pub fn load_symbol<T>(&self, symbol_name: &[u8]) -> JuizResult<libloading::Symbol<T>> {
        log::trace!("【呼出】RustPlugin::load_symbol({:?})", std::str::from_utf8(symbol_name));
        unsafe {
            match self.lib.as_ref().unwrap().get::<T>(symbol_name) {
                Ok(func) => Ok(func),
                Err(_) => {
                    log::error!("RustPlugin::load_symbol({:?}) failed.", std::str::from_utf8(symbol_name));
                    Err(anyhow::Error::from(JuizError::PluginLoadSymbolFailedError{plugin_path:self.path.display().to_string(), symbol_name: std::str::from_utf8(symbol_name)?.to_string()}))
                }
            }
        }
    }

    pub fn load_process_factory(&self, _working_dir: Option<PathBuf>, symbol_name: &str) -> JuizResult<ProcessFactoryPtr> {
        log::trace!("【呼出】load_process_factory({_working_dir:?}, symbol_name={symbol_name}) called");
        type SymbolType = libloading::Symbol<'static, unsafe extern "Rust" fn() -> JuizResult<ProcessFactoryStruct>>;
        unsafe {
            let symbol = self.load_symbol::<SymbolType>(symbol_name.as_bytes())?;
            let ProcessFactoryStruct(manifest, proc_function) = (symbol)().with_context(||format!("calling symbol '{symbol_name}'"))?;
            process_factory_create(manifest, proc_function)
        }
    }

    pub fn load_container_factory(&self, _working_dir: Option<PathBuf>, symbol_name: &str) -> JuizResult<ContainerFactoryPtr> {
        log::trace!("【呼出】load_container_factory({_working_dir:?}, symbol_name={symbol_name}) called");
        type SymbolType = libloading::Symbol<'static, unsafe extern "Rust" fn() -> JuizResult<ContainerFactoryStruct>>;
        unsafe {
            let symbol = self.load_symbol::<SymbolType>(symbol_name.as_bytes())?;
            let ContainerFactoryStruct(manifest, factory_function) = (symbol)()?;
            container_factory_create(manifest, factory_function)
        }
    }

    pub fn load_container_process_factory(&self, _working_dir: Option<PathBuf>, symbol_name: &str) -> JuizResult<ContainerProcessFactoryPtr> {
        log::trace!("【呼出】load_container_process_factory({_working_dir:?}, symbol_name={symbol_name}) called");
        type SymbolType = libloading::Symbol<'static, unsafe extern "Rust" fn() -> JuizResult<ContainerProcessFactoryStruct>>;
        unsafe {
            let symbol = self.load_symbol::<SymbolType>(symbol_name.as_bytes())?;
            let ContainerProcessFactoryStruct(manifest, factory_function) = (symbol)()?;
            container_process_factory_create(manifest, factory_function)
        }
    }

    pub fn load_component_manifest(&self) -> JuizResult<ComponentManifest> {
        log::trace!("【呼出】load_component_manifest() for RustPlugin(path={:?}) called", self.path);
        type ComponentProfileFunctionSymbolType<'a> = libloading::Symbol<'a, unsafe extern "Rust" fn() -> ComponentManifest>;
        let symbol = self.load_symbol::<ComponentProfileFunctionSymbolType>(b"component_manifest")?;
        Ok(unsafe {
             (symbol)()//.with_context(||format!("calling symbol 'container_factory'. arg is {manifest:}"))?;
        })
    }

    pub fn load_broker_factory(&self, system: &mut System, ) -> JuizResult<Arc<Mutex<dyn BrokerFactory>>> {
        log::trace!("【呼出】load_broker_factory() for RustPlugin(path={:?}) called", self.path);
        //log::trace!("BrokerFactory (type_name={:?}) created.", juiz_lock(&bf)?.type_name());
        type BrokerFactorySymbolType<'a> = libloading::Symbol<'a, unsafe extern "Rust" fn(CoreBrokerPtr) -> JuizResult<Arc<Mutex<dyn BrokerFactory>>>>;
        let symbol_bf = self.load_symbol::<BrokerFactorySymbolType>(b"broker_factory")?;
        unsafe {
            (symbol_bf)(system.core_broker().clone())
        }
    }

    pub fn load_broker_proxy_factory(&self, _system: &mut System,) -> JuizResult<Arc<Mutex<dyn BrokerProxyFactory>>> {
        log::trace!("【呼出】load_broker_proxy_factory() for RustPlugin(path={:?}) called", self.path);
        type BrokerProxyFactorySymbolType<'a> = libloading::Symbol<'a, unsafe extern "Rust" fn() -> JuizResult<Arc<Mutex<dyn BrokerProxyFactory>>>>;
        let symbol_bpf = self.load_symbol::<BrokerProxyFactorySymbolType>(b"broker_proxy_factory")?;
        unsafe {
            (symbol_bpf)()
        }
    }
    
}

impl Plugin for RustPlugin {
    fn profile_full(&self) -> JuizResult<Value> {
        Ok(jvalue!({
            "path": self.path,
        }))
    }
}

impl Drop for RustPlugin {
    fn drop(&mut self) {
        log::info!("RustPlugin({})::drop() called", self.path.display());
    }
}