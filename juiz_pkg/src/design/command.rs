//! `juiz pkg design` と `juiz pkg design check` (docs/spec/pkg.md §2.6・§2.7)

use std::path::{Path, PathBuf};

use super::plugin::{self, PluginLoader};
use super::{check, check_and_resolve, parse, preset, to_yaml, Report, FILE_NAME};
use crate::cli::{DesignArgs, DesignCheckArgs};
use crate::idl::{relative_under, IdlSource};
use crate::PkgError;

/// `juiz pkg design` の結果
#[derive(Debug, Clone)]
pub struct Written {
    pub path: PathBuf,
    pub content: String,
    /// 成功したときに表示する文
    pub summary: String,
    /// 警告 (`--from-plugin` で型の無い引数など)
    pub warnings: Vec<String>,
}

fn shown(p: &Path) -> String {
    p.display().to_string()
}

/// 設計文書だけを書く (§2.6)。`-t` のプリセット、`--idl` の .jidl から作った文書 (Step 4)、
/// または `--from-plugin` のプラグインの manifest から作った文書 (Step 5) に `-b` の言語を書いたもの
pub fn design(name: Option<&str>, args: &DesignArgs, loader: PluginLoader) -> Result<Written, PkgError> {
    if let Some(lib) = &args.from_plugin {
        return design_from_plugin(name, lib, args, loader);
    }
    let name = name.ok_or_else(|| PkgError::new("give a component name: juiz pkg design <name>"))?;
    let template = args.template;
    let path = args.output.clone().unwrap_or_else(|| PathBuf::from(FILE_NAME));
    let label = shown(&path);
    // 書き先がある (ファイルでもディレクトリでも) ならエラー (上書きしない、pkg.md §8)
    if std::fs::symlink_metadata(&path).is_ok() {
        return Err(PkgError::new(format!(
            "{label} already exists (juiz pkg design does not overwrite files)"
        )));
    }
    let base = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let d = match &args.idl {
        // .jidl の宣言から (design.md §6.2・§8.4)。`idl:` は文書のディレクトリからの相対パス
        Some(jidl) => {
            let src = IdlSource::load(jidl)?;
            let main = src.dir.join(Path::new(&src.main));
            let rel = base
                .canonicalize()
                .ok()
                .and_then(|b| relative_under(&main, &b))
                .ok_or_else(|| {
                    PkgError::new(format!(
                        "--idl {}: the .jidl is not in the directory of {label}, and idl: of the document must be a path in that directory; write the document there with -o {}",
                        jidl.display(),
                        src.dir.join(FILE_NAME).display()
                    ))
                })?;
            src.design(name, args.lang, &rel)?
        }
        None => {
            let mut d = preset(name, template);
            d.component.language = args.lang;
            d
        }
    };
    let report = check(&d, args.lang, &base);
    if !report.is_ok() {
        return Err(PkgError::new(format!(
            "the design is not valid:\n{}",
            report.error_text(&label)
        )));
    }
    let content = to_yaml(&d);
    std::fs::create_dir_all(&base)
        .map_err(|e| PkgError::new(format!("cannot create {}: {e}", base.display())))?;
    std::fs::write(&path, content.as_bytes())
        .map_err(|e| PkgError::new(format!("cannot write {label}: {e}")))?;
    let lang = match args.lang {
        Some(_) => String::new(),
        None => " -b <rust|python|cpp|haskell|zig>".to_owned(),
    };
    let summary = format!(
        "Wrote {label} ({})\n\nNext steps:\n  # edit {label}, then\n  juiz pkg design check {label}{lang}\n  juiz pkg create -d {label}{lang}\n",
        d.counts(),
    );
    Ok(Written {
        path,
        content,
        summary,
        warnings: Vec::new(),
    })
}

/// 書き先のファイルと、そのディレクトリ
fn output_of(args: &DesignArgs) -> Result<(PathBuf, PathBuf), PkgError> {
    let path = args.output.clone().unwrap_or_else(|| PathBuf::from(FILE_NAME));
    if std::fs::symlink_metadata(&path).is_ok() {
        return Err(PkgError::new(format!(
            "{} already exists (juiz pkg design does not overwrite files)",
            shown(&path)
        )));
    }
    let base = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    Ok((path, base))
}

/// `juiz pkg design --from-plugin` (design.md §8.6)
fn design_from_plugin(name: Option<&str>, lib: &Path, args: &DesignArgs, loader: PluginLoader) -> Result<Written, PkgError> {
    let (path, base) = output_of(args)?;
    let label = shown(&path);
    let lang = plugin::plugin_lang(lib, args.lang)?;
    let manifest = plugin::load(loader, lib, lang)?;
    let comp = name.unwrap_or(&manifest.type_name).to_owned();
    let jidl_rel = format!("idl/{comp}.jidl");
    let made = plugin::from_manifest(&manifest, name, lang, &jidl_rel)?;
    let jidl_file = base.join("idl").join(format!("{comp}.jidl"));
    if made.jidl.is_some() && std::fs::symlink_metadata(&jidl_file).is_ok() {
        return Err(PkgError::new(format!(
            "{} already exists (juiz pkg design does not overwrite files); write the document to another directory with -o",
            shown(&jidl_file)
        )));
    }
    // 検査は idl: のファイルを読むので、.jidl を先に書き、通らなければ消す
    let made_idl_dir = !base.join("idl").exists();
    let cleanup = || {
        if made.jidl.is_some() {
            let _ = std::fs::remove_file(&jidl_file);
            if made_idl_dir {
                let _ = std::fs::remove_dir(base.join("idl"));
            }
        }
    };
    if let Some(text) = &made.jidl {
        std::fs::create_dir_all(base.join("idl"))
            .map_err(|e| PkgError::new(format!("cannot create {}: {e}", base.join("idl").display())))?;
        std::fs::write(&jidl_file, text.as_bytes())
            .map_err(|e| PkgError::new(format!("cannot write {}: {e}", shown(&jidl_file))))?;
    }
    let d = &made.design;
    let report = check(d, Some(lang), &base);
    if !report.is_ok() {
        cleanup();
        let hint = if name.is_none() && report.errors.iter().any(|e| e.path == "component.name") {
            "\n(give another component name: juiz pkg design <name> --from-plugin …)"
        } else {
            ""
        };
        return Err(PkgError::new(format!(
            "the design made from the plugin {} is not valid:\n{}{hint}",
            lib.display(),
            report.error_text(&label)
        )));
    }
    let content = to_yaml(d);
    if let Err(e) = std::fs::create_dir_all(&base).and_then(|_| std::fs::write(&path, content.as_bytes())) {
        cleanup();
        return Err(PkgError::new(format!("cannot write {label}: {e}")));
    }
    let mut summary = format!("Wrote {label} ({}) from the plugin {}\n", d.counts(), lib.display());
    if made.jidl.is_some() {
        summary.push_str(&format!(
            "Wrote {} (the record and enum types of the plugin, idl: of the document)\n",
            shown(&jidl_file)
        ));
    }
    if !d.containers.is_empty() {
        summary.push_str(
            "\nA plugin does not tell what its containers hold, so state: is empty; write it before juiz pkg create -d.\n",
        );
    }
    summary.push_str(&format!(
        "\nNext steps:\n  juiz pkg design check {label} --plugin {}\n  juiz pkg create -d {label}\n",
        lib.display()
    ));
    Ok(Written {
        path,
        content,
        summary,
        warnings: made.warnings.clone(),
    })
}

/// `juiz pkg design check` の結果
#[derive(Debug, Clone)]
pub struct Checked {
    /// 通ったときの 1 行 (`ok: …`)。エラーがあれば None
    pub ok_line: Option<String>,
    /// エラーと警告の行 (`<file>:<行>:<桁>: error: …`)
    pub lines: Vec<String>,
    pub report: Report,
}

/// 設計文書を検査する (§2.7)。`--plugin` があれば、文書が検査を通ったときにプラグインの manifest とも突き合わせる
/// (design.md §8.5)。ファイルは何も書かない
pub fn check_file(args: &DesignCheckArgs, loader: PluginLoader) -> Result<Checked, PkgError> {
    let label = shown(&args.file);
    let text = std::fs::read_to_string(&args.file)
        .map_err(|e| PkgError::new(format!("cannot read {label}: {e}")))?;
    let (design, report) = match parse(&text) {
        Err(r) => (None, r),
        Ok(d) => {
            let lang = args.lang.or(d.component.language);
            let base = args
                .file
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            let (mut r, resolved) = check_and_resolve(&d, lang, &base);
            if let (Some(lib), Some(resolved)) = (&args.plugin, resolved) {
                let pl = plugin::plugin_lang(lib, lang)?;
                let m = plugin::load(loader, lib, pl)?;
                let p = plugin::compare(&d, &resolved.signatures, &m);
                r.errors.extend(p.errors);
                r.warnings.extend(p.warnings);
            }
            (Some(d), r)
        }
    };
    let mut lines: Vec<String> = report.errors.iter().map(|e| e.format(&label, "error")).collect();
    lines.extend(report.warnings.iter().map(|w| w.format(&label, "warning")));
    let ok_line = match (&design, report.is_ok()) {
        (Some(d), true) => Some(match &args.plugin {
            Some(lib) => format!("ok: {label} ({}) matches the plugin {}", d.counts(), lib.display()),
            None => format!("ok: {label} ({})", d.counts()),
        }),
        _ => None,
    };
    Ok(Checked {
        ok_line,
        lines,
        report,
    })
}
