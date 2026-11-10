//! juiz の雛形生成 (`juiz pkg create` / `juiz-pkg create`、docs/spec/pkg.md)
//!
//! Process・Container・Component を作り始めるときの「最初のプロジェクト」(1 つの .juizapp になるもの) を作る。
//! テンプレートは templates/ の下にあり、build.rs でバイナリに埋め込む。

pub mod cli;
pub mod create;
pub mod names;
pub mod render;

mod templates {
    include!(concat!(env!("OUT_DIR"), "/templates.rs"));
}

use clap::ValueEnum;

/// このクレートをビルドしたときの target triple (`juiz pkg target`、§2.3)。
/// juiz と一緒にビルドすれば juiz_core::HOST_TARGET と同じ
pub const HOST_TARGET: &str = env!("JUIZ_TARGET");

/// 雛形に書く juiz の版 (§7.1)。ワークスペースで juiz と同じ版
pub const JUIZ_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 埋め込んだテンプレート (templates/ からの相対パス、中身)
pub fn templates() -> &'static [(&'static str, &'static str)] {
    templates::TEMPLATES
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkgError(String);

impl PkgError {
    pub fn new(msg: impl Into<String>) -> Self {
        PkgError(msg.into())
    }
}

impl std::fmt::Display for PkgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PkgError {}

/// 言語 (`-b` / `--lang`)
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Lang {
    Rust,
    Python,
    #[value(name = "cpp", alias = "c++")]
    Cpp,
    Haskell,
}

impl Lang {
    /// templates/ の下のディレクトリ名
    pub fn dir(self) -> &'static str {
        match self {
            Lang::Rust => "rust",
            Lang::Python => "python",
            Lang::Cpp => "cpp",
            Lang::Haskell => "haskell",
        }
    }

    /// conf の plugins の language
    pub fn plugin_language(self) -> &'static str {
        match self {
            Lang::Rust => "rust",
            Lang::Python => "python",
            Lang::Cpp => "c++",
            Lang::Haskell => "haskell",
        }
    }

    /// conf の plugins の path
    pub fn plugin_path(self) -> &'static str {
        match self {
            Lang::Python => "python",
            _ => "plugins/{target}",
        }
    }

    /// Windows 向けの scripts/*.ps1 があるか (Haskell の Windows は #137 まで無い、§5.3)
    pub fn has_ps1(self) -> bool {
        self != Lang::Haskell
    }

    /// ビルドして app/plugins/<target>/ に置く scripts/dev.* があるか
    pub fn has_build(self) -> bool {
        self != Lang::Python
    }
}

/// テンプレートの種類 (`-t` / `--template`、§4)
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Template {
    Component,
    Process,
    Container,
}

impl Template {
    pub fn dir(self) -> &'static str {
        match self {
            Template::Component => "component",
            Template::Process => "process",
            Template::Container => "container",
        }
    }

    pub fn has_process(self) -> bool {
        self != Template::Container
    }

    pub fn has_container(self) -> bool {
        self != Template::Process
    }
}
