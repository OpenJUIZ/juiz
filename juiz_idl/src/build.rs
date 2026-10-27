//! build.rs 用のヘルパ (tonic-build と同じ流れ)
//!
//! ```toml
//! [dependencies]
//! juiz_sdk = "…"
//!
//! [build-dependencies]
//! juiz_idl = "…"
//! ```
//!
//! ```no_run
//! // build.rs
//! fn main() {
//!     juiz_idl::build::compile(&["idl/geometry.jidl"]).unwrap();
//! }
//! ```
//!
//! ```ignore
//! // src/lib.rs。robot.geometry の型は robot::geometry::Pose2D になる
//! include!(concat!(env!("OUT_DIR"), "/juiz_idl.rs"));
//! ```

use std::path::{Path, PathBuf};

use crate::{generate, load_files, write_files, IdlError, OutputKind};

/// `configure().compile(files)` と同じ
pub fn compile<P: AsRef<Path>>(files: &[P]) -> Result<(), IdlError> {
    configure().compile(files)
}

pub fn configure() -> Builder {
    Builder::default()
}

/// 生成の設定
#[derive(Clone, Debug)]
pub struct Builder {
    out_dir: Option<PathBuf>,
    kinds: Vec<OutputKind>,
    emit_rerun_if_changed: bool,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            out_dir: None,
            kinds: vec![OutputKind::Rust],
            emit_rerun_if_changed: true,
        }
    }
}

impl Builder {
    /// 出力先。既定は環境変数 `OUT_DIR` (build.rs の中で cargo が設定する)
    pub fn out_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.out_dir = Some(dir.as_ref().to_path_buf());
        self
    }

    /// Rust のコードに加えて .avsc も書き出す
    pub fn avsc(mut self, enable: bool) -> Self {
        self.set_kind(OutputKind::Avsc, enable);
        self
    }

    /// Rust のコードに加えて JSON Schema も書き出す
    pub fn json_schema(mut self, enable: bool) -> Self {
        self.set_kind(OutputKind::JsonSchema, enable);
        self
    }

    /// Rust のコードに加えて `<namespace>.processes.json` も書き出す
    pub fn processes(mut self, enable: bool) -> Self {
        self.set_kind(OutputKind::Processes, enable);
        self
    }

    /// 読んだファイル (import した先も) の `cargo:rerun-if-changed` を出すか。既定は出す
    pub fn emit_rerun_if_changed(mut self, enable: bool) -> Self {
        self.emit_rerun_if_changed = enable;
        self
    }

    fn set_kind(&mut self, kind: OutputKind, enable: bool) {
        self.kinds.retain(|k| *k != kind);
        if enable {
            self.kinds.push(kind);
        }
    }

    pub fn compile<P: AsRef<Path>>(self, files: &[P]) -> Result<(), IdlError> {
        let out_dir = match self.out_dir {
            Some(d) => d,
            None => std::env::var_os("OUT_DIR")
                .map(PathBuf::from)
                .ok_or_else(|| IdlError::Generate {
                    message: "OUT_DIR is not set; call from build.rs or use out_dir()".to_owned(),
                })?,
        };
        let model = load_files(files)?;
        if self.emit_rerun_if_changed {
            for f in model.files.iter() {
                println!("cargo:rerun-if-changed={}", f.display());
            }
        }
        write_files(&generate(&model, &self.kinds)?, &out_dir)
    }
}
