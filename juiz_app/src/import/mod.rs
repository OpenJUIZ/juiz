//! URDF・複数 link の SDF の取り込み (ContainerModel Step 9、issue #388、仕様 docs/spec/container-model.md §13.1)
//!
//! ```text
//! juiz import urdf <file> [-o <conf>] [--anchor <名前>] [--namespace <ns>]
//!                  [--package-path <パッケージ>=<ディレクトリ>]... [--model-path <ディレクトリ>]...
//!                  [--models-dir <ディレクトリ>] [--copy-meshes] [--force]
//! juiz import sdf  <file> (同じ)
//! ```
//!
//! - link → Container (組み込みの `frame`、根の link は `static_anchor`)、joint → Transform (関節の値 0 の位置)、
//!   visual → model。System は起動しない。
//! - visual が 1 つで、.glb / .gltf / .stl のメッシュならそのファイルを `model.path` に指す。
//!   それ以外 (基本形・複数の visual) は link ごとに SDF の部分集合 (§3.2) を書き出して指す。
//! - `package://<pkg>/…` は `--package-path`・`ROS_PACKAGE_PATH`・`AMENT_PREFIX_PATH` (`<prefix>/share/<pkg>`)、
//!   `model://<名前>/…` は `--model-path`・`GZ_SIM_RESOURCE_PATH`・`IGN_GAZEBO_RESOURCE_PATH`・`SDF_PATH`、
//!   `file://` と相対パスは読んだファイルのディレクトリから解決する。

mod robot;
mod sdf;
mod urdf;
mod write;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use clap::{Args as ClapArgs, Subcommand};

#[derive(Debug, Subcommand, Clone)]
pub(crate) enum ImportSubCommands {
    /// Import a URDF as a conf (link -> Container, joint -> Transform, visual -> model)
    #[clap(arg_required_else_help = true)]
    Urdf {
        #[command(flatten)]
        args: ImportArgs,
    },
    /// Import an SDF with several links and joints as a conf (same as urdf)
    #[clap(arg_required_else_help = true)]
    Sdf {
        #[command(flatten)]
        args: ImportArgs,
    },
}

#[derive(Debug, ClapArgs, Clone)]
pub(crate) struct ImportArgs {
    #[arg(help = "URDF / SDF file")]
    pub file: String,

    #[arg(
        short = 'o',
        long = "output",
        help = "Conf file to write (default: print to stdout; model files go under the current directory)"
    )]
    pub output: Option<String>,

    #[arg(
        long = "anchor",
        help = "Add a static_anchor of this name above the root link(s). Without it, the root link itself becomes a static_anchor"
    )]
    pub anchor: Option<String>,

    #[arg(long = "namespace", help = "Namespace of the Containers (ex., robot1)")]
    pub namespace: Option<String>,

    #[arg(
        long = "package-path",
        value_name = "PACKAGE=DIR",
        help = "Directory of a ROS package for package:// URIs (repeatable)"
    )]
    pub package_paths: Vec<String>,

    #[arg(
        long = "model-path",
        value_name = "DIR",
        help = "Directory that has Gazebo models for model:// URIs (repeatable)"
    )]
    pub model_paths: Vec<String>,

    #[arg(
        long = "models-dir",
        help = "Where to write the model files, relative to the conf (default: models/<robot name>)"
    )]
    pub models_dir: Option<String>,

    #[arg(
        long = "copy-meshes",
        help = "Copy the mesh files into the models directory (for a .juizapp package)"
    )]
    pub copy_meshes: bool,

    #[arg(long = "force", help = "Overwrite existing files")]
    pub force: bool,
}

/// 取り込みの結果 (書き出す前)
pub(crate) struct ImportOutput {
    /// conf の YAML
    pub conf: String,
    /// 書き出すモデルのファイル (絶対パス、中身)
    pub files: Vec<(PathBuf, Vec<u8>)>,
    /// 写すメッシュのファイル (元、先)
    pub copies: Vec<(PathBuf, PathBuf)>,
    pub warnings: Vec<String>,
}

/// URI の解決に使う場所
#[derive(Debug, Clone, Default)]
pub(crate) struct Resolver {
    /// 読んだファイルのディレクトリ (相対パスの起点)
    pub base_dir: PathBuf,
    /// `--package-path` (パッケージ名 → ディレクトリ)
    pub packages: Vec<(String, PathBuf)>,
    /// `ROS_PACKAGE_PATH` の各ディレクトリ (`<dir>/<pkg>` を探す)
    pub package_roots: Vec<PathBuf>,
    /// `AMENT_PREFIX_PATH` の各ディレクトリ (`<prefix>/share/<pkg>` を探す)
    pub ament_prefixes: Vec<PathBuf>,
    /// `--model-path` と Gazebo の環境変数 (`<dir>/<名前>` を探す)
    pub model_roots: Vec<PathBuf>,
}

fn split_paths(var: &str) -> Vec<PathBuf> {
    std::env::var_os(var)
        .map(|v| {
            std::env::split_paths(&v)
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        })
        .unwrap_or_default()
}

impl Resolver {
    /// 引数と環境変数から作る
    pub fn from_args(base_dir: &Path, args: &ImportArgs) -> Result<Resolver, String> {
        let mut packages = Vec::new();
        for p in &args.package_paths {
            let (name, dir) = p
                .split_once('=')
                .ok_or_else(|| format!("--package-path must be PACKAGE=DIR, but '{p}'"))?;
            packages.push((name.to_owned(), PathBuf::from(dir)));
        }
        let mut model_roots: Vec<PathBuf> = args.model_paths.iter().map(PathBuf::from).collect();
        for var in [
            "GZ_SIM_RESOURCE_PATH",
            "IGN_GAZEBO_RESOURCE_PATH",
            "SDF_PATH",
        ] {
            model_roots.extend(split_paths(var));
        }
        Ok(Resolver {
            base_dir: base_dir.to_path_buf(),
            packages,
            package_roots: split_paths("ROS_PACKAGE_PATH"),
            ament_prefixes: split_paths("AMENT_PREFIX_PATH"),
            model_roots,
        })
    }

    /// メッシュの URI をファイルのパスにする。見つからなければ理由
    pub fn resolve(&self, uri: &str) -> Result<PathBuf, String> {
        let found = |p: PathBuf| -> Result<PathBuf, String> {
            if p.is_file() {
                Ok(p)
            } else {
                Err(format!(
                    "'{uri}' is resolved to '{}', but it does not exist",
                    p.display()
                ))
            }
        };
        if let Some(rest) = uri.strip_prefix("package://") {
            let (pkg, path) = rest.split_once('/').unwrap_or((rest, ""));
            if let Some((_, dir)) = self.packages.iter().find(|(n, _)| n == pkg) {
                return found(dir.join(path));
            }
            let candidates = self.package_roots.iter().map(|r| r.join(pkg)).chain(
                self.ament_prefixes
                    .iter()
                    .map(|p| p.join("share").join(pkg)),
            );
            for dir in candidates {
                if dir.is_dir() {
                    return found(dir.join(path));
                }
            }
            return Err(format!(
                "package '{pkg}' of '{uri}' is not found. Give --package-path {pkg}=<dir> \
                 (or set ROS_PACKAGE_PATH / AMENT_PREFIX_PATH)"
            ));
        }
        if let Some(rest) = uri.strip_prefix("model://") {
            let (model, path) = rest.split_once('/').unwrap_or((rest, ""));
            for root in &self.model_roots {
                let dir = root.join(model);
                if dir.is_dir() {
                    return found(dir.join(path));
                }
            }
            return Err(format!(
                "model '{model}' of '{uri}' is not found. Give --model-path <dir that has {model}/> \
                 (or set GZ_SIM_RESOURCE_PATH)"
            ));
        }
        if let Some(rest) = uri.strip_prefix("file://") {
            return found(PathBuf::from(rest));
        }
        if uri.contains("://") {
            return Err(format!(
                "'{uri}' is not supported (package://, model://, file:// or a path)"
            ));
        }
        let p = Path::new(uri);
        found(if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.base_dir.join(p)
        })
    }
}

/// `juiz import` の入口。終了コードを返す
pub(crate) fn run_import_command(subcommand: &ImportSubCommands) -> i32 {
    let (kind, args) = match subcommand {
        ImportSubCommands::Urdf { args } => (Kind::Urdf, args),
        ImportSubCommands::Sdf { args } => (Kind::Sdf, args),
    };
    match run(kind, args) {
        Ok(warnings) => {
            for w in &warnings {
                eprintln!("warning: {w}");
            }
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Kind {
    Urdf,
    Sdf,
}

/// 読んで、書き出す。警告を返す
fn run(kind: Kind, args: &ImportArgs) -> Result<Vec<String>, String> {
    let input = Path::new(&args.file);
    let text = std::fs::read_to_string(input)
        .map_err(|e| format!("can not read '{}': {e}", input.display()))?;
    let input_dir = std::path::absolute(input)
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let conf_dir = match args.output.as_deref() {
        Some(o) => std::path::absolute(Path::new(o))
            .map_err(|e| format!("bad output path '{o}': {e}"))?
            .parent()
            .map(|d| d.to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".")),
        None => std::env::current_dir().map_err(|e| e.to_string())?,
    };
    let resolver = Resolver::from_args(&input_dir, args)?;
    let out = import(kind, &text, &resolver, &conf_dir, args)?;
    // 先に全部の書き出し先を確かめてから書く (途中で止まって半端なファイルを残さない)
    let mut targets: Vec<&Path> = out.files.iter().map(|(p, _)| p.as_path()).collect();
    targets.extend(out.copies.iter().map(|(_, to)| to.as_path()));
    let conf_path = args.output.as_deref().map(PathBuf::from);
    if let Some(c) = &conf_path {
        targets.push(c);
    }
    if !args.force {
        if let Some(p) = targets.iter().find(|p| p.exists()) {
            return Err(format!(
                "'{}' already exists. Use --force to overwrite",
                p.display()
            ));
        }
    }
    for (path, bytes) in &out.files {
        write_file(path, bytes)?;
    }
    for (from, to) in &out.copies {
        if let Some(d) = to.parent() {
            std::fs::create_dir_all(d)
                .map_err(|e| format!("can not create '{}': {e}", d.display()))?;
        }
        std::fs::copy(from, to).map_err(|e| {
            format!(
                "can not copy '{}' to '{}': {e}",
                from.display(),
                to.display()
            )
        })?;
    }
    match &conf_path {
        Some(c) => {
            write_file(c, out.conf.as_bytes())?;
            eprintln!(
                "wrote: {} ({} model file(s), {} mesh copy(ies))",
                c.display(),
                out.files.len(),
                out.copies.len()
            );
        }
        None => print!("{}", out.conf),
    }
    Ok(out.warnings)
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("can not create '{}': {e}", d.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("can not write '{}': {e}", path.display()))
}

/// 読んで conf とモデルのファイルを作る (書き出さない)。`conf_dir` は conf を置くディレクトリ (絶対パス)
pub(crate) fn import(
    kind: Kind,
    text: &str,
    resolver: &Resolver,
    conf_dir: &Path,
    args: &ImportArgs,
) -> Result<ImportOutput, String> {
    let mut warnings = Vec::new();
    let robot = match kind {
        Kind::Urdf => urdf::parse(text, &mut warnings)?,
        Kind::Sdf => sdf::parse(text, &mut warnings)?,
    };
    write::build(&robot, kind, resolver, conf_dir, args, warnings)
}
