use juiz_sdk::anyhow::{self, anyhow};
use libloading::{Library, Symbol};
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::Arc;
//use super::cpp_container_factory_impl::CppContainerFactoryImpl;
//use super::cpp_container_process_factory_impl::CppContainerProcessFactoryImpl;
//use crate::brokers::http::http_router::container;
use crate::containers::{
    bind_container_function, container_factory_create, container_process_factory_create_from_trait,
};
//use crate::plugin::cpp::cpp_container_factory_impl::CppContainerStruct;
use crate::prelude::*;
use crate::processes::process_factory_create_from_trait;

/// C ABI のプラグイン (C++ など) が公開する関数の型。
/// プラグイン側は extern "C" で定義するので、Rust ABI ではなく extern "C" の関数ポインタで受ける。
type ManifestEntryPoint = unsafe extern "C" fn(*mut CapsulePtr) -> i64;
type ProcessFunction = unsafe extern "C" fn(*mut CapsuleMap, *mut Capsule) -> i64;
type ContainerCreateFunction = unsafe extern "C" fn(*mut CapsuleMap, *mut *mut c_void) -> i64;
type ContainerDestroyFunction = unsafe extern "C" fn(*mut c_void);
type ContainerProcessFunction =
    unsafe extern "C" fn(*mut c_void, *mut CapsuleMap, *mut Capsule) -> i64;

/// C ABI で読み込むプラグインの言語。読み込み方は同じで、Haskell は RTS の初期化が加わる
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CAbiLanguage {
    Cpp,
    Haskell,
}

impl CAbiLanguage {
    fn as_str(&self) -> &'static str {
        match self {
            CAbiLanguage::Cpp => "c++",
            CAbiLanguage::Haskell => "haskell",
        }
    }

    fn build_hint(&self) -> &'static str {
        match self {
            CAbiLanguage::Cpp => "Build the C++ plugin first with cmake (juiz's own examples: \
                       `cmake -S examples/cpp -B examples/cpp/build && cmake --build examples/cpp/build` \
                       after `cargo build`). For juiz's own tests, `scripts/test.sh` builds all the examples.",
            CAbiLanguage::Haskell => "Build the Haskell plugin first with cabal (juiz's own examples: \
                       `examples/haskell/build.sh` after `cargo build`). \
                       For juiz's own tests, `scripts/test.sh` builds all the examples.",
        }
    }
}

/// Haskell のプラグインが公開する、GHC の RTS を初期化する関数 (int64_t juiz_hs_init(void))
const HASKELL_INIT_SYMBOL: &str = "juiz_hs_init";

pub struct CppPlugin {
    path: PathBuf,
    language: CAbiLanguage,
    /// 関数ポインタを使う間はライブラリを閉じないよう、作った Process や Container にも持たせる
    lib: Arc<Library>,
    manifest: Value,
}

impl std::fmt::Debug for CppPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CppPlugin")
            .field("path", &self.path)
            .field("language", &self.language)
            .field("lib", &self.lib)
            .field("manifest", &self.manifest)
            .finish()
    }
}

pub struct CppContainerStruct {
    pub cobj: *mut std::ffi::c_void,
    /// プラグインが <シンボル名>_destroy_entry_point を公開していれば、Drop で呼んで cobj を解放する
    destroy: Option<ContainerDestroyFunction>,
    _lib: Arc<Library>,
}

impl Drop for CppContainerStruct {
    fn drop(&mut self) {
        if let Some(destroy) = self.destroy {
            if !self.cobj.is_null() {
                log::trace!("CppContainerStruct::drop() calls destroy function");
                unsafe { destroy(self.cobj) };
                self.cobj = std::ptr::null_mut();
            }
        }
    }
}

/*
pub type Symbol<'lib, T> = libloading::Symbol<'lib, libloading::Symbol<'lib, T>>;
*/
impl CppPlugin {
    /// CppPluginのコンストラクタ
    /// path:
    /// manifest_entry_point: ファイル自体のマニフェストのエントリーポイント
    pub fn new(path: PathBuf, manifest_entry_point: &str) -> JuizResult<CppPlugin> {
        Self::open(path, manifest_entry_point, CAbiLanguage::Cpp)
    }

    /// Haskell (GHC) のプラグインを開く。
    /// 開いた直後に juiz_hs_init を呼んで RTS を初期化する (無ければエラー)。
    /// GHC は hs_exit の後に hs_init し直せないので juiz_hs_exit は呼ばず、
    /// RTS のスレッドが動いているコードを外さないよう、ライブラリも閉じない。
    pub fn new_haskell(path: PathBuf, manifest_entry_point: &str) -> JuizResult<CppPlugin> {
        Self::open(path, manifest_entry_point, CAbiLanguage::Haskell)
    }

    fn open(
        path: PathBuf,
        manifest_entry_point: &str,
        language: CAbiLanguage,
    ) -> JuizResult<CppPlugin> {
        log::trace!(
            "【呼出】CppPlugin::open(path={:?}, manifest_entry_point={:?}, language={:?})",
            path,
            manifest_entry_point,
            language
        );
        if !path.exists() {
            log::error!("【エラー】CppPlugin::new({path:?}) failed. ファイルが見つかりません");
            return Err(anyhow::Error::from(JuizError::PluginFileNotFoundError {
                plugin_path: path.display().to_string(),
                hint: language.build_hint().to_owned(),
            }));
        }
        let entry_point = manifest_entry_point.to_owned() + "_entry_point";
        unsafe {
            let lib = Arc::new(Library::new(path.clone()).or_else(|e| {
                log::error!("【エラー】Library::new({path:?}) failed. Error ({e:?})");
                Err(e)
            })?);
            if language == CAbiLanguage::Haskell {
                init_haskell_runtime(&path, &lib)?;
            }
            let mut manif_cap = CapsulePtr::new();
            let manifest_function: Symbol<ManifestEntryPoint> =
                lib.get(entry_point.as_bytes()).or_else(|e| {
                    log::error!("【エラー】Library::new({path:?}) failed. Error ({e:?})");
                    Err(e)
                })?;
            if manifest_function(&mut manif_cap) != 0 {
                log::error!("プロセスのマニフェスト取得に失敗");
                return Err(anyhow::Error::from(
                    JuizError::CppProcessFunctionCallError {},
                ));
            }
            let manifest_value = manif_cap.extract_value()?;
            log::debug!("プロセスのマニフェスト取得に成功。マニフェスト：{manifest_value:}");
            Ok(CppPlugin {
                path,
                language,
                lib,
                manifest: manifest_value,
            })
        }
    }

    /// CppPluginのプロファイル
    pub fn profile_full(&self) -> JuizResult<Value> {
        Ok(jvalue!({
            "path": self.path,
            "language": self.language.as_str(),
        }))
    }

    pub fn get_manifest(&self) -> &Value {
        log::trace!("【呼出】get_manifest() -> {:?}", self.manifest);
        &self.manifest
    }

    pub fn load_component_manifest(
        &self,
        _working_dir: Option<PathBuf>,
    ) -> JuizResult<ComponentManifest> {
        serde_json::from_value(self.get_manifest().clone()).or_else(|e| Err(anyhow!(e)))
    }

    pub fn load_process_factory(
        &self,
        _working_dir: Option<PathBuf>,
        symbol_name: &str,
        type_name_opt: Option<&str>,
    ) -> JuizResult<ProcessFactoryPtr> {
        log::trace!("【呼出】load_process_factory({symbol_name:})");
        let full_symbol_name = symbol_name.to_owned() + "_entry_point";
        type SymbolType = unsafe extern "C" fn() -> ProcessFunction;
        let f = unsafe {
            let symbol = self.load_symbol::<SymbolType>(full_symbol_name.as_bytes())?;
            (symbol)()
        };
        let manifest = match type_name_opt {
            Some(type_name) => {
                log::debug!("type_name_optが検出されました({type_name})。コンポーネントのマニフェストを取得します。");
                let manif: ComponentManifest = serde_json::from_value(self.get_manifest().clone())?;
                manif
                    .processes
                    .iter()
                    .find(|p| p.type_name == type_name)
                    .ok_or(anyhow!(JuizError::ArgumentError {
                        message: format!(
                            "ComponentManifest does not include process(type_name={type_name})"
                        )
                    }))?
                    .clone()
            }
            None => {
                log::debug!("type_name_optが検出されませんでした。バイナリから取得したマニフェストを使用します。");
                match serde_json::from_value(self.get_manifest().clone()) {
                    Ok(proc_manif) => {
                        log::debug!("バイナリから取得したマニフェストからProcessManifestへの変換に成功しました ({proc_manif}) ");
                        Ok(proc_manif)
                    }
                    Err(e) => {
                        log::error!("バイナリから取得したマニフェストからProcessManifestへの変換に失敗しました。エラー ({e})");
                        Err(e)
                    }
                }?
            }
        };
        create_cpp_process_factory(manifest, f, self.lib.clone())
    }

    pub fn load_container_factory(
        &self,
        _working_dir: Option<PathBuf>,
        symbol_name: &str,
        type_name_opt: Option<&str>,
    ) -> JuizResult<ContainerFactoryPtr> {
        log::trace!(
            "CppPlugin({:?})::load_container_factory({symbol_name}, {type_name_opt:?}) called",
            self.path
        );
        let full_symbol_name = symbol_name.to_owned() + "_entry_point";
        type SymbolType = unsafe extern "C" fn() -> ContainerCreateFunction;
        let entry_point = unsafe {
            let symbol = self.load_symbol::<SymbolType>(full_symbol_name.as_bytes())?;
            (symbol)()
        };
        // 解放関数は任意。古いプラグインには無いので、無ければ今まで通り何もしない
        let destroy_symbol_name = symbol_name.to_owned() + "_destroy_entry_point";
        let destroy: Option<ContainerDestroyFunction> = unsafe {
            self.lib
                .get::<ContainerDestroyFunction>(destroy_symbol_name.as_bytes())
                .ok()
                .map(|f| *f)
        };
        if destroy.is_none() {
            log::debug!(
                "CppPlugin({:?}) does not export {destroy_symbol_name}. Containers will not be freed.",
                self.path
            );
        }
        let lib = self.lib.clone();

        let container_manifest = match type_name_opt {
            Some(type_name) => {
                let manif: ComponentManifest = serde_json::from_value(self.get_manifest().clone())?;
                manif
                    .containers
                    .iter()
                    .find(|p| p.type_name == type_name)
                    .ok_or(anyhow!(JuizError::ArgumentError {
                        message: format!(
                            "ComponentManifest does not include container(type_name={type_name})"
                        )
                    }))?
                    .clone()
            }
            None => serde_json::from_value(self.get_manifest().clone())?,
        };
        let constructor =
            move |cm: ContainerManifest, mut v: CapsuleMap| -> JuizResult<ContainerPtr> {
                let mut pobj: *mut c_void = std::ptr::null_mut();
                let retval = unsafe { (entry_point)(&mut v, &mut pobj) };
                if retval < 0 || pobj == std::ptr::null_mut() {
                    return Err(anyhow::Error::from(JuizError::CppPluginFunctionCallError {
                        function_name: "create_container".to_owned(),
                        return_value: retval,
                    }));
                }
                Ok(ContainerPtr::new(ContainerImpl::new(
                    cm,
                    Box::new(CppContainerStruct {
                        cobj: pobj,
                        destroy,
                        _lib: lib.clone(),
                    }),
                )?))
            };
        container_factory_create(container_manifest, Arc::new(constructor))
    }

    pub fn load_container_process_factory(
        &self,
        _working_dir: Option<PathBuf>,
        symbol_name: &str,
        type_name_opt: Option<&str>,
    ) -> JuizResult<ContainerProcessFactoryPtr> {
        log::trace!("CppPlugin({:?})::load_container_process_factory({symbol_name}, {type_name_opt:?}) called", self.path);
        let full_symbol_name = symbol_name.to_owned() + "_entry_point";
        type SymbolType = unsafe extern "C" fn() -> ContainerProcessFunction;
        let entry_point = unsafe {
            let symbol = self.load_symbol::<SymbolType>(full_symbol_name.as_bytes())?;
            (symbol)()
        };
        let container_process_manifest: ProcessManifest = match type_name_opt {
            Some(type_name) => find_container_process_from_component_manifest(
                serde_json::from_value(self.get_manifest().clone())?,
                type_name,
            )?,
            None => serde_json::from_value(self.get_manifest().clone())?,
        };
        let type_name = container_process_manifest.type_name.to_owned();
        let lib = self.lib.clone();
        let constructor = move |c: &mut ContainerImpl<CppContainerStruct>,
                                mut argument: CapsuleMap|
              -> JuizResult<Capsule> {
            let _lib = &lib;
            let mut retval = Capsule::empty();
            let return_value = unsafe { (entry_point)(c.t.cobj, &mut argument, &mut retval) };
            if return_value != 0 {
                return Err(anyhow::Error::from(JuizError::CppPluginFunctionCallError {
                    function_name: format!("container_process({type_name})"),
                    return_value,
                }));
            }
            Ok(retval)
        };
        container_process_factory_create_from_trait(
            container_process_manifest.into(),
            bind_container_function(constructor),
        )
        //Ok(ContainerProcessFactoryPtr::new(CppContainerProcessFactoryImpl::new2(self.get_manifest().clone().try_into()?, f)?))
    }

    pub fn load_symbol<T>(&self, symbol_name: &[u8]) -> JuizResult<libloading::Symbol<'_, T>> {
        log::trace!(
            "CppPlugin::load_symbol({:?}) called",
            std::str::from_utf8(symbol_name)
        );
        unsafe {
            match self.lib.get::<T>(symbol_name) {
                Ok(func) => Ok(func),
                Err(e) => {
                    log::error!(
                        "CppPlugin::load_symbol({:?}) failed. Err({e:})",
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
}

/// Haskell のプラグインの juiz_hs_init を呼び、ライブラリを閉じないようにする
unsafe fn init_haskell_runtime(path: &PathBuf, lib: &Arc<Library>) -> JuizResult<()> {
    type InitFunction = unsafe extern "C" fn() -> i64;
    // hs_init は複数のスレッドから同時に呼ぶと初期化が終わる前に戻ることがあり、
    // 別々の Haskell ライブラリでも RTS は共有されるので、juiz 全体で一つずつ呼ぶ
    static INIT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = INIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let init: Symbol<InitFunction> = lib.get(HASKELL_INIT_SYMBOL.as_bytes()).or_else(|e| {
        log::error!("【エラー】Haskell plugin ({path:?}) does not export {HASKELL_INIT_SYMBOL}. Error ({e:?})");
        Err(anyhow::Error::from(
            JuizError::PluginLoadSymbolFailedError {
                plugin_path: path.display().to_string(),
                symbol_name: HASKELL_INIT_SYMBOL.to_owned(),
            },
        ))
    })?;
    let retval = init();
    if retval != 0 {
        return Err(anyhow::Error::from(JuizError::CppPluginFunctionCallError {
            function_name: HASKELL_INIT_SYMBOL.to_owned(),
            return_value: retval,
        }));
    }
    // dlclose で RTS ごとコードを外すとプロセスが落ちるので、参照を一つ残して閉じないようにする
    std::mem::forget(lib.clone());
    Ok(())
}

fn find_container_process_from_component_manifest(
    comp_manif: ComponentManifest,
    type_name: &str,
) -> JuizResult<ProcessManifest> {
    for c in comp_manif.containers.iter() {
        for p in c.processes.iter() {
            if p.type_name == type_name {
                return Ok(p.clone());
            }
        }
    }
    Err(anyhow!(JuizError::ArgumentError {
        message: format!("ComponentManifest does not include container(type_name={type_name})")
    }))
}

fn create_cpp_process_factory(
    manifest: ProcessManifest,
    entry_point: ProcessFunction,
    lib: Arc<Library>,
) -> JuizResult<ProcessFactoryPtr> {
    log::trace!("【呼出】create_cpp_process_factory({manifest}, {entry_point:?}");
    let entry_point_name = "process_entry_point".to_owned();
    let function = move |mut argument: CapsuleMap| -> JuizResult<Capsule> {
        log::trace!("【呼出】func_cpp(argument={argument:?})");
        let _lib = &lib;
        let mut func_result: Capsule = Capsule::empty();
        unsafe {
            let v = entry_point(&mut argument, &mut func_result);
            if v < 0 {
                return Err(anyhow::Error::from(JuizError::CppPluginFunctionCallError {
                    function_name: entry_point_name.clone(),
                    return_value: v,
                }));
            }
        }
        return Ok(func_result);
    };

    process_factory_create_from_trait(manifest, function)
}
