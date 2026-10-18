use juiz_sdk::anyhow::{self, Context};
use juiz_sdk::build_info::{BuildInfo, BUILD_INFO_SYMBOL_SUFFIX};
use libloading::Library;
use std::{
    ffi::CStr,
    os::raw::c_char,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use crate::plugin::Plugin;
use crate::{
    containers::{container_factory_create, container_process_factory_create},
    prelude::*,
};
// use super::plugin::Plugin;

pub trait PluginManager {}

#[allow(unused)]
pub struct RustPlugin {
    path: PathBuf,
    plugin_manager: Option<Arc<Mutex<dyn PluginManager>>>,
    lib: Option<Library>,
}

impl std::fmt::Debug for RustPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RustPlugin")
            .field("path", &self.path)
            .field("plugin_manager", &"{}".to_owned())
            .field("lib", &self.lib)
            .finish()
    }
}

impl RustPlugin {
    pub fn load(path: PathBuf) -> JuizResult<RustPlugin> {
        log::trace!("【呼出】RustPlugin::load({:?})", path);
        if !path.exists() {
            log::error!(
                "【失敗】RustPlugin::load({:?}). ファイルが見つかりません",
                path
            );
            return Err(anyhow::Error::from(JuizError::PluginFileNotFoundError {
                plugin_path: path.display().to_string(),
                hint: "Build the Rust plugin first with `cargo build`. \
                       For juiz's own tests, `scripts/test.sh` builds all the examples."
                    .to_owned(),
            }));
        }
        unsafe {
            match libloading::Library::new(path.clone()) {
                Ok(lib) => {
                    type FunctionType = libloading::Symbol<
                        'static,
                        unsafe extern "Rust" fn() -> Arc<Mutex<dyn PluginManager>>,
                    >;
                    let symbol_name = "plugin_manager";
                    let plugin_manager = match lib.get::<FunctionType>(symbol_name.as_bytes()) {
                        Ok(func) => {
                            log::info!("PluginManagerをロード完了");
                            Some(func())
                        }
                        Err(_) => None,
                    };
                    log::info!("RustPlugin({:?})をロード完了", path);
                    Ok(RustPlugin {
                        lib: Some(lib),
                        plugin_manager,
                        path,
                    })
                }
                Err(e) => {
                    log::error!("【失敗】RustPlugin::load({:?}). エラー({e:})", path);
                    Err(anyhow::Error::from(JuizError::PluginLoadFailedError {
                        plugin_path: path.display().to_string(),
                    }))
                }
            }
        }
    }

    pub fn load_symbol<T>(&self, symbol_name: &[u8]) -> JuizResult<libloading::Symbol<T>> {
        log::trace!(
            "【呼出】RustPlugin::load_symbol({:?})",
            std::str::from_utf8(symbol_name)
        );
        unsafe {
            match self.lib.as_ref().unwrap().get::<T>(symbol_name) {
                Ok(func) => Ok(func),
                Err(_) => {
                    log::error!(
                        "RustPlugin::load_symbol({:?}) failed.",
                        std::str::from_utf8(symbol_name)
                    );
                    Err(anyhow::Error::from(
                        JuizError::PluginLoadSymbolFailedError {
                            plugin_path: self.path.display().to_string(),
                            symbol_name: std::str::from_utf8(symbol_name)?.to_string(),
                        },
                    ))
                }
            }
        }
    }

    /// ファクトリ `symbol_name` を呼ぶ前に、プラグインのビルド情報をホストと比べる。
    ///
    /// juiz_macro が生成した `<symbol_name>_build_info` を呼び、コンパイラか juiz_sdk の版が
    /// 食い違えば、ファクトリを呼ばずにエラーにする(Rust の ABI がずれたまま呼ぶと異常終了しうる)。
    /// 関数が無い古いプラグインは警告を出して読み込む。
    pub fn check_build_info(&self, symbol_name: &str) -> JuizResult<()> {
        let build_info_symbol = format!("{symbol_name}{BUILD_INFO_SYMBOL_SUFFIX}");
        type BuildInfoSymbolType<'a> =
            libloading::Symbol<'a, unsafe extern "C" fn() -> *const c_char>;
        let func = match unsafe {
            self.lib
                .as_ref()
                .unwrap()
                .get::<BuildInfoSymbolType>(build_info_symbol.as_bytes())
        } {
            Ok(func) => func,
            Err(_) => {
                log::warn!(
                    "RustPlugin({}) has no '{build_info_symbol}', so the compiler and juiz_sdk versions can not be checked. \
                     Rebuild the plugin with the same juiz_sdk as this juiz ({}) to enable the check.",
                    self.path.display(),
                    BuildInfo::current().juiz_sdk_str()
                );
                return Ok(());
            }
        };
        let ptr = unsafe { func() };
        if ptr.is_null() {
            log::warn!(
                "RustPlugin({})::{build_info_symbol}() returned null.",
                self.path.display()
            );
            return Ok(());
        }
        let plugin_info = BuildInfo::parse(&unsafe { CStr::from_ptr(ptr) }.to_string_lossy());
        verify_build_info(
            &self.path.display().to_string(),
            &plugin_info,
            &BuildInfo::current(),
        )
    }

    pub fn load_process_factory(
        &self,
        _working_dir: Option<PathBuf>,
        symbol_name: &str,
    ) -> JuizResult<ProcessFactoryPtr> {
        log::trace!(
            "【呼出】load_process_factory({_working_dir:?}, symbol_name={symbol_name}) called"
        );
        type SymbolType = libloading::Symbol<
            'static,
            unsafe extern "Rust" fn() -> JuizResult<ProcessFactoryStruct>,
        >;
        self.check_build_info(symbol_name)?;
        unsafe {
            let symbol = self.load_symbol::<SymbolType>(symbol_name.as_bytes())?;
            let ProcessFactoryStruct(manifest, proc_function) =
                (symbol)().with_context(|| format!("calling symbol '{symbol_name}'"))?;
            process_factory_create(manifest, proc_function)
        }
    }

    pub fn load_container_factory(
        &self,
        _working_dir: Option<PathBuf>,
        symbol_name: &str,
    ) -> JuizResult<ContainerFactoryPtr> {
        log::trace!(
            "【呼出】load_container_factory({_working_dir:?}, symbol_name={symbol_name}) called"
        );
        type SymbolType = libloading::Symbol<
            'static,
            unsafe extern "Rust" fn() -> JuizResult<ContainerFactoryStruct>,
        >;
        self.check_build_info(symbol_name)?;
        unsafe {
            let symbol = self.load_symbol::<SymbolType>(symbol_name.as_bytes())?;
            let ContainerFactoryStruct(manifest, factory_function) = (symbol)()?;
            container_factory_create(manifest, factory_function)
        }
    }

    pub fn load_container_process_factory(
        &self,
        _working_dir: Option<PathBuf>,
        symbol_name: &str,
    ) -> JuizResult<ContainerProcessFactoryPtr> {
        log::trace!("【呼出】load_container_process_factory({_working_dir:?}, symbol_name={symbol_name}) called");
        type SymbolType = libloading::Symbol<
            'static,
            unsafe extern "Rust" fn() -> JuizResult<ContainerProcessFactoryStruct>,
        >;
        self.check_build_info(symbol_name)?;
        unsafe {
            let symbol = self.load_symbol::<SymbolType>(symbol_name.as_bytes())?;
            let ContainerProcessFactoryStruct(manifest, factory_function) = (symbol)()?;
            container_process_factory_create(manifest, factory_function)
        }
    }

    pub fn load_component_manifest(&self) -> JuizResult<ComponentManifest> {
        log::trace!(
            "【呼出】load_component_manifest() for RustPlugin(path={:?}) called",
            self.path
        );
        type ComponentProfileFunctionSymbolType<'a> =
            libloading::Symbol<'a, unsafe extern "Rust" fn() -> ComponentManifest>;
        self.check_build_info("component_manifest")?;
        let symbol =
            self.load_symbol::<ComponentProfileFunctionSymbolType>(b"component_manifest")?;
        Ok(unsafe {
            (symbol)() //.with_context(||format!("calling symbol 'container_factory'. arg is {manifest:}"))?;
        })
    }

    pub fn load_broker_factory(
        &self,
        system: &mut System,
    ) -> JuizResult<Arc<Mutex<dyn BrokerFactory>>> {
        log::trace!(
            "【呼出】load_broker_factory() for RustPlugin(path={:?}) called",
            self.path
        );
        //log::trace!("BrokerFactory (type_name={:?}) created.", juiz_lock(&bf)?.type_name());
        type BrokerFactorySymbolType<'a> = libloading::Symbol<
            'a,
            unsafe extern "Rust" fn(CoreBrokerPtr) -> JuizResult<Arc<Mutex<dyn BrokerFactory>>>,
        >;
        let symbol_bf = self.load_symbol::<BrokerFactorySymbolType>(b"broker_factory")?;
        unsafe { (symbol_bf)(system.core_broker().clone()) }
    }

    pub fn load_broker_proxy_factory(
        &self,
        _system: &mut System,
    ) -> JuizResult<Arc<Mutex<dyn BrokerProxyFactory>>> {
        log::trace!(
            "【呼出】load_broker_proxy_factory() for RustPlugin(path={:?}) called",
            self.path
        );
        type BrokerProxyFactorySymbolType<'a> = libloading::Symbol<
            'a,
            unsafe extern "Rust" fn() -> JuizResult<Arc<Mutex<dyn BrokerProxyFactory>>>,
        >;
        let symbol_bpf =
            self.load_symbol::<BrokerProxyFactorySymbolType>(b"broker_proxy_factory")?;
        unsafe { (symbol_bpf)() }
    }
}

/// プラグインのビルド情報がホストと同じか比べ、食い違えば PluginBuildInfoMismatchError を返す。
fn verify_build_info(
    plugin_path: &str,
    plugin_info: &BuildInfo,
    host_info: &BuildInfo,
) -> JuizResult<()> {
    if plugin_info == host_info {
        return Ok(());
    }
    log::error!(
        "【失敗】RustPlugin({plugin_path}) のビルド情報がホストと食い違います (plugin: {plugin_info}, juiz: {host_info})"
    );
    Err(anyhow::Error::from(
        JuizError::PluginBuildInfoMismatchError {
            plugin_path: plugin_path.to_owned(),
            plugin_build_info: plugin_info.to_string(),
            host_build_info: host_info.to_string(),
            host_juiz_sdk: host_info.juiz_sdk_str().to_owned(),
        },
    ))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_build_info_is_accepted() {
        let host = BuildInfo::current();
        assert!(verify_build_info("libfoo.so", &host.clone(), &host).is_ok());
    }

    #[test]
    fn different_build_info_is_rejected() {
        let host = BuildInfo::parse("juiz_sdk=0.0.15;rustc=rustc 1.97.0 (2d8144b78 2026-07-07)");
        for plugin_str in [
            "juiz_sdk=0.0.14;rustc=rustc 1.97.0 (2d8144b78 2026-07-07)",
            "juiz_sdk=0.0.15;rustc=rustc 1.96.0 (aaaaaaaaa 2026-05-28)",
            "juiz_sdk=0.0.15",
        ] {
            let plugin = BuildInfo::parse(plugin_str);
            let err = verify_build_info("libfoo.so", &plugin, &host).unwrap_err();
            match err.downcast_ref::<JuizError>() {
                Some(JuizError::PluginBuildInfoMismatchError {
                    plugin_path,
                    host_juiz_sdk,
                    ..
                }) => {
                    assert_eq!(plugin_path, "libfoo.so");
                    assert_eq!(host_juiz_sdk, "0.0.15");
                }
                other => panic!("unexpected error {other:?}"),
            }
            let message = err.to_string();
            assert!(message.contains("rustc 1.97.0"), "{message}");
            assert!(message.contains("juiz_sdk = \"=0.0.15\""), "{message}");
        }
    }
}
