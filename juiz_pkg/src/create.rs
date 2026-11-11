//! `juiz pkg create` (docs/spec/pkg.md §2.2、§5〜§9)

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::cli::{CreateArgs, LicenseKind};
use crate::render::{render, render_path, Vars};
use crate::idl::IdlPlan;
use crate::{names, templates, Lang, PkgError, Template, JUIZ_VERSION};

/// 生成するファイル 1 つ
#[derive(Debug, Clone)]
pub struct GeneratedFile {
    /// 雛形のディレクトリからの相対パス (/ 区切り)
    pub path: String,
    pub content: String,
}

impl GeneratedFile {
    /// 実行権限を付けるか (§5)
    pub fn executable(&self) -> bool {
        self.path.ends_with(".sh")
    }
}

/// 生成の結果
#[derive(Debug, Clone)]
pub struct Created {
    pub dir: PathBuf,
    pub files: Vec<GeneratedFile>,
    pub warnings: Vec<String>,
    /// 成功したときに表示する文 (§9)
    pub summary: String,
}

/// 生成に使う値をまとめたもの (オプションの解釈と検査が済んだもの)
#[derive(Debug, Clone)]
pub struct Plan {
    pub name: String,
    pub lang: Lang,
    pub template: Template,
    /// 表示用の作り先 (`-o` に書いたもの、省けば ./<name>)
    pub display_dir: String,
    /// 次に打つコマンドの cd の先 (`-o` に書いたもの、省けば <name>)
    pub cd_dir: String,
    pub license: bool,
    pub author: Option<String>,
    pub juiz_source: Option<PathBuf>,
    pub juiz_sdk_dir: PathBuf,
    /// `--juiz-sdk-dir` を指定したか (C++ の CMakeLists.txt に書くのは指定したときだけ、§6.3)
    pub juiz_sdk_dir_given: bool,
    /// juiz_sdk_dir に libjuiz_sdk の CMake のパッケージが無い (C++ の次の手順に SDK の用意を出す、§9)
    pub sdk_missing: bool,
    pub hsjuiz_dir: PathBuf,
    /// hsjuiz_dir に hsjuiz.cabal が無い (Haskell の次の手順に juiz のソースの用意を出す、§6.4)
    pub hsjuiz_missing: bool,
    /// `--hsjuiz-dir` を指定したか
    pub hsjuiz_given: bool,
    pub year: i64,
    /// Windows 向けの表示にするか (§9)
    pub windows: bool,
    /// `--idl` で読んだ .jidl (§6.5)。あれば -t の型の代わりに .jidl の宣言を入れる
    pub idl: Option<IdlPlan>,
}

/// 1970-01-01 からの日数を西暦の年にする (UTC)
fn year_from_days(days: i64) -> i64 {
    // Howard Hinnant の civil_from_days
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    yoe + era * 400 + if m <= 2 { 1 } else { 0 }
}

fn current_year() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    year_from_days(secs.div_euclid(86_400))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// `~` で始まるパスを展開する
fn expand_tilde(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if s == "~" {
        return home_dir().unwrap_or_else(|| p.to_path_buf());
    }
    if let Some(rest) = s.strip_prefix("~/").or_else(|| s.strip_prefix("~\\")) {
        if let Some(h) = home_dir() {
            return h.join(rest);
        }
    }
    p.to_path_buf()
}

/// 絶対パスにする (在るものは正規化する)
fn absolute(p: &Path) -> PathBuf {
    let p = expand_tilde(p);
    if let Ok(c) = std::fs::canonicalize(&p) {
        return strip_verbatim(c);
    }
    if p.is_absolute() {
        p
    } else {
        std::env::current_dir().map(|d| d.join(&p)).unwrap_or(p)
    }
}

/// Windows の canonicalize が付ける \\?\ を外す (ビルドの道具が読めないことがあるため)
fn strip_verbatim(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => p,
    }
}

/// ファイルに書くパス (Windows でも / 区切り)
fn slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// 場所を引数に付けるとき (空白があれば "" で囲む)
fn shell_arg(p: &Path) -> String {
    let d = slash(p);
    if d.contains(' ') {
        format!("\"{d}\"")
    } else {
        d
    }
}

/// libjuiz_sdk を用意するコマンド。`given` (--juiz-sdk-dir を指定した) ときはその場所を 2 つめの引数に付ける
fn sdk_install_command(dir: &Path, given: bool, windows: bool) -> String {
    let dest = if given {
        format!(" {}", shell_arg(dir))
    } else {
        String::new()
    };
    if windows {
        format!(
            "& ([scriptblock]::Create((irm https://raw.githubusercontent.com/OpenJUIZ/juiz/main/scripts/install-juiz-sdk.ps1))) {JUIZ_VERSION}{dest}"
        )
    } else {
        format!(
            "curl -fsSL https://raw.githubusercontent.com/OpenJUIZ/juiz/main/scripts/install-juiz-sdk.sh | sh -s -- {JUIZ_VERSION}{dest}"
        )
    }
}

/// hsjuiz を含む juiz のソースを取るコマンド (Haskell、§6.4)。
/// 既定の場所 (`~/juiz-<版>/bindings/hsjuiz`) なら `~/juiz-<版>` に clone する
fn hsjuiz_clone_command(hsjuiz_dir: &Path, given: bool) -> String {
    let clone = |dest: &str| {
        format!("git clone --depth 1 --branch v{JUIZ_VERSION} https://github.com/OpenJUIZ/juiz.git {dest}")
    };
    let source = hsjuiz_dir.parent().and_then(|p| p.parent());
    let ends_with_bindings = hsjuiz_dir.file_name().is_some_and(|n| n == "hsjuiz")
        && hsjuiz_dir.parent().and_then(|p| p.file_name()).is_some_and(|n| n == "bindings");
    match source {
        Some(src) if !given => clone(&format!("~/{}", src.file_name().unwrap_or_default().to_string_lossy())),
        Some(src) if ends_with_bindings => clone(&shell_arg(src)),
        _ => format!(
            "{} <dir>   # then use <dir>/bindings/hsjuiz",
            clone("")
                .trim_end()
        ),
    }
}

fn git_user_name() -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["config", "user.name"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (!s.is_empty()).then_some(s)
}

impl Plan {
    /// オプションを解釈して検査する。ファイルはまだ何も書かない (§8 の最後)
    pub fn from_args(args: &CreateArgs) -> Result<(Plan, Vec<String>), PkgError> {
        Self::from_args_with(args, git_user_name, cfg!(windows))
    }

    pub fn from_args_with(
        args: &CreateArgs,
        git_user: impl Fn() -> Option<String>,
        windows: bool,
    ) -> Result<(Plan, Vec<String>), PkgError> {
        let mut warnings = Vec::new();
        let lang = args.lang;
        names::validate(&args.name, lang)?;

        if args.hsjuiz_dir.is_some() && lang != Lang::Haskell {
            warnings.push("--hsjuiz-dir is only for -b haskell; ignored".to_owned());
        }
        if args.juiz_sdk_dir.is_some() && !matches!(lang, Lang::Cpp | Lang::Haskell) {
            warnings.push("--juiz-sdk-dir is only for -b cpp and -b haskell; ignored".to_owned());
        }
        if args.author.is_some() && args.license == LicenseKind::None {
            warnings.push("--author is only used with --license mit-apache; ignored".to_owned());
        }

        // --idl は先に読む (Rust のほかの言語はここでエラー)
        let idl = match &args.idl {
            Some(p) => {
                let plan = IdlPlan::load(&expand_tilde(p), lang)?;
                // 名前によるもの (Haskell のモジュール名の重なり) もファイルを書く前に確かめる
                plan.source(&args.name)?;
                Some(plan)
            }
            None => None,
        };

        let license = args.license == LicenseKind::MitApache;
        let author = if license {
            match args.author.clone().filter(|a| !a.trim().is_empty()).or_else(git_user) {
                Some(a) => Some(a),
                None => {
                    return Err(PkgError::new(
                        "--license mit-apache needs the copyright holder: give --author <name> (git config user.name is not set)",
                    ))
                }
            }
        } else {
            None
        };

        let juiz_source = match &args.juiz_source {
            Some(dir) => {
                let dir = absolute(dir);
                if !dir.join("juiz_sdk").join("Cargo.toml").is_file() {
                    return Err(PkgError::new(format!(
                        "--juiz-source {}: not a juiz repository (juiz_sdk/Cargo.toml is not there)",
                        dir.display()
                    )));
                }
                Some(dir)
            }
            None => None,
        };

        let home = home_dir().unwrap_or_else(|| PathBuf::from("~"));
        let juiz_sdk_dir = match &args.juiz_sdk_dir {
            Some(d) => absolute(d),
            None => home.join(".juiz").join("sdk").join(JUIZ_VERSION),
        };
        let sdk_missing = !juiz_sdk_dir
            .join("lib")
            .join("cmake")
            .join("juiz_sdk")
            .join("juiz_sdkConfig.cmake")
            .is_file();
        let hsjuiz_dir = match (&args.hsjuiz_dir, &juiz_source) {
            (Some(d), _) => absolute(d),
            (None, Some(src)) => src.join("bindings").join("hsjuiz"),
            (None, None) => home
                .join(format!("juiz-{JUIZ_VERSION}"))
                .join("bindings")
                .join("hsjuiz"),
        };

        let hsjuiz_missing = !hsjuiz_dir.join("hsjuiz.cabal").is_file();
        let hsjuiz_given = args.hsjuiz_dir.is_some() || juiz_source.is_some();
        if lang == Lang::Haskell {
            if windows {
                warnings.push(
                    "Haskell plugins on Windows are not supported yet (juiz issue #137): the skeleton is made for Linux and macOS, without .ps1 scripts"
                        .to_owned(),
                );
            }
            if hsjuiz_missing {
                warnings.push(format!(
                    "hsjuiz is not in {} (cabal.project points there). Get the juiz source of {JUIZ_VERSION}:\n    {}",
                    slash(&hsjuiz_dir),
                    hsjuiz_clone_command(&hsjuiz_dir, hsjuiz_given)
                ));
            }
            if sdk_missing {
                warnings.push(format!(
                    "libjuiz_sdk {JUIZ_VERSION} is not in {} (cabal.project points to its lib). Install it:\n    {}",
                    slash(&juiz_sdk_dir),
                    sdk_install_command(&juiz_sdk_dir, args.juiz_sdk_dir.is_some(), false)
                ));
            }
        }

        let (display_dir, cd_dir) = match &args.output {
            Some(o) => (o.display().to_string(), o.display().to_string()),
            None => (format!("./{}", args.name), args.name.clone()),
        };

        Ok((
            Plan {
                name: args.name.clone(),
                lang,
                template: args.template,
                display_dir,
                cd_dir,
                license,
                author,
                juiz_source,
                juiz_sdk_dir,
                juiz_sdk_dir_given: args.juiz_sdk_dir.is_some(),
                sdk_missing,
                hsjuiz_dir,
                hsjuiz_missing,
                hsjuiz_given,
                year: current_year(),
                windows,
                idl,
            },
            warnings,
        ))
    }

    /// juiz_sdk / pyjuiz の版の表示 (§9 の 1 行目)
    fn sdk_label(&self) -> String {
        match (&self.juiz_source, self.lang) {
            (Some(src), _) => format!("juiz sources at {}", src.display()),
            (None, Lang::Rust) => format!("juiz_sdk ={JUIZ_VERSION}"),
            (None, Lang::Python) => format!("pyjuiz =={JUIZ_VERSION}"),
            (None, Lang::Cpp) => format!("libjuiz_sdk {JUIZ_VERSION}"),
            (None, Lang::Haskell) => format!("hsjuiz {JUIZ_VERSION}"),
        }
    }

    /// 表示用の libjuiz_sdk の場所 (既定なら ~ のまま)
    fn sdk_dir_label(&self) -> PathBuf {
        if self.juiz_sdk_dir_given {
            self.juiz_sdk_dir.clone()
        } else if self.windows {
            PathBuf::from(format!("%USERPROFILE%/.juiz/sdk/{JUIZ_VERSION}"))
        } else {
            PathBuf::from(format!("~/.juiz/sdk/{JUIZ_VERSION}"))
        }
    }

    /// Rust の依存の右辺 (`"=<版>"`、--juiz-source ならソースの `<crate>`、§7)
    fn rust_dep(&self, krate: &str) -> String {
        match &self.juiz_source {
            Some(src) => format!("{{ path = '{}' }}", slash(&src.join(krate))),
            None => format!("\"={JUIZ_VERSION}\""),
        }
    }

    fn juiz_sdk_dep(&self) -> String {
        self.rust_dep("juiz_sdk")
    }

    /// テンプレートの種類の表示 (§9 の 1 行目)
    fn kind_label(&self) -> String {
        match &self.idl {
            Some(i) => format!("from {}", i.main),
            None => self.template.dir().to_owned(),
        }
    }

    /// 入っている型の表 (README、§4)
    fn types_table(&self) -> String {
        if let Some(i) = &self.idl {
            return i.types_table();
        }
        let n = &self.name;
        let mut s = String::from(
            "| Type | Kind | Arguments (default) | Returns | What it does |\n| --- | --- | --- | --- | --- |\n",
        );
        if self.template.has_process() {
            s.push_str(&format!(
                "| `{n}_add` | Process | `a: int` (0), `b: int` (0) | int | Returns `a + b` |\n"
            ));
        }
        if self.template.has_container() {
            s.push_str(&format!(
                "| `{n}` | Container | `initial_value: int` (0) | - | Holds one integer `value` |\n"
            ));
            s.push_str(&format!(
                "| `{n}_increment` | ContainerProcess | `step: int` (1) | int | `value += step`, returns `value` |\n"
            ));
            s.push_str(&format!(
                "| `{n}_get` | ContainerProcess (readonly) | - | int | Returns `value` |\n"
            ));
        }
        s.trim_end().to_owned()
    }

    /// 呼び出しの例 (§9)
    fn call_examples(&self) -> Vec<String> {
        if let Some(i) = &self.idl {
            return i.call_examples();
        }
        let n = &self.name;
        let mut v = Vec::new();
        if self.template.has_process() {
            v.push(format!(
                "juiz process call \"http://127.0.0.1:8000/process/{n}_add0::{n}_add\" '{{\"a\": 1, \"b\": 2}}' -p"
            ));
        }
        if self.template.has_container() {
            v.push(format!(
                "juiz container-process call \"http://127.0.0.1:8000/container_process/{n}_increment0::{n}_increment:{n}0\" '{{\"step\": 1}}' -p"
            ));
            v.push(format!(
                "juiz container-process call \"http://127.0.0.1:8000/container_process/{n}_get0::{n}_get:{n}0\" '{{}}' -p"
            ));
        }
        v
    }

    /// libjuiz_sdk を用意するコマンド (C++・Haskell、§6.3・§6.4・§9)
    fn sdk_install_command(&self, windows: bool) -> String {
        sdk_install_command(&self.juiz_sdk_dir, self.juiz_sdk_dir_given, windows)
    }

    /// 次に打つコマンド (§9)。`cd` は表示のときだけ付ける。
    /// README 用 (`with_cd` が false) では、C++ の SDK の用意を在るかどうかによらず書く
    pub fn next_steps(&self, with_cd: bool, windows: bool) -> String {
        // Haskell には .ps1 が無い (§6.4)
        let windows = windows && self.lang.has_ps1();
        let script = |s: &str| {
            if windows {
                format!("scripts\\{s}.ps1")
            } else {
                format!("scripts/{s}.sh")
            }
        };
        let mut lines: Vec<(String, &str)> = Vec::new();
        if with_cd {
            lines.push((format!("cd {}", self.cd_dir), ""));
        }
        match self.lang {
            Lang::Python => {
                if windows {
                    lines.push(("python -m venv .venv".into(), ""));
                    lines.push((".venv\\Scripts\\activate".into(), ""));
                } else {
                    lines.push(("python3 -m venv .venv".into(), ""));
                    lines.push((". .venv/bin/activate".into(), ""));
                }
                lines.push(("pip install -e .".into(), "# installs pyjuiz"));
            }
            Lang::Cpp if self.sdk_missing || !with_cd => {
                lines.push((
                    format!("# once: put libjuiz_sdk {JUIZ_VERSION} in {}", slash(&self.sdk_dir_label())),
                    "",
                ));
                lines.push((self.sdk_install_command(windows), ""));
                lines.push((
                    script("dev"),
                    "# build and put the plugin and libjuiz_sdk in app/plugins/<target>/",
                ));
            }
            Lang::Cpp => {
                lines.push((
                    script("dev"),
                    "# build and put the plugin and libjuiz_sdk in app/plugins/<target>/",
                ));
            }
            Lang::Haskell => {
                if self.hsjuiz_missing || !with_cd {
                    lines.push((
                        format!("# once: get hsjuiz {JUIZ_VERSION} (the juiz source) in {}", slash(&self.hsjuiz_dir)),
                        "",
                    ));
                    lines.push((hsjuiz_clone_command(&self.hsjuiz_dir, self.hsjuiz_given), ""));
                }
                if self.sdk_missing || !with_cd {
                    lines.push((
                        format!("# once: put libjuiz_sdk {JUIZ_VERSION} in {}", slash(&self.juiz_sdk_dir)),
                        "",
                    ));
                    lines.push((self.sdk_install_command(false), ""));
                }
                lines.push((script("dev"), "# build and put the plugin in app/plugins/<target>/"));
            }
            _ => {
                lines.push((
                    script("dev"),
                    "# build and put the plugin in app/plugins/<target>/",
                ));
            }
        }
        // --idl: 関数の本体を書いてから動かす (§6.5)。ビルドのある言語は scripts/dev.sh の前、Python は pip の後
        if let Some(i) = &self.idl {
            let at = if self.lang.has_build() { lines.len() - 1 } else { lines.len() };
            lines.insert(at, (i.write_bodies_line(&self.name), ""));
        }
        lines.push(("juiz -f app/app.conf -d".into(), "# start (keep it running)"));
        // コメントを並べる幅。コメントの無い長い行 (SDK の用意) は数えない
        let width = lines
            .iter()
            .filter(|(_, comment)| !comment.is_empty())
            .map(|(c, _)| c.len())
            .max()
            .unwrap_or(0)
            .max(30);
        let mut out = String::new();
        for (c, comment) in &lines {
            if comment.is_empty() {
                out.push_str(&format!("  {c}\n"));
            } else {
                out.push_str(&format!("  {c:<width$}  {comment}\n"));
            }
        }
        out.push_str("\n  # in another terminal\n");
        for c in self.call_examples() {
            out.push_str(&format!("  {c}\n"));
        }
        let pkg = format!("dist/{}-0.1.0.juizapp", self.name);
        out.push('\n');
        out.push_str(&format!(
            "  {:<width$}  # make {pkg}\n",
            script("pack")
        ));
        out.push_str(&format!("  juiz run {pkg}\n"));
        out
    }

    /// 置換に使う変数 (§10.2)
    pub fn vars(&self) -> Vars {
        let mut v = Vars::new();
        let steps = self.next_steps(false, false);
        // README のコードブロックに入れるので、先頭の 2 文字の字下げを外す
        let steps: String = steps
            .lines()
            .map(|l| l.strip_prefix("  ").unwrap_or(l))
            .collect::<Vec<_>>()
            .join("\n");
        v.set("name", &self.name)
            .set("Name", names::camel(&self.name))
            .set("name_kebab", names::kebab(&self.name))
            .set("juiz_version", JUIZ_VERSION)
            .set("plugin_language", self.lang.plugin_language())
            .set("plugin_path", self.lang.plugin_path())
            .set("juiz_sdk_dep", self.juiz_sdk_dep())
            .set("juiz_sdk_dir", slash(&self.juiz_sdk_dir))
            .set_opt(
                "juiz_sdk_dir_arg",
                self.juiz_sdk_dir_given.then(|| slash(&self.juiz_sdk_dir)),
            )
            .set("hsjuiz_dir", slash(&self.hsjuiz_dir))
            .set_opt("license", self.license.then(|| "MIT OR Apache-2.0".to_owned()))
            .set_opt("author", self.author.clone())
            .set("year", self.year.to_string())
            .set("types_table", self.types_table())
            .set("next_steps", steps.trim_end().to_owned())
            .set("juiz_idl_dep", self.rust_dep("juiz_idl"));
        match &self.idl {
            Some(i) => {
                v.set("idl_main", &i.main)
                    .set("idl_conf", i.conf_types())
                    .set_opt("idl_extern", i.extern_line())
                    .set_opt("idl_depends", i.depends())
                    .set("idl_hs_modules", i.hs_other_modules())
                    .set_opt("idl_hs_gen_dir", (self.lang == Lang::Haskell).then(|| "gen".to_owned()))
                    .set_opt(
                        "juiz_std_types_dep",
                        i.uses_juiz_std.then(|| self.rust_dep("juiz_std_types")),
                    );
            }
            None => {
                v.set_opt("idl_main", None)
                    .set_opt("idl_conf", None)
                    .set_opt("idl_extern", None)
                    .set_opt("idl_depends", None)
                    .set_opt("idl_hs_modules", None)
                    .set_opt("idl_hs_gen_dir", None)
                    .set_opt("juiz_std_types_dep", None);
            }
        }
        v
    }

    /// 生成するファイルの一覧を作る (§10.1 の重ね方)
    pub fn files(&self) -> Result<Vec<GeneratedFile>, PkgError> {
        // --idl では <template> の代わりに idl の層を重ねる (§6.5)
        let kind = if self.idl.is_some() { "idl" } else { self.template.dir() };
        let layers = [
            "common/_base/".to_owned(),
            format!("common/{kind}/"),
            format!("{}/_base/", self.lang.dir()),
            format!("{}/{kind}/", self.lang.dir()),
        ];
        let vars = self.vars();
        let mut out: BTreeMap<String, String> = BTreeMap::new();
        for layer in &layers {
            for (path, text) in templates() {
                let Some(rel) = path.strip_prefix(layer.as_str()) else {
                    continue;
                };
                let rel = rel.strip_suffix(".tmpl").ok_or_else(|| {
                    PkgError::new(format!("template file {path} must end with .tmpl"))
                })?;
                if !self.license && (rel == "LICENSE-MIT" || rel == "LICENSE-APACHE") {
                    continue;
                }
                let rel = render_path(rel, &vars)?;
                // 改行は LF にする (Windows の git が CRLF にして取り出したテンプレートでも、§5)
                let content = render(&text.replace("\r\n", "\n"), &vars)?;
                out.insert(rel, content);
            }
        }
        // --idl: .jidl の写し、Python の型のコード、juiz_idl の雛形から作る Component のソース
        // (テンプレートの置換は通さない)
        if let Some(i) = &self.idl {
            for (path, text) in i.files.iter().chain(i.generated.iter()) {
                out.insert(path.clone(), text.clone());
            }
            let (path, text) = i.source(&self.name)?;
            out.insert(path, text);
        }
        Ok(out
            .into_iter()
            .map(|(path, content)| GeneratedFile { path, content })
            .collect())
    }

    /// 成功したときの表示 (§9)
    pub fn summary(&self) -> String {
        format!(
            "Created {} ({}, {}) in {}  [{}]\n\nNext steps:\n{}",
            self.name,
            self.lang.dir(),
            self.kind_label(),
            self.display_dir,
            self.sdk_label(),
            self.next_steps(true, self.windows)
        )
    }
}

/// 作り先の状態 (§8)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dest {
    Missing,
    Empty,
}

fn check_dest(dir: &Path) -> Result<Dest, PkgError> {
    match std::fs::metadata(dir) {
        Err(_) => Ok(Dest::Missing),
        Ok(m) if !m.is_dir() => Err(PkgError::new(format!(
            "{} already exists and is not a directory",
            dir.display()
        ))),
        Ok(_) => {
            let mut entries = std::fs::read_dir(dir)
                .map_err(|e| PkgError::new(format!("cannot read {}: {e}", dir.display())))?;
            if entries.next().is_some() {
                Err(PkgError::new(format!(
                    "{} already exists and is not empty (juiz pkg create does not overwrite files)",
                    dir.display()
                )))
            } else {
                Ok(Dest::Empty)
            }
        }
    }
}

fn write_one(dir: &Path, f: &GeneratedFile) -> std::io::Result<()> {
    let path = dir.join(&f.path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, f.content.as_bytes())?;
    #[cfg(unix)]
    if f.executable() {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

/// 作ったものを消して、作る前の状態に戻す (§8)
fn rollback(dir: &Path, dest: Dest) {
    match dest {
        Dest::Missing => {
            let _ = std::fs::remove_dir_all(dir);
        }
        Dest::Empty => {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for e in entries.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        let _ = std::fs::remove_dir_all(&p);
                    } else {
                        let _ = std::fs::remove_file(&p);
                    }
                }
            }
        }
    }
}

/// ファイルを書く。`fail_at` 番目で失敗したことにする (テスト用)
pub(crate) fn write_files(
    dir: &Path,
    files: &[GeneratedFile],
    fail_at: Option<usize>,
) -> Result<(), PkgError> {
    let dest = check_dest(dir)?;
    let result = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        for (i, f) in files.iter().enumerate() {
            if fail_at == Some(i) {
                return Err(std::io::Error::other("injected failure"));
            }
            write_one(dir, f)?;
        }
        Ok(())
    })();
    result.map_err(|e| {
        rollback(dir, dest);
        PkgError::new(format!("failed to write the skeleton in {}: {e}", dir.display()))
    })
}

/// 雛形を作る
pub fn create(args: &CreateArgs) -> Result<Created, PkgError> {
    let (plan, warnings) = Plan::from_args(args)?;
    let dir = match &args.output {
        Some(o) => expand_tilde(o),
        None => PathBuf::from(&plan.name),
    };
    // ファイルを書く前に、一覧と作り先の検査を済ませる
    let files = plan.files()?;
    check_dest(&dir)?;
    write_files(&dir, &files, None)?;
    Ok(Created {
        dir,
        files,
        warnings,
        summary: plan.summary(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        args: crate::cli::PkgArgs,
    }

    fn parse(args: &[&str]) -> CreateArgs {
        let mut v = vec!["juiz-pkg"];
        v.extend_from_slice(args);
        match Cli::try_parse_from(v).unwrap().args.command {
            crate::cli::PkgCommand::Create(c) => c,
            _ => panic!(),
        }
    }

    fn plan(args: &[&str]) -> Plan {
        Plan::from_args_with(&parse(args), || Some("Tester".to_owned()), false)
            .unwrap()
            .0
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("juiz_pkg_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn year() {
        assert_eq!(year_from_days(0), 1970);
        assert_eq!(year_from_days(20_401), 2025); // 2025-11-09
        assert_eq!(year_from_days(20_454), 2026); // 2026-01-01
        assert_eq!(year_from_days(20_453), 2025); // 2025-12-31
    }

    #[test]
    fn options() {
        let c = parse(&["create", "my_robot", "-b", "RUST"]);
        assert_eq!(c.lang, Lang::Rust);
        assert_eq!(c.template, Template::Component);
        let c = parse(&["create", "my_robot", "--lang", "c++", "-t", "process"]);
        assert_eq!(c.lang, Lang::Cpp);
        assert_eq!(c.template, Template::Process);
        let c = parse(&["create", "my_robot", "-b", "Python", "--template", "Container"]);
        assert_eq!(c.lang, Lang::Python);
        assert_eq!(c.template, Template::Container);
        let e = Cli::try_parse_from(["juiz-pkg", "create", "x", "-b", "go"]).err().unwrap();
        let e = e.to_string();
        assert!(e.contains("rust") && e.contains("python") && e.contains("haskell"), "{e}");
    }

    #[test]
    fn haskell_component_files() {
        let sdk = temp("hs_sdk_missing");
        let hs = temp("hs_hsjuiz_missing").join("juiz-x").join("bindings").join("hsjuiz");
        let args = parse(&[
            "create", "my_robot", "-b", "haskell",
            "--juiz-sdk-dir", sdk.to_str().unwrap(),
            "--hsjuiz-dir", hs.to_str().unwrap(),
        ]);
        let (p, w) = Plan::from_args_with(&args, || None, false).unwrap();
        assert_eq!(
            paths(&p),
            [
                ".gitignore",
                "README.md",
                "app/app.conf",
                "app/juizapp.yaml",
                "cabal.project",
                "my-robot.cabal",
                "scripts/bundle-plugin-deps.sh",
                "scripts/dev.sh",
                "scripts/pack.sh",
                "src/MyRobot.hs",
            ]
        );
        // hsjuiz も libjuiz_sdk も無いので、用意するコマンドを警告に出す
        assert_eq!(w.len(), 2, "{w:?}");
        assert!(w[0].contains("git clone --depth 1 --branch v") && w[0].contains("/juiz-x"), "{w:?}");
        assert!(w[1].contains("install-juiz-sdk.sh | sh -s -- "), "{w:?}");
        let files = p.files().unwrap();
        let get = |n: &str| files.iter().find(|f| f.path == n).unwrap().content.clone();
        // cabal.project には絶対パスを埋める (§6.4)
        let project = get("cabal.project");
        assert!(project.contains(&format!("packages: . {}\n", slash(&hs))), "{project}");
        assert!(project.contains(&format!("  extra-lib-dirs: {}/lib\n", slash(&sdk))), "{project}");
        assert!(project.contains(&format!("  ghc-options: -optl-Wl,-rpath,{}/lib\n", slash(&sdk))), "{project}");
        let cabal = get("my-robot.cabal");
        assert!(cabal.contains("name:               my-robot\n"), "{cabal}");
        assert!(cabal.contains("foreign-library my_robot\n"), "{cabal}");
        assert!(cabal.contains("other-modules:    MyRobot\n"), "{cabal}");
        let conf = get("app/app.conf");
        assert!(conf.contains("\"language\": \"haskell\""), "{conf}");
        assert!(conf.contains("\"path\": \"plugins/{target}\""), "{conf}");
        assert!(get("app/juizapp.yaml").contains(&format!("juiz: \"={JUIZ_VERSION}\"\n")));
        let hs_src = get("src/MyRobot.hs");
        assert!(hs_src.contains("module MyRobot () where"), "{hs_src}");
        assert!(hs_src.contains("newtype MyRobotState = MyRobotState (IORef Int64)"), "{hs_src}");
        assert!(hs_src.contains("& readonly True"), "{hs_src}");
        // 次の手順に用意するコマンドを出す
        let s = p.summary();
        assert!(s.contains("git clone --depth 1"), "{s}");
        assert!(s.contains("install-juiz-sdk.sh"), "{s}");
        assert!(s.contains("scripts/dev.sh"), "{s}");
    }

    #[test]
    fn haskell_found_and_windows() {
        let root = temp("hs_found");
        let sdk = root.join("sdk");
        let cfg = sdk.join("lib").join("cmake").join("juiz_sdk");
        std::fs::create_dir_all(&cfg).unwrap();
        std::fs::write(cfg.join("juiz_sdkConfig.cmake"), "").unwrap();
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let args = parse(&[
            "create", "my_robot", "-b", "haskell",
            "--juiz-sdk-dir", sdk.to_str().unwrap(),
            "--juiz-source", repo.to_str().unwrap(),
        ]);
        // --juiz-source の bindings/hsjuiz を使い、両方あれば警告も用意の手順も出さない
        let (p, w) = Plan::from_args_with(&args, || None, false).unwrap();
        assert!(w.is_empty(), "{w:?}");
        assert!(p.hsjuiz_dir.ends_with("bindings/hsjuiz"), "{}", p.hsjuiz_dir.display());
        let s = p.summary();
        assert!(!s.contains("git clone") && !s.contains("install-juiz-sdk"), "{s}");
        // README には在っても書く
        let files = p.files().unwrap();
        let readme = &files.iter().find(|f| f.path == "README.md").unwrap().content;
        assert!(readme.contains("git clone --depth 1") && readme.contains("install-juiz-sdk.sh"), "{readme}");
        // Windows では警告を出し、.ps1 は置かず .sh の形で表示する
        let (p, w) = Plan::from_args_with(&args, || None, true).unwrap();
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("#137"), "{w:?}");
        assert!(!paths(&p).iter().any(|f| f.ends_with(".ps1")));
        let s = p.summary();
        assert!(s.contains("scripts/dev.sh") && !s.contains(".ps1"), "{s}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn default_hsjuiz_clone_command() {
        let home = home_dir().unwrap();
        let d = home.join(format!("juiz-{JUIZ_VERSION}")).join("bindings").join("hsjuiz");
        assert_eq!(
            hsjuiz_clone_command(&d, false),
            format!("git clone --depth 1 --branch v{JUIZ_VERSION} https://github.com/OpenJUIZ/juiz.git ~/juiz-{JUIZ_VERSION}")
        );
        let c = hsjuiz_clone_command(Path::new("/opt/hs"), true);
        assert!(c.ends_with("juiz.git <dir>   # then use <dir>/bindings/hsjuiz"), "{c}");
    }

    /// 雛形の scripts/bundle-plugin-deps.sh は juiz の scripts/bundle-plugin-deps.sh の写し (§6.4、§11)。
    /// crates.io のパッケージには juiz の scripts/ が無いので、在るときだけ比べる
    #[test]
    fn bundle_script_is_a_copy() {
        let orig = Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/bundle-plugin-deps.sh");
        let Ok(orig) = std::fs::read_to_string(&orig) else {
            return;
        };
        let (_, copy) = templates()
            .iter()
            .find(|(p, _)| *p == "haskell/_base/scripts/bundle-plugin-deps.sh.tmpl")
            .unwrap();
        assert_eq!(
            copy.replace("\r\n", "\n"),
            orig.replace("\r\n", "\n"),
            "copy scripts/bundle-plugin-deps.sh to juiz_pkg/templates/haskell/_base/scripts/bundle-plugin-deps.sh.tmpl"
        );
        let p = plan(&["create", "my_robot", "-b", "haskell"]);
        let files = p.files().unwrap();
        let out = &files.iter().find(|f| f.path == "scripts/bundle-plugin-deps.sh").unwrap().content;
        assert_eq!(out, &orig.replace("\r\n", "\n"));
    }

    #[test]
    fn warns_on_options_for_other_languages() {
        let (_, w) = Plan::from_args_with(
            &parse(&["create", "x", "-b", "rust", "--hsjuiz-dir", "/a", "--juiz-sdk-dir", "/b", "--author", "c"]),
            || None,
            false,
        )
        .unwrap();
        assert_eq!(w.len(), 3, "{w:?}");
    }

    #[test]
    fn license_needs_author() {
        let args = parse(&["create", "x", "-b", "rust", "--license", "mit-apache"]);
        assert!(Plan::from_args_with(&args, || None, false).is_err());
        let (p, _) = Plan::from_args_with(&args, || Some("Git User".into()), false).unwrap();
        assert_eq!(p.author.as_deref(), Some("Git User"));
        let args = parse(&["create", "x", "-b", "rust", "--license", "mit-apache", "--author", "Me"]);
        let (p, _) = Plan::from_args_with(&args, || Some("Git User".into()), false).unwrap();
        assert_eq!(p.author.as_deref(), Some("Me"));
    }

    fn paths(p: &Plan) -> Vec<String> {
        p.files().unwrap().into_iter().map(|f| f.path).collect()
    }

    #[test]
    fn rust_component_files() {
        let p = plan(&["create", "my_robot", "-b", "rust"]);
        assert_eq!(
            paths(&p),
            [
                ".gitignore",
                "Cargo.toml",
                "README.md",
                "app/app.conf",
                "app/juizapp.yaml",
                "scripts/dev.ps1",
                "scripts/dev.sh",
                "scripts/pack.ps1",
                "scripts/pack.sh",
                "src/lib.rs",
            ]
        );
    }

    #[test]
    fn cpp_component_files() {
        let p = plan(&["create", "my_robot", "-b", "cpp"]);
        assert_eq!(
            paths(&p),
            [
                ".gitignore",
                "CMakeLists.txt",
                "README.md",
                "app/app.conf",
                "app/juizapp.yaml",
                "scripts/dev.ps1",
                "scripts/dev.sh",
                "scripts/pack.ps1",
                "scripts/pack.sh",
                "src/my_robot.cpp",
            ]
        );
        let files = p.files().unwrap();
        let get = |n: &str| files.iter().find(|f| f.path == n).unwrap().content.clone();
        let cmake = get("CMakeLists.txt");
        assert!(cmake.contains(&format!("set(JUIZ_VERSION \"{JUIZ_VERSION}\")")), "{cmake}");
        assert!(cmake.contains("find_package(juiz_sdk ${JUIZ_VERSION} EXACT"), "{cmake}");
        assert!(cmake.contains("juiz_sdk_bundle(my_robot)"), "{cmake}");
        // --juiz-sdk-dir を付けなければ、CMakeLists.txt に絶対パスを書かない
        assert!(!cmake.contains("--juiz-sdk-dir"), "{cmake}");
        let conf = get("app/app.conf");
        assert!(conf.contains("\"language\": \"c++\""), "{conf}");
        assert!(conf.contains("\"path\": \"plugins/{target}\""), "{conf}");
        // libjuiz_sdk はホストと同じ版でないと読み込めないので、.juizapp も同じ版の juiz を求める
        assert!(get("app/juizapp.yaml").contains(&format!("juiz: \"={JUIZ_VERSION}\"\n")));
        let cpp = get("src/my_robot.cpp");
        assert!(cpp.contains("class MyRobot {"), "{cpp}");
        assert!(cpp.contains("std::optional<int64_t> my_robot_get(const MyRobot* container)"), "{cpp}");
        assert!(cpp.contains("DEFINE_COMPONENT_MANIFEST_ENTRY_POINT(my_robot_component_manifest)"), "{cpp}");
        let readme = get("README.md");
        assert!(readme.contains("install-juiz-sdk.sh | sh -s -- "), "{readme}");
    }

    #[test]
    fn cpp_juiz_sdk_dir() {
        // 無い場所 (空白を含む)。絶対パスは OS で形が違うので一時ディレクトリの下に作る
        let missing = temp("cpp_sdk_missing").join("juiz sdk");
        let missing_s = slash(&missing);
        let args = parse(&["create", "my_robot", "-b", "cpp", "--juiz-sdk-dir", missing.to_str().unwrap()]);
        let (p, w) = Plan::from_args_with(&args, || None, false).unwrap();
        assert!(w.is_empty(), "{w:?}");
        assert!(p.sdk_missing);
        let files = p.files().unwrap();
        let cmake = &files.iter().find(|f| f.path == "CMakeLists.txt").unwrap().content;
        assert!(
            cmake.contains(&format!(
                "set(JUIZ_SDK_DEFAULT_DIR \"{missing_s}\")  # given with juiz pkg create --juiz-sdk-dir\n"
            )),
            "{cmake}"
        );
        // SDK が無ければ、次の手順に用意するコマンドを出す (指定した場所に)
        let s = p.summary();
        assert!(
            s.contains(&format!("install-juiz-sdk.sh | sh -s -- {JUIZ_VERSION} \"{missing_s}\"\n")),
            "{s}"
        );
        // 在れば出さない
        let sdk = temp("cpp_sdk");
        let cfg = sdk.join("lib").join("cmake").join("juiz_sdk");
        std::fs::create_dir_all(&cfg).unwrap();
        std::fs::write(cfg.join("juiz_sdkConfig.cmake"), "").unwrap();
        let args = parse(&["create", "my_robot", "-b", "cpp", "--juiz-sdk-dir", sdk.to_str().unwrap()]);
        let (p, _) = Plan::from_args_with(&args, || None, true).unwrap();
        assert!(!p.sdk_missing);
        let s = p.summary();
        assert!(!s.contains("install-juiz-sdk"), "{s}");
        assert!(s.contains("scripts\\dev.ps1"), "{s}");
        // README には在っても書く
        let files = p.files().unwrap();
        let readme = &files.iter().find(|f| f.path == "README.md").unwrap().content;
        assert!(readme.contains("install-juiz-sdk.sh"), "{readme}");
        let _ = std::fs::remove_dir_all(&sdk);
    }

    #[test]
    fn python_files_with_license() {
        let p = plan(&["create", "my_robot", "-b", "python", "--license", "mit-apache"]);
        assert_eq!(
            paths(&p),
            [
                ".gitignore",
                "LICENSE-APACHE",
                "LICENSE-MIT",
                "README.md",
                "app/app.conf",
                "app/juizapp.yaml",
                "app/python/my_robot.py",
                "pyproject.toml",
                "scripts/pack.ps1",
                "scripts/pack.sh",
            ]
        );
        let files = p.files().unwrap();
        let get = |n: &str| files.iter().find(|f| f.path == n).unwrap().content.clone();
        assert!(get("LICENSE-MIT").contains(&format!("Copyright (c) {} Tester", p.year)));
        assert!(get("app/juizapp.yaml").contains("license: MIT OR Apache-2.0\n"));
        assert!(get("app/juizapp.yaml").contains("python:\n  version: \">=3.10\"\n"));
        assert!(get("pyproject.toml").contains("license = \"MIT OR Apache-2.0\""));
        assert!(get("pyproject.toml").contains(&format!("\"pyjuiz=={JUIZ_VERSION}\"")));
    }

    #[test]
    fn no_license_lines_without_license() {
        for lang in ["rust", "python", "cpp", "haskell"] {
            let p = plan(&["create", "my_robot", "-b", lang]);
            for f in p.files().unwrap() {
                assert!(!f.path.starts_with("LICENSE"), "{}", f.path);
                for l in f.content.lines() {
                    let l = l.trim_start().to_ascii_lowercase();
                    assert!(!l.starts_with("license"), "{}:\n{}", f.path, f.content);
                }
            }
        }
    }

    /// 全言語・全種類で、置換が残らない・conf の型と表示が合う (§11)
    #[test]
    fn all_combinations_render() {
        for lang in ["rust", "python", "cpp", "haskell"] {
            for t in ["component", "process", "container"] {
                for lic in ["none", "mit-apache"] {
                    let p = plan(&["create", "my_robot", "-b", lang, "-t", t, "--license", lic]);
                    let files = p.files().unwrap();
                    for f in &files {
                        assert!(!f.content.contains("{{"), "{lang} {t} {}", f.path);
                        assert!(!f.path.contains("{{"), "{lang} {t} {}", f.path);
                        assert!(!f.content.contains('\r'), "{lang} {t} {}", f.path);
                    }
                    let conf = &files.iter().find(|f| f.path == "app/app.conf").unwrap().content;
                    assert_eq!(conf.contains("my_robot_add0"), p.template.has_process(), "{conf}");
                    assert_eq!(conf.contains("my_robot_get0"), p.template.has_container(), "{conf}");
                    assert_eq!(conf.contains("main_loop_ec"), p.template.has_container(), "{conf}");
                    let s = p.summary();
                    assert_eq!(s.contains("my_robot_add0"), p.template.has_process(), "{s}");
                    assert_eq!(s.contains("my_robot_increment0"), p.template.has_container(), "{s}");
                }
            }
        }
    }

    /// --idl (§6.5): .jidl の写し・build.rs・型付きの src/lib.rs を作り、-t の型は入れない
    #[test]
    fn rust_idl_files() {
        let d = temp("idl_files");
        let idl = crate::idl::write_jidl(
            &d,
            "robot.jidl",
            "namespace robot; import idl \"juiz_std/geometry.jidl\";\n\
             process double turn(juiz_std.geometry.Pose2D pose, double speed = 1.0);\n\
             container wheel { container_process long spin(long n = 1); }\n",
        );
        for lic in ["none", "mit-apache"] {
            let p = plan(&["create", "my_robot", "-b", "rust", "--idl", idl.to_str().unwrap(), "--license", lic]);
            let files = p.files().unwrap();
            let mut expected = vec![
                ".gitignore",
                "Cargo.toml",
                "README.md",
                "app/app.conf",
                "app/juizapp.yaml",
                "build.rs",
                "idl/robot.jidl",
                "scripts/dev.ps1",
                "scripts/dev.sh",
                "scripts/pack.ps1",
                "scripts/pack.sh",
                "src/lib.rs",
            ];
            if lic == "mit-apache" {
                expected.extend(["LICENSE-APACHE", "LICENSE-MIT"]);
                expected.sort();
            }
            assert_eq!(paths(&p), expected);
            let get = |path: &str| files.iter().find(|f| f.path == path).unwrap().content.clone();
            for f in &files {
                if !f.path.starts_with("idl/") && f.path != "src/lib.rs" {
                    assert!(!f.content.contains("{{"), "{}", f.path);
                }
            }
            let conf = get("app/app.conf");
            assert!(conf.contains("\"name\": \"turn0\"") && conf.contains("\"name\": \"wheel0\"") && conf.contains("\"name\": \"spin0\""), "{conf}");
            assert!(!conf.contains("my_robot_add") && !conf.contains("main_loop_ec"), "{conf}");
            let cargo = get("Cargo.toml");
            assert!(cargo.contains(&format!("juiz_std_types = \"={JUIZ_VERSION}\"")), "{cargo}");
            assert!(cargo.contains(&format!("[build-dependencies]\n# build.rs generates the Rust types of the .jidl. Same version as juiz_sdk\njuiz_idl = \"={JUIZ_VERSION}\"")), "{cargo}");
            assert_eq!(cargo.contains("license = "), lic == "mit-apache", "{cargo}");
            let build = get("build.rs");
            assert!(build.contains(".extern_path(\"juiz_std\", \"::juiz_std_types::juiz_std\")\n        .compile(&[\"idl/robot.jidl\"])"), "{build}");
            assert!(get("src/lib.rs").contains("component_name = \"my_robot\""));
            assert!(get("README.md").contains("| `turn` | Process | `pose: juiz_std.geometry.Pose2D`, `speed: double` (1.0) | `double` | - |"));
            let s = p.summary();
            assert!(s.starts_with("Created my_robot (rust, from idl/robot.jidl) in ./my_robot"), "{s}");
            assert!(s.contains("todo!()") && s.contains("/process/turn0::turn") && s.contains("/container_process/spin0::spin:wheel0"), "{s}");
        }
        // juiz_std を使わなければ juiz_std_types も extern も無い
        let idl2 = crate::idl::write_jidl(&d, "plain.jidl", "namespace plain; process long f(long x = 0);");
        let p = plan(&["create", "my_robot", "-b", "rust", "--idl", idl2.to_str().unwrap()]);
        let files = p.files().unwrap();
        let get = |path: &str| files.iter().find(|f| f.path == path).unwrap().content.clone();
        assert!(!get("Cargo.toml").contains("juiz_std_types"));
        assert_eq!(
            get("build.rs").lines().filter(|l| l.contains("extern_path")).count(),
            0
        );
        // --juiz-source なら juiz_idl・juiz_std_types もソースを使う
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let p = plan(&["create", "my_robot", "-b", "rust", "--idl", idl.to_str().unwrap(), "--juiz-source", repo.to_str().unwrap()]);
        let cargo = p.files().unwrap().into_iter().find(|f| f.path == "Cargo.toml").unwrap().content;
        assert!(cargo.contains("/juiz_idl' }") && cargo.contains("/juiz_std_types' }"), "{cargo}");
        // -t とは一緒に使えない
        let e = Cli::try_parse_from(["juiz-pkg", "create", "x", "-b", "rust", "-t", "process", "--idl", "a.jidl"]);
        assert!(e.is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// --idl の Python・C++・Haskell (§6.5、issue #468): 言語ごとのファイルと、その言語で書けない宣言のエラー
    #[test]
    fn other_language_idl_files() {
        let d = temp("idl_langs");
        crate::idl::write_jidl(
            &d.join("common"),
            "goal.jidl",
            "namespace robot.common; import idl \"juiz_std/geometry.jidl\";\n\
             enum Mode { FAST, SLOW } = SLOW;\n\
             record Goal { juiz_std.geometry.Pose2D pose; double tolerance = 0.1; Mode mode; }\n",
        );
        let idl = crate::idl::write_jidl(
            &d,
            "robot.jidl",
            "namespace robot; import idl \"juiz_std/geometry.jidl\"; import idl \"common/goal.jidl\";\n\
             /** Turns */\n\
             process double turn(robot.common.Goal goal, juiz_std.geometry.Pose2D pose, double speed = 1.0);\n\
             container wheel { container_process long spin(long n = 1, robot.common.Mode mode = \"FAST\"); container_process void stop(); }\n",
        );
        let idl = idl.to_str().unwrap();
        let common = |files: &[GeneratedFile], lang: &str, source: &str| {
            let get = |path: &str| {
                files
                    .iter()
                    .find(|f| f.path == path)
                    .unwrap_or_else(|| panic!("{lang}: {path} is not made"))
                    .content
                    .clone()
            };
            for f in files {
                if !f.path.starts_with("idl/") && f.path != source && !f.path.starts_with("app/python/robot") {
                    assert!(!f.content.contains("{{"), "{lang} {}", f.path);
                }
            }
            assert!(get("idl/robot.jidl").contains("process double turn"));
            assert!(get("idl/common/goal.jidl").contains("record Goal"));
            let conf = get("app/app.conf");
            assert!(conf.contains("\"name\": \"turn0\"") && conf.contains("\"name\": \"spin0\"") && conf.contains("\"name\": \"stop0\""), "{lang} {conf}");
            assert!(!conf.contains("my_robot_add"), "{conf}");
            let readme = get("README.md");
            assert!(readme.contains("juiz pkg create --idl") && readme.contains("| `turn` | Process |"), "{lang} {readme}");
            get(source)
        };

        // Python: 型のパッケージは app/python/ に作る (juiz_std は pyjuiz のもの)
        let p = plan(&["create", "my_robot", "-b", "python", "--idl", idl]);
        let files = p.files().unwrap();
        let src = common(&files, "python", "app/python/my_robot.py");
        assert!(src.contains("from juiz import juiz_container, juiz_container_process, juiz_process\n"), "{src}");
        assert!(src.contains("from juiz_std.geometry import Pose2D\nfrom robot.common import Goal, Mode\n"), "{src}");
        assert!(src.contains("@juiz_process\ndef turn(goal: Goal, pose: Pose2D, speed: float = 1.0) -> float:\n    \"\"\"Turns\"\"\"\n    raise NotImplementedError(\"write the body of turn\")"), "{src}");
        assert!(src.contains("def spin(container: WheelState, n: int = 1, mode: Mode = Mode.from_value(\"FAST\")) -> int:"), "{src}");
        assert!(src.contains("def stop(container: WheelState) -> None:"), "{src}");
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).filter(|p| p.starts_with("app/python/")).collect();
        assert_eq!(paths, ["app/python/my_robot.py", "app/python/robot/__init__.py", "app/python/robot/common/__init__.py"]);
        let s = p.summary();
        assert!(s.contains("pip install -e .                # installs pyjuiz\n  # write the bodies of the functions (raise NotImplementedError) in app/python/my_robot.py, then\n"), "{s}");
        assert!(get_readme(&files).contains("juiz idl -g python --extern juiz_std -o app/python idl/robot.jidl"));

        // C++: CMake が juiz_idl.h を作る
        let p = plan(&["create", "my_robot", "-b", "cpp", "--idl", idl]);
        let files = p.files().unwrap();
        let src = common(&files, "cpp", "src/my_robot.cpp");
        assert!(src.contains("#include \"juiz_idl.h\""), "{src}");
        assert!(src.contains("std::optional<double> turn(const ::robot::common::Goal& goal, const ::juiz_std::geometry::Pose2D& pose, double speed) {"), "{src}");
        assert!(src.contains("auto my_robot_component_manifest() {\n    return ComponentManifest(\"my_robot\")"), "{src}");
        let cmake = files.iter().find(|f| f.path == "CMakeLists.txt").unwrap().content.clone();
        assert!(cmake.contains("juiz_generate_types(my_robot\n  IDL idl/robot.jidl\n  DEPENDS idl/common/goal.jidl\n)"), "{cmake}");
        let s = p.summary();
        assert!(s.contains("# write the bodies of the functions (throw std::runtime_error) in src/my_robot.cpp, then\n  scripts/dev.sh"), "{s}");

        // Haskell: scripts/dev.sh が gen/ に型のモジュールを作り、.cabal の other-modules に書く
        let p = plan(&["create", "my_robot", "-b", "haskell", "--idl", idl]);
        let files = p.files().unwrap();
        let src = common(&files, "haskell", "src/MyRobot.hs");
        assert!(src.contains("module MyRobot () where"), "{src}");
        assert!(src.contains("turn_process :: Robot.Common.Goal -> Juiz_std.Geometry.Pose2D -> Double -> Double\n"), "{src}");
        assert!(src.contains("juizComponent 'my_robot_component\n  [ componentProcess 'turn_process 'turn_manifest\n"), "{src}");
        let cabal = files.iter().find(|f| f.path == "my-robot.cabal").unwrap().content.clone();
        assert!(cabal.contains("hs-source-dirs:   src gen\n    other-modules:    MyRobot\n                    , Juiz_std.Common\n                    , Juiz_std.Geometry\n                    , Robot.Common\n"), "{cabal}");
        assert!(cabal.contains(", containers"), "{cabal}");
        let dev = files.iter().find(|f| f.path == "scripts/dev.sh").unwrap().content.clone();
        assert!(dev.contains("juiz idl -g haskell -o gen idl/robot.jidl"), "{dev}");
        assert!(files.iter().find(|f| f.path == ".gitignore").unwrap().content.contains("/gen/\n"));
        // --idl でない雛形には gen/ の行が無い
        let files = plan(&["create", "my_robot", "-b", "haskell"]).files().unwrap();
        assert!(!files.iter().find(|f| f.path == "scripts/dev.sh").unwrap().content.contains("juiz idl"));
        assert!(!files.iter().find(|f| f.path == ".gitignore").unwrap().content.contains("/gen/"));

        // その言語で書けない宣言 (ファイルを書く前にエラー)
        let bad = crate::idl::write_jidl(&d, "bytes.jidl", "namespace b; process long f(bytes data);");
        for lang in ["python", "cpp"] {
            let args = parse(&["create", "my_robot", "-b", lang, "--idl", bad.to_str().unwrap()]);
            let e = Plan::from_args_with(&args, || None, false).unwrap_err().to_string();
            assert!(e.contains("f argument data: bytes is not supported"), "{lang} {e}");
        }
        let args = parse(&["create", "my_robot", "-b", "haskell", "--idl", bad.to_str().unwrap()]);
        assert!(Plan::from_args_with(&args, || None, false).is_ok());
        // Haskell: 型のモジュールが Component のモジュールと同じ名前
        let same = crate::idl::write_jidl(&d, "same.jidl", "namespace myrobot; record R { long x; } process R f();");
        let args = parse(&["create", "myrobot", "-b", "haskell", "--idl", same.to_str().unwrap()]);
        let e = Plan::from_args_with(&args, || None, false).unwrap_err().to_string();
        assert!(e.contains("Myrobot"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    fn get_readme(files: &[GeneratedFile]) -> String {
        files.iter().find(|f| f.path == "README.md").unwrap().content.clone()
    }

    /// テンプレートに使っている変数がすべて表にある (§10.2)
    #[test]
    fn templates_use_known_variables() {
        for (path, text) in templates() {
            assert!(path.ends_with(".tmpl"), "{path}");
            for line in text.lines().chain(std::iter::once(*path)) {
                for v in crate::render::variables_in(line).unwrap() {
                    assert!(crate::render::VARIABLES.contains(&v), "{path}: {v}");
                }
            }
        }
    }

    #[test]
    fn summary_for_windows() {
        let args = parse(&["create", "my_robot", "-b", "rust"]);
        let (p, _) = Plan::from_args_with(&args, || None, true).unwrap();
        let s = p.summary();
        assert!(s.contains("scripts\\dev.ps1"), "{s}");
        assert!(s.contains("scripts\\pack.ps1"), "{s}");
        let (p, _) = Plan::from_args_with(&args, || None, false).unwrap();
        let s = p.summary();
        assert!(s.starts_with(&format!("Created my_robot (rust, component) in ./my_robot  [juiz_sdk ={JUIZ_VERSION}]")), "{s}");
        assert!(s.contains("\n  cd my_robot\n"), "{s}");
        assert!(s.contains("scripts/dev.sh"), "{s}");
    }

    #[test]
    fn juiz_source_uses_path_dependency() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let args = parse(&["create", "my_robot", "-b", "rust", "--juiz-source", repo.to_str().unwrap()]);
        let (p, _) = Plan::from_args_with(&args, || None, false).unwrap();
        let files = p.files().unwrap();
        let cargo = &files.iter().find(|f| f.path == "Cargo.toml").unwrap().content;
        assert!(cargo.contains("juiz_sdk = { path = '"), "{cargo}");
        assert!(!cargo.contains('\\'), "{cargo}");
        assert!(cargo.contains("/juiz_sdk' }"), "{cargo}");
        assert!(p.summary().contains("juiz sources at"));
        let bad = parse(&["create", "my_robot", "-b", "rust", "--juiz-source", "/nonexistent"]);
        assert!(Plan::from_args_with(&bad, || None, false).is_err());
    }

    fn sample_files() -> Vec<GeneratedFile> {
        plan(&["create", "my_robot", "-b", "rust"]).files().unwrap()
    }

    #[test]
    fn dest_rules() {
        let files = sample_files();
        // 無ければ親ごと作る
        let d = temp("dest_missing").join("a").join("b");
        write_files(&d, &files, None).unwrap();
        assert!(d.join("src/lib.rs").is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let m = std::fs::metadata(d.join("scripts/dev.sh")).unwrap().permissions().mode();
            assert_eq!(m & 0o777, 0o755);
        }
        // 空でないディレクトリはエラー (書き換えない)
        let before = std::fs::read_to_string(d.join("Cargo.toml")).unwrap();
        assert!(write_files(&d, &files, None).is_err());
        assert_eq!(std::fs::read_to_string(d.join("Cargo.toml")).unwrap(), before);
        // 隠しファイルだけでも空でない
        let h = temp("dest_hidden");
        std::fs::create_dir_all(h.join(".git")).unwrap();
        assert!(write_files(&h, &files, None).is_err());
        // ファイルはエラー
        let f = temp("dest_file");
        std::fs::write(&f, "x").unwrap();
        assert!(write_files(&f, &files, None).is_err());
        // 空のディレクトリには作る
        let e = temp("dest_empty");
        std::fs::create_dir_all(&e).unwrap();
        write_files(&e, &files, None).unwrap();
        assert!(e.join("app/app.conf").is_file());
        for p in [temp("dest_missing"), h, f, e] {
            let _ = std::fs::remove_dir_all(&p);
            let _ = std::fs::remove_file(&p);
        }
    }

    #[test]
    fn rollback_on_failure() {
        let files = sample_files();
        // 自分で作ったディレクトリは作り先ごと消す
        let d = temp("rollback_missing");
        assert!(write_files(&d, &files, Some(3)).is_err());
        assert!(!d.exists());
        // 空のディレクトリだったなら中身だけ消す
        let e = temp("rollback_empty");
        std::fs::create_dir_all(&e).unwrap();
        assert!(write_files(&e, &files, Some(5)).is_err());
        assert!(e.is_dir());
        assert_eq!(std::fs::read_dir(&e).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&e);
    }
}
