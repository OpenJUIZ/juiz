//! `juiz pkg design` と `juiz pkg design check` (docs/spec/pkg.md §2.6・§2.7)

use std::path::{Path, PathBuf};

use super::{check, parse, preset, to_yaml, Report, FILE_NAME};
use crate::cli::{DesignArgs, DesignCheckArgs};
use crate::PkgError;

/// `juiz pkg design` の結果
#[derive(Debug, Clone)]
pub struct Written {
    pub path: PathBuf,
    pub content: String,
    /// 成功したときに表示する文
    pub summary: String,
}

fn shown(p: &Path) -> String {
    p.display().to_string()
}

/// 設計文書だけを書く (§2.6)。`-t` のプリセットに `-b` の言語を書いたもの
pub fn design(name: &str, args: &DesignArgs) -> Result<Written, PkgError> {
    let template = args.template;
    let path = args.output.clone().unwrap_or_else(|| PathBuf::from(FILE_NAME));
    let label = shown(&path);
    let mut d = preset(name, template);
    d.component.language = args.lang;
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

/// 設計文書を検査する (§2.7)。ファイルは何も書かない
pub fn check_file(args: &DesignCheckArgs) -> Result<Checked, PkgError> {
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
            let r = check(&d, lang, &base);
            (Some(d), r)
        }
    };
    let mut lines: Vec<String> = report.errors.iter().map(|e| e.format(&label, "error")).collect();
    lines.extend(report.warnings.iter().map(|w| w.format(&label, "warning")));
    let ok_line = match (&design, report.is_ok()) {
        (Some(d), true) => Some(format!("ok: {label} ({})", d.counts())),
        _ => None,
    };
    Ok(Checked {
        ok_line,
        lines,
        report,
    })
}
