//! feature "python" を無効にしてビルドしたときの PythonPlugin。
//! 読み込もうとすると、Python に対応していないことを伝えるエラーを返す。

use crate::{
    containers::{ContainerFactoryPtr, ContainerProcessFactoryPtr},
    prelude::*,
};
use juiz_sdk::anyhow::{self, anyhow};
use std::path::PathBuf;

#[derive(Debug)]
pub struct PythonPlugin {
    path: PathBuf,
}

fn python_disabled_error(path: &PathBuf) -> anyhow::Error {
    anyhow!(
        "Python のプラグイン({path:?})を読み込めません。この juiz は Python に対応せずにビルドされています。\
         Python を使うときは feature \"embed-python\" を有効にしてビルドし直してください \
         (例: cargo install juiz_app)。"
    )
}

impl PythonPlugin {
    pub fn profile_full(&self) -> JuizResult<Value> {
        Ok(jvalue!({
            "path": self.path,
        }))
    }

    pub fn load(path: PathBuf, _pythonpaths: Option<Vec<PathBuf>>) -> JuizResult<PythonPlugin> {
        Err(python_disabled_error(&path))
    }

    pub fn load_process_factory(
        &self,
        _working_dir: Option<PathBuf>,
        _symbol_name: &str,
        _type_name_opt: Option<&str>,
    ) -> JuizResult<ProcessFactoryPtr> {
        Err(python_disabled_error(&self.path))
    }

    pub fn load_container_process_factory(
        &self,
        _working_dir: Option<PathBuf>,
        _symbol_name: &str,
        _type_name_opt: Option<&str>,
    ) -> JuizResult<ContainerProcessFactoryPtr> {
        Err(python_disabled_error(&self.path))
    }

    pub fn load_container_factory(
        &self,
        _working_dir: Option<PathBuf>,
        _symbol_name: &str,
        _type_name_opt: Option<&str>,
    ) -> JuizResult<ContainerFactoryPtr> {
        Err(python_disabled_error(&self.path))
    }

    pub fn load_component_manifest(
        &self,
        _working_dir: Option<PathBuf>,
    ) -> JuizResult<ComponentManifest> {
        Err(python_disabled_error(&self.path))
    }
}
