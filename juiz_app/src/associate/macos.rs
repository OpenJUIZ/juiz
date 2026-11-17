//! macOS の関連付け(docs/spec/app-package.md §17.4、#396)
//!
//! macOS はダブルクリックしたファイルを argv ではなく Apple Event で渡すので、CLI の juiz を
//! そのまま関連付けられない。`~/Applications/JUIZ Launcher.app` を `osacompile` で AppleScript の
//! アプレットとして作り、`on open` で Terminal に `"<juiz>" run --launched <パス>` を打たせる。
//!
//! 1. `osacompile -o "JUIZ Launcher.app" <スクリプト>`
//! 2. `plutil -replace` で Info.plist に識別子・文書の種類(拡張子 juizapp)・型の宣言を書く
//! 3. `codesign --force --sign -` で署名し直す(Info.plist を書き換えると元の署名が合わなくなる)
//! 4. `lsregister -f` で Launch Services に登録する
//!
//! アプリの中の `Contents/Resources/juiz-launcher.txt` に、どの juiz を指しているかを書いておく。
//! これがあるものだけを juiz が作ったアプリとして書き換え・削除する。

use super::tools::{warning, ToolOutcome, Tools};
use super::Associated;
use juiz_core::anyhow::{anyhow, bail, Result};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub(crate) const APP_NAME: &str = "JUIZ Launcher.app";
pub(crate) const BUNDLE_ID: &str = "io.github.openjuiz.launcher";
pub(crate) const UTI: &str = "io.github.openjuiz.juizapp";
/// juiz が作ったアプリの印と、指している juiz
pub(crate) const MARKER: &str = "Contents/Resources/juiz-launcher.txt";
/// アプリの作り方を変えたら上げる(同じ juiz でも作り直す)
const LAYOUT: u32 = 1;

pub(crate) const LSREGISTER: &str = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";

/// `~/Applications`
pub(crate) fn apps_dir_from_env() -> Result<PathBuf> {
    home::home_dir()
        .map(|h| h.join("Applications"))
        .ok_or_else(|| anyhow!("cannot find the home directory"))
}

/// AppleScript の文字列リテラル
pub(crate) fn applescript_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// アプレットのスクリプト
pub(crate) fn script(exe: &str) -> String {
    let exe = applescript_string(exe);
    format!(
        r#"-- Written by `juiz associate`; removed by `juiz associate --remove`.
on open theFiles
	repeat with f in theFiles
		set p to POSIX path of f
		tell application "Terminal"
			activate
			do script (quoted form of {exe}) & " run --launched " & (quoted form of p)
		end tell
	end repeat
end open

on run
	display dialog "Double-click a .juizapp file to run it with juiz." & return & return & "juiz: " & {exe} buttons {{"OK"}} default button "OK" with title "JUIZ Launcher"
end run
"#
    )
}

/// 印の中身
pub(crate) fn marker(exe: &str) -> String {
    format!("executable={exe}\nlayout={LAYOUT}\n")
}

fn marker_executable(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix("executable="))
        .map(str::to_owned)
}

/// `plutil -replace <キー> <型> <値>` で書くもの
pub(crate) fn plist_edits() -> Vec<(&'static str, &'static str, String)> {
    vec![
        ("CFBundleIdentifier", "-string", BUNDLE_ID.to_owned()),
        ("CFBundleName", "-string", "JUIZ Launcher".to_owned()),
        (
            "CFBundleDocumentTypes",
            "-xml",
            format!(
                "<array><dict>\
<key>CFBundleTypeName</key><string>JUIZ application</string>\
<key>CFBundleTypeRole</key><string>Viewer</string>\
<key>LSHandlerRank</key><string>Owner</string>\
<key>LSItemContentTypes</key><array><string>{UTI}</string></array>\
<key>CFBundleTypeExtensions</key><array><string>juizapp</string></array>\
</dict></array>"
            ),
        ),
        (
            "UTExportedTypeDeclarations",
            "-xml",
            format!(
                "<array><dict>\
<key>UTTypeIdentifier</key><string>{UTI}</string>\
<key>UTTypeDescription</key><string>JUIZ application</string>\
<key>UTTypeConformsTo</key><array><string>public.data</string></array>\
<key>UTTypeTagSpecification</key><dict><key>public.filename-extension</key><array><string>juizapp</string></array></dict>\
</dict></array>"
            ),
        ),
    ]
}

fn must(program: &str, outcome: ToolOutcome) -> Result<()> {
    match outcome {
        ToolOutcome::Ok => Ok(()),
        ToolOutcome::NotFound => bail!("{program} was not found (it comes with macOS)"),
        ToolOutcome::Failed(e) => bail!("{program} failed: {e}"),
    }
}

/// 今の juiz を .juizapp に関連付ける(§17.1)。警告も返す
pub(crate) fn associate(
    tools: &mut impl Tools,
    apps_dir: &Path,
    exe: &Path,
) -> Result<(Associated, Vec<String>)> {
    let exe_str = exe
        .to_str()
        .ok_or_else(|| anyhow!("the path of juiz is not UTF-8: {}", exe.display()))?;
    let app = apps_dir.join(APP_NAME);
    let marker_path = app.join(MARKER);
    let wanted_marker = marker(exe_str);
    let current = std::fs::read_to_string(&marker_path).ok();
    if current.as_deref() == Some(wanted_marker.as_str()) {
        return Ok((Associated::Unchanged, Vec::new()));
    }
    if app.exists() && current.is_none() {
        bail!(
            "{} exists but was not made by juiz associate; move it away and try again",
            app.display()
        );
    }
    let previous = current
        .as_deref()
        .and_then(marker_executable)
        .filter(|p| p != exe_str);

    std::fs::create_dir_all(apps_dir)
        .map_err(|e| anyhow!("cannot create {}: {e}", apps_dir.display()))?;
    if app.exists() {
        std::fs::remove_dir_all(&app)
            .map_err(|e| anyhow!("cannot remove {}: {e}", app.display()))?;
    }
    let source = apps_dir.join(".juiz-launcher.applescript");
    std::fs::write(&source, script(exe_str))
        .map_err(|e| anyhow!("cannot write {}: {e}", source.display()))?;
    let o = tools.run(
        "osacompile",
        &[OsStr::new("-o"), app.as_os_str(), source.as_os_str()],
    );
    let _ = std::fs::remove_file(&source);
    must("osacompile", o)?;

    let plist = app.join("Contents/Info.plist");
    for (key, kind, value) in plist_edits() {
        let o = tools.run(
            "plutil",
            &[
                OsStr::new("-replace"),
                OsStr::new(key),
                OsStr::new(kind),
                OsStr::new(&value),
                plist.as_os_str(),
            ],
        );
        must("plutil", o)?;
    }
    let marker_path = app.join(MARKER);
    if let Some(parent) = marker_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow!("cannot create {}: {e}", parent.display()))?;
    }
    std::fs::write(&marker_path, &wanted_marker)
        .map_err(|e| anyhow!("cannot write {}: {e}", marker_path.display()))?;

    let mut warnings = Vec::new();
    let o = tools.run(
        "codesign",
        &[
            OsStr::new("--force"),
            OsStr::new("--sign"),
            OsStr::new("-"),
            app.as_os_str(),
        ],
    );
    warnings.extend(warning(
        "codesign",
        &o,
        "macOS may refuse to open JUIZ Launcher",
    ));
    let o = tools.run(LSREGISTER, &[OsStr::new("-f"), app.as_os_str()]);
    warnings.extend(warning(
        LSREGISTER,
        &o,
        "open JUIZ Launcher once from ~/Applications so that macOS learns about it",
    ));
    Ok((
        match previous {
            Some(p) => Associated::Replaced(p),
            None => Associated::New,
        },
        warnings,
    ))
}

/// 関連付けを外す(§17.1)。外したものがあれば true。juiz が作っていない同名のアプリは消さない
pub(crate) fn remove(tools: &mut impl Tools, apps_dir: &Path) -> Result<(bool, Vec<String>)> {
    let app = apps_dir.join(APP_NAME);
    if !app.exists() {
        return Ok((false, Vec::new()));
    }
    if !app.join(MARKER).exists() {
        return Ok((
            false,
            vec![format!(
                "{} was not made by juiz associate; left it as it is",
                app.display()
            )],
        ));
    }
    let mut warnings = Vec::new();
    let o = tools.run(LSREGISTER, &[OsStr::new("-u"), app.as_os_str()]);
    warnings.extend(warning(
        LSREGISTER,
        &o,
        "Finder may show the old association for a while",
    ));
    std::fs::remove_dir_all(&app).map_err(|e| anyhow!("cannot remove {}: {e}", app.display()))?;
    Ok((true, warnings))
}

#[cfg(test)]
mod tests {
    use super::super::tools::fake::{temp_dir, FakeTools};
    use super::*;

    /// osacompile がアプリを作るのを真似る偽物
    fn tools() -> FakeTools {
        let mut t = FakeTools::default();
        t.effects.push((
            "osacompile".to_owned(),
            Box::new(|args: &[String]| {
                let app = Path::new(&args[1]);
                std::fs::create_dir_all(app.join("Contents/Resources")).unwrap();
                std::fs::write(app.join("Contents/Info.plist"), "<plist/>").unwrap();
                // スクリプトを渡されているか
                assert!(std::fs::read_to_string(&args[2])
                    .unwrap()
                    .contains("on open theFiles"));
            }),
        ));
        t
    }

    #[test]
    fn script_quotes_the_path() {
        let s = script(r#"/Users/a b/"x"\y/juiz"#);
        assert!(
            s.contains(r#"do script (quoted form of "/Users/a b/\"x\"\\y/juiz") & " run --launched " & (quoted form of p)"#),
            "{s}"
        );
    }

    #[test]
    fn associate_builds_registers_and_marks() {
        let d = temp_dir("macos_new");
        let apps = d.join("Applications");
        let mut t = tools();
        let exe = Path::new("/Users/a/.venv/bin/juiz");
        let (r, w) = associate(&mut t, &apps, exe).unwrap();
        assert_eq!(r, Associated::New);
        assert!(w.is_empty(), "{w:?}");
        let app = apps.join(APP_NAME);
        assert_eq!(
            std::fs::read_to_string(app.join(MARKER)).unwrap(),
            "executable=/Users/a/.venv/bin/juiz\nlayout=1\n"
        );
        assert!(!apps.join(".juiz-launcher.applescript").exists());
        let programs: Vec<&str> = t.calls.iter().map(|c| c[0].as_str()).collect();
        assert_eq!(
            programs,
            [
                "osacompile",
                "plutil",
                "plutil",
                "plutil",
                "plutil",
                "codesign",
                LSREGISTER
            ]
        );
        assert_eq!(
            t.calls[1][1..4],
            ["-replace", "CFBundleIdentifier", "-string"]
        );
        assert!(t.calls[3][4].contains("<string>juizapp</string>"));
        assert_eq!(t.calls[6][1], "-f");

        // 同じ juiz ならなにもしない
        let mut t2 = tools();
        assert_eq!(
            associate(&mut t2, &apps, exe).unwrap().0,
            Associated::Unchanged
        );
        assert!(t2.calls.is_empty());

        // 別の juiz なら作り直して前のパスを返す
        let (r, _) = associate(&mut t2, &apps, Path::new("/opt/juiz")).unwrap();
        assert_eq!(
            r,
            Associated::Replaced("/Users/a/.venv/bin/juiz".to_owned())
        );

        // 外す
        let mut t3 = tools();
        assert!(remove(&mut t3, &apps).unwrap().0);
        assert!(!app.exists());
        assert_eq!(t3.calls[0][0], LSREGISTER);
        assert_eq!(t3.calls[0][1], "-u");
        assert!(!remove(&mut t3, &apps).unwrap().0);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn foreign_app_is_left_alone_and_failures_are_errors() {
        let d = temp_dir("macos_foreign");
        let apps = d.join("Applications");
        std::fs::create_dir_all(apps.join(APP_NAME)).unwrap();
        let mut t = tools();
        let e = associate(&mut t, &apps, Path::new("/usr/local/bin/juiz")).unwrap_err();
        assert!(
            e.to_string().contains("was not made by juiz associate"),
            "{e}"
        );
        let (removed, w) = remove(&mut t, &apps).unwrap();
        assert!(!removed);
        assert_eq!(w.len(), 1);
        assert!(apps.join(APP_NAME).exists());
        std::fs::remove_dir_all(apps.join(APP_NAME)).unwrap();

        let mut t = tools();
        t.outcomes
            .insert("osacompile".into(), ToolOutcome::NotFound);
        let e = associate(&mut t, &apps, Path::new("/usr/local/bin/juiz")).unwrap_err();
        assert!(e.to_string().contains("osacompile was not found"), "{e}");

        // codesign・lsregister の失敗は警告
        let mut t = tools();
        t.outcomes
            .insert("codesign".into(), ToolOutcome::Failed("x".into()));
        t.outcomes.insert(LSREGISTER.into(), ToolOutcome::NotFound);
        let (r, w) = associate(&mut t, &apps, Path::new("/usr/local/bin/juiz")).unwrap();
        assert_eq!(r, Associated::New);
        assert_eq!(w.len(), 2, "{w:?}");
        assert!(w[1].starts_with("lsregister was not found"), "{w:?}");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 本物の osacompile・plutil・codesign でアプリを作れる(macOS だけ)
    #[cfg(target_os = "macos")]
    #[test]
    fn real_tools_build_a_valid_app() {
        use super::super::tools::SystemTools;
        let d = temp_dir("macos_real");
        let apps = d.join("Applications");
        let mut t = SystemTools;
        let (r, w) = associate(&mut t, &apps, Path::new("/Users/a b/.venv/bin/juiz")).unwrap();
        assert_eq!(r, Associated::New);
        assert!(w.is_empty(), "{w:?}");
        let app = apps.join(APP_NAME);
        let plist = app.join("Contents/Info.plist");
        let o = std::process::Command::new("plutil")
            .args(["-convert", "json", "-o", "-"])
            .arg(&plist)
            .output()
            .unwrap();
        assert!(o.status.success(), "{o:?}");
        let json = String::from_utf8_lossy(&o.stdout);
        assert!(json.contains(BUNDLE_ID), "{json}");
        assert!(json.contains(UTI), "{json}");
        assert!(json.contains("juizapp"), "{json}");
        let o = std::process::Command::new("codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(&app)
            .output()
            .unwrap();
        assert!(o.status.success(), "{o:?}");
        let (removed, w) = remove(&mut t, &apps).unwrap();
        assert!(removed);
        assert!(w.is_empty(), "{w:?}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
