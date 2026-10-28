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

use crate::{
    generate_with, load_files_with, write_files, ExternNamespace, GenerateOptions, IdlError,
    LoadOptions, OutputKind, IMPORT_PATH_ENV,
};

/// 型のクレートが自分の .jidl のディレクトリを、依存するクレートの build.rs に知らせる (仕様 §24.3)。
/// Cargo.toml に `links = "<名前>"` を書いたクレートの build.rs で呼ぶと、依存するクレートの build.rs で
/// 環境変数 `DEP_<名前>_IDL_DIR` (名前は大文字、`-` は `_`) にこのディレクトリの絶対パスが入る
pub fn export_idl_dir(dir: impl AsRef<Path>) -> Result<(), IdlError> {
    let dir = dir.as_ref();
    let abs = dir.canonicalize().map_err(|e| IdlError::Io {
        path: dir.display().to_string(),
        message: e.to_string(),
    })?;
    println!("cargo:idl_dir={}", abs.display());
    Ok(())
}

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
    import_dirs: Vec<PathBuf>,
    externs: Vec<ExternNamespace>,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            out_dir: None,
            kinds: vec![OutputKind::Rust],
            emit_rerun_if_changed: true,
            import_dirs: Vec::new(),
            externs: Vec::new(),
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

    /// `process` / `container` 宣言の Rust の雛形 `<namespace>.skeleton.rs` も書き出す。
    /// 雛形はふつうはコマンド (`juiz-idl -g rust-skeleton`) で一度だけ作ってコピーする。これは雛形を検査するテスト用
    pub fn rust_skeleton(mut self, enable: bool) -> Self {
        self.set_kind(OutputKind::RustSkeleton, enable);
        self
    }

    /// 読んだファイル (import した先も) の `cargo:rerun-if-changed` を出すか。既定は出す
    pub fn emit_rerun_if_changed(mut self, enable: bool) -> Self {
        self.emit_rerun_if_changed = enable;
        self
    }

    /// import の探し先を足す (仕様 §24.1)。依存する型のクレートが `cargo:idl_dir` で出したディレクトリ
    /// (`DEP_<links>_IDL_DIR`) などを指定する。環境変数 `JUIZ_IDL_PATH` もこの後に探す
    pub fn import_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.import_dirs.push(dir.as_ref().to_path_buf());
        self
    }

    /// 名前空間 `namespace` (と下の名前空間) の型をほかのクレートのものとして使う (仕様 §24.2)。
    /// その型は書き出さず、`rust_path` の下の型を参照する
    ///
    /// ```no_run
    /// juiz_idl::build::configure()
    ///     .extern_path("juiz_std", "::juiz_std_types::juiz_std")
    ///     .compile(&["idl/my_robot.jidl"])
    ///     .unwrap();
    /// ```
    pub fn extern_path(mut self, namespace: &str, rust_path: &str) -> Self {
        self.externs.push(ExternNamespace {
            namespace: namespace.to_owned(),
            rust_path: Some(rust_path.to_owned()),
        });
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
        let load = LoadOptions {
            import_dirs: self.import_dirs,
        }
        .with_env_import_path();
        let model = load_files_with(files, &load)?;
        if self.emit_rerun_if_changed {
            // 組み込みの型 (juiz_std/…) は juiz_idl の中にあるので、ファイルとしては出さない
            for f in model.files.iter() {
                println!("cargo:rerun-if-changed={}", f.display());
            }
            println!("cargo:rerun-if-env-changed={IMPORT_PATH_ENV}");
        }
        let options = GenerateOptions {
            externs: self.externs,
        };
        write_files(&generate_with(&model, &self.kinds, &options)?, &out_dir)
    }
}
