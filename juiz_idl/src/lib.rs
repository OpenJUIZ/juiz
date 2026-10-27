//! juiz の IDL (JIDL) の生成器 (issue #191、仕様 docs/spec/datatype.md §9・§11・§16)
//!
//! .jidl (と手で書いた .avsc) を読み、次のものを書き出す。
//!
//! | 出力 | ファイル | 内容 |
//! |---|---|---|
//! | [`OutputKind::Avsc`] | `<完全名>.avsc` | 名前付き型ごとの、それだけで完結した Avro のスキーマ (型の正本) |
//! | [`OutputKind::JsonSchema`] | `<完全名>.schema.json` | 辞書の表し方 (仕様 §4) の JSON Schema 2020-12 |
//! | [`OutputKind::Processes`] | `<namespace>.processes.json` | `process` / `container` 宣言のシグネチャ (仕様 §9.3) |
//! | [`OutputKind::Rust`] | `<namespace>.rs`、`juiz_idl.rs` | struct / enum と `juiz_sdk::types::JuizType` の実装 |
//!
//! 型のモデル・指紋は `juiz_types` を使う (juiz_sdk には依存しない)。生成した Rust のコードは juiz_sdk を使う。
//! Java の avro-tools は使わない。
//!
//! ```
//! let model = juiz_idl::load_str(r#"
//!     namespace robot.geometry;
//!     record Point2D { double x; double y = 0.0; }
//!     process Point2D origin();
//! "#, "geometry.jidl").unwrap();
//! let files = juiz_idl::generate(&model, &[juiz_idl::OutputKind::Avsc]).unwrap();
//! assert_eq!(files[0].path.to_str(), Some("robot.geometry.Point2D.avsc"));
//! ```
//!
//! build.rs からは [`build`] を使う。

pub mod build;
pub mod cli;
mod json_schema;
mod model;
mod parser;
mod rust;

use std::path::{Path, PathBuf};

use juiz_types::TypeError;

pub use json_schema::{json_schema_of, JSON_SCHEMA_DIALECT};
pub use model::{
    load_files, load_str, ArgumentSignature, ContainerSignature, IdlModel, ProcessSignature,
    Signatures,
};
pub use parser::{parse_idl, IdlFile};
pub use rust::{generate_rust, MODULE_TREE_FILE};

/// juiz-idl のエラー。位置は 1 から数える
#[derive(thiserror::Error, Debug, Clone, PartialEq)]
pub enum IdlError {
    /// JIDL の文法の誤り
    #[error("{file}:{line}:{column}: syntax error: {message}")]
    Syntax {
        file: String,
        line: usize,
        column: usize,
        message: String,
    },
    /// 名前の解決や宣言の誤り
    #[error("{file}:{line}:{column}: {message}")]
    Semantic {
        file: String,
        line: usize,
        column: usize,
        message: String,
    },
    /// 型が仕様 §3 の規則に合わない (juiz_types の検査)
    #[error("{file}:{line}:{column}: {error}")]
    Type {
        file: String,
        line: usize,
        column: usize,
        error: Box<TypeError>,
    },
    #[error("{path}: {message}")]
    Io { path: String, message: String },
    /// コードを書き出せない
    #[error("{message}")]
    Generate { message: String },
}

/// 書き出すものの種類
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OutputKind {
    Avsc,
    JsonSchema,
    Processes,
    Rust,
}

impl OutputKind {
    pub const ALL: [OutputKind; 4] = [
        OutputKind::Avsc,
        OutputKind::JsonSchema,
        OutputKind::Processes,
        OutputKind::Rust,
    ];
}

/// 書き出すファイル 1 つ。`path` は出力先のディレクトリからの相対パス
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedFile {
    pub path: PathBuf,
    pub contents: String,
}

/// 先に書くキー。serde_json の Map はキーを辞書順に並べるので、人が読みやすい順に並べ直して書く
const KEY_ORDER: [&str; 23] = [
    "$schema",
    "name",
    "type_name",
    "type",
    "namespace",
    "title",
    "description",
    "doc",
    "fingerprint",
    "aliases",
    "symbols",
    "items",
    "values",
    "properties",
    "required",
    "fields",
    "arguments",
    "schema",
    "default",
    "output_schema",
    "avro",
    "processes",
    "containers",
];
/// record・enum の定義は Avro の慣例どおり type、name の順に書く
const NAMED_TYPE_KEY_ORDER: [&str; 3] = ["type", "name", "namespace"];

fn key_rank(m: &serde_json::Map<String, serde_json::Value>, key: &str) -> usize {
    let named = matches!(
        m.get("type").and_then(|t| t.as_str()),
        Some("record" | "enum")
    );
    if named {
        if let Some(i) = NAMED_TYPE_KEY_ORDER.iter().position(|o| *o == key) {
            return i;
        }
    }
    NAMED_TYPE_KEY_ORDER.len()
        + KEY_ORDER
            .iter()
            .position(|o| *o == key)
            .unwrap_or(KEY_ORDER.len())
}

fn pretty(v: &impl serde::Serialize) -> String {
    let v = serde_json::to_value(v).unwrap();
    let mut out = String::new();
    write_pretty(&v, 0, &mut out);
    out.push('\n');
    out
}

fn write_pretty(v: &serde_json::Value, indent: usize, out: &mut String) {
    use serde_json::Value;
    let pad = |n: usize| "  ".repeat(n);
    match v {
        Value::Array(a) if !a.is_empty() => {
            out.push_str("[\n");
            for (i, e) in a.iter().enumerate() {
                out.push_str(&pad(indent + 1));
                write_pretty(e, indent + 1, out);
                out.push_str(if i + 1 < a.len() { ",\n" } else { "\n" });
            }
            out.push_str(&pad(indent));
            out.push(']');
        }
        Value::Object(m) if !m.is_empty() => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort_by_key(|k| key_rank(m, k));
            out.push_str("{\n");
            for (i, k) in keys.iter().enumerate() {
                out.push_str(&pad(indent + 1));
                out.push_str(&serde_json::to_string(k).unwrap());
                out.push_str(": ");
                write_pretty(&m[k.as_str()], indent + 1, out);
                out.push_str(if i + 1 < keys.len() { ",\n" } else { "\n" });
            }
            out.push_str(&pad(indent));
            out.push('}');
        }
        other => out.push_str(&serde_json::to_string(other).unwrap()),
    }
}

/// モデルから `kinds` のファイルを作る (書き込まない)
pub fn generate(model: &IdlModel, kinds: &[OutputKind]) -> Result<Vec<GeneratedFile>, IdlError> {
    let mut files = Vec::new();
    for kind in kinds.iter() {
        match kind {
            OutputKind::Avsc => {
                for t in model.types.iter() {
                    let avro = t.to_avro().map_err(|e| IdlError::Generate {
                        message: format!("{}: {e}", t.display_name()),
                    })?;
                    files.push(GeneratedFile {
                        path: format!("{}.avsc", t.display_name()).into(),
                        contents: pretty(&avro),
                    });
                }
            }
            OutputKind::JsonSchema => {
                for t in model.types.iter() {
                    files.push(GeneratedFile {
                        path: format!("{}.schema.json", t.display_name()).into(),
                        contents: pretty(&json_schema_of(t)),
                    });
                }
            }
            OutputKind::Processes => {
                for s in model.signatures.iter() {
                    files.push(GeneratedFile {
                        path: format!("{}.processes.json", s.namespace).into(),
                        contents: pretty(s),
                    });
                }
            }
            OutputKind::Rust => files.extend(generate_rust(&model.types)?),
        }
    }
    Ok(files)
}

/// ファイルを `out_dir` に書く。中身が同じファイルは書き直さない (build.rs で無駄な再ビルドを起こさないため)
pub fn write_files(files: &[GeneratedFile], out_dir: &Path) -> Result<(), IdlError> {
    std::fs::create_dir_all(out_dir).map_err(|e| IdlError::Io {
        path: out_dir.display().to_string(),
        message: e.to_string(),
    })?;
    for f in files.iter() {
        let path = out_dir.join(&f.path);
        if std::fs::read_to_string(&path).ok().as_deref() == Some(f.contents.as_str()) {
            continue;
        }
        std::fs::write(&path, &f.contents).map_err(|e| IdlError::Io {
            path: path.display().to_string(),
            message: e.to_string(),
        })?;
    }
    Ok(())
}
