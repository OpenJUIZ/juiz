//! `juiz pkg` のテスト (docs/spec/pkg.md §11、issue #457)
//!
//! 雛形を一時ディレクトリに `--juiz-source` で作り、`juiz pkg add` で型を足してから
//! scripts/dev.sh → `juiz -f app/app.conf` → 型を全部 call → scripts/pack.sh → `juiz run` が通ることを確かめる。
//! Rust の雛形のビルドは target/pkg-test に置く (ワークスペースの target と分け、2 回目からは速い)。

use crate::common::{juiz, path_str, run, temp_dir, Daemon};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()
        .unwrap()
}

/// テストしている juiz があるディレクトリを先頭にした PATH (雛形の scripts が juiz を呼ぶため)
fn path_with_juiz() -> String {
    let bin = assert_cmd::cargo::cargo_bin("juiz");
    let dir = bin.parent().unwrap().to_path_buf();
    let mut paths = vec![dir];
    if let Some(p) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&p));
    }
    std::env::join_paths(paths)
        .unwrap()
        .into_string()
        .unwrap()
}

#[test]
fn pkg_target_prints_host_target() {
    let out = run(&["pkg", "target"]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8(out.stdout).unwrap().trim(),
        juiz_core::HOST_TARGET
    );
}

#[test]
fn pkg_create_errors() {
    let dir = temp_dir("pkg_cli_errors");
    let out_dir = dir.join("x");
    let o = path_str(&out_dir);
    // 名前の規則に合わない (直した例を出す)
    let r = run(&["pkg", "create", "My-Robot", "-b", "rust", "-o", o]);
    assert!(!r.status.success());
    let err = String::from_utf8_lossy(&r.stderr);
    assert!(err.contains("'my_robot'"), "{err}");
    // 知らない言語 (受け付ける値を並べる)
    let r = run(&["pkg", "create", "my_robot", "-b", "go", "-o", o]);
    assert!(!r.status.success());
    let err = String::from_utf8_lossy(&r.stderr);
    assert!(err.contains("rust") && err.contains("python"), "{err}");
    // 何も残さない
    assert!(!out_dir.exists());
    // 空でないディレクトリには作らない
    std::fs::create_dir_all(&out_dir).unwrap();
    std::fs::write(out_dir.join("keep"), "x").unwrap();
    let r = run(&["pkg", "create", "my_robot", "-b", "rust", "-o", o]);
    assert!(!r.status.success());
    assert_eq!(std::fs::read_dir(&out_dir).unwrap().count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// `juiz pkg design` と `juiz pkg design check` (pkg.md §2.6・§2.7、設計文書 Step 1)
#[test]
fn pkg_design_and_check() {
    let dir = temp_dir("pkg_cli_design");
    let file = dir.join("d").join("juizdesign.yaml");
    let f = path_str(&file);
    let r = run(&["pkg", "design", "my_robot", "-b", "rust", "-o", f]);
    assert!(r.status.success(), "{r:?}");
    let out = String::from_utf8_lossy(&r.stdout);
    assert!(out.contains("(component my_robot: 1 process, 1 container, 2 container processes)"), "{out}");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("  language: rust\n") && text.contains("  - name: my_robot_add\n"), "{text}");
    // 上書きしない
    let r = run(&["pkg", "design", "my_robot", "-o", f]);
    assert!(!r.status.success());
    assert!(String::from_utf8_lossy(&r.stderr).contains("already exists"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
    // check: 通れば ok の 1 行
    let r = run(&["pkg", "design", "check", f]);
    assert!(r.status.success(), "{r:?}");
    assert_eq!(
        String::from_utf8_lossy(&r.stdout).trim(),
        format!("ok: {f} (component my_robot: 1 process, 1 container, 2 container processes)")
    );
    // check: エラーはすべて <file>:<行>:<桁>: error: <道筋>: の形で並べ、0 以外で終わる
    std::fs::write(
        &file,
        text.replace("type: long, default: 1,", "type: long, default: one,")
            .replace("returns: long\n        readonly: true", "returns: Count\n        readonly: true"),
    )
    .unwrap();
    let r = run(&["pkg", "design", "check", f]);
    assert!(!r.status.success());
    let err = String::from_utf8_lossy(&r.stderr);
    assert!(err.contains(&format!("{f}:28:")) && err.contains(": error: containers[0].processes[0].arguments[0].default: "), "{err}");
    assert!(err.contains(": error: containers[0].processes[1].returns: unknown type 'Count'"), "{err}");
    assert!(err.contains("(2 errors)"), "{err}");
    // 言語の規則は -b で
    std::fs::write(&file, text.replace("my_robot_add", "fn")).unwrap();
    assert!(run(&["pkg", "design", "check", f, "-b", "python"]).status.success());
    let r = run(&["pkg", "design", "check", f]);
    assert!(!r.status.success());
    assert!(String::from_utf8_lossy(&r.stderr).contains("processes[0]: cannot be made in Rust"));
    // juiz pkg create も雛形の直下に設計文書を置く
    let app = dir.join("app_py");
    let r = run(&["pkg", "create", "my_robot", "-b", "python", "-t", "process", "-o", path_str(&app)]);
    assert!(r.status.success(), "{r:?}");
    let r = run(&["pkg", "design", "check", path_str(&app.join("juizdesign.yaml"))]);
    assert!(r.status.success(), "{r:?}");
    assert!(String::from_utf8_lossy(&r.stdout).contains("(component my_robot: 1 process, 0 containers, 0 container processes)"));
    // juizdesign.yaml の無い雛形 (設計文書より前に作ったもの) への pkg add は警告して、ソースと conf だけを直す
    std::fs::remove_file(app.join("juizdesign.yaml")).unwrap();
    let r = run(&["pkg", "add", "process", "my_mul", "-C", path_str(&app)]);
    assert!(r.status.success(), "{r:?}");
    assert!(String::from_utf8_lossy(&r.stderr).contains("warning: juizdesign.yaml is not in this skeleton"), "{r:?}");
    assert!(String::from_utf8_lossy(&r.stdout).contains("Edited app/python/my_robot.py, app/app.conf\n"), "{r:?}");
    assert!(!app.join("juizdesign.yaml").exists());
    // juiz pkg design --idl: .jidl の宣言から文書を作る (idl: は文書からの相対パス)
    let idl_dir = dir.join("from_idl");
    std::fs::create_dir_all(idl_dir.join("idl")).unwrap();
    std::fs::write(idl_dir.join("idl").join("arm.jidl"), "namespace arm; /** Moves */ process double move_to(double x = 0.5);\n").unwrap();
    let doc = idl_dir.join("juizdesign.yaml");
    let r = run(&["pkg", "design", "my_arm", "--idl", path_str(&idl_dir.join("idl").join("arm.jidl")), "-o", path_str(&doc)]);
    assert!(r.status.success(), "{r:?}");
    let text = std::fs::read_to_string(&doc).unwrap();
    assert!(text.contains("idl:\n  - idl/arm.jidl\nprocesses:\n  - name: move_to\n    description: \"Moves\"\n"), "{text}");
    assert!(run(&["pkg", "design", "check", path_str(&doc), "-b", "zig"]).status.success());
    // juiz pkg design --from-plugin (設計文書 Step 5): 型付きと型の無い引数が混ざったテスト用のプラグインから起こす。
    // 型の無い引数は警告、record は idl/<name>.jidl に書く
    let lib = crate::common::plugin_file("native_pose_component");
    let doc = dir.join("from_plugin").join("juizdesign.yaml");
    let r = run(&["pkg", "design", "--from-plugin", &lib, "-o", path_str(&doc)]);
    assert!(!r.status.success());
    assert!(String::from_utf8_lossy(&r.stderr).contains("give -b"), "{r:?}");
    let r = run(&["pkg", "design", "--from-plugin", &lib, "-b", "rust", "-o", path_str(&doc)]);
    assert!(r.status.success(), "{r:?}");
    let err = String::from_utf8_lossy(&r.stderr);
    assert!(err.contains("warning: processes[0].arguments[0]: the plugin gives no type (schema) for the argument 'x', so it was written as double"), "{err}");
    assert!(doc.parent().unwrap().join("idl").join("native_pose_component.jidl").is_file());
    let text = std::fs::read_to_string(&doc).unwrap();
    assert!(text.contains("  name: native_pose_component\n  language: rust\n") && text.contains("    returns: robot.geometry.Pose2D\n"), "{text}");
    let r = run(&["pkg", "design", "check", path_str(&doc), "--plugin", &lib]);
    assert!(r.status.success(), "{r:?}");
    assert!(String::from_utf8_lossy(&r.stdout).contains("matches the plugin"), "{r:?}");
    // 食い違いはエラー (P1)
    std::fs::write(&doc, text.replace("  name: native_pose_component\n", "  name: other_component\n")).unwrap();
    let r = run(&["pkg", "design", "check", path_str(&doc), "--plugin", &lib]);
    assert!(!r.status.success());
    assert!(String::from_utf8_lossy(&r.stderr).contains("error: component.name: the component name is 'other_component', but the plugin's is 'native_pose_component'"), "{r:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// 生成した app.conf のポートをテスト用に変える (既定の 8000 は他と重なりうるので)
fn set_port(app: &Path, port: u16) {
    let conf = app.join("app").join("app.conf");
    let text = std::fs::read_to_string(&conf).unwrap();
    assert!(text.contains("\"port\": 8000\n"), "{text}");
    std::fs::write(&conf, text.replace("\"port\": 8000\n", &format!("\"port\": {port}\n"))).unwrap();
}

fn create(dir: &Path, name: &str, lang: &str, extra: &[&str]) -> PathBuf {
    let app = dir.join(name);
    let repo = repo_root();
    let mut args = vec![
        "pkg",
        "create",
        name,
        "-b",
        lang,
        "-o",
        path_str(&app),
        "--juiz-source",
        path_str(&repo),
        "--license",
        "mit-apache",
        "--author",
        "juiz test",
    ];
    args.extend_from_slice(extra);
    let out = run(&args);
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Next steps:"), "{stdout}");
    app
}

/// cargo test が LD_LIBRARY_PATH (macOS は DYLD_LIBRARY_PATH) に足す target/ の下を外したもの。
/// 外さないと bundle-plugin-deps.sh の確かめで、隣に写した libjuiz_sdk より先に target/debug のものが見つかる (Haskell)
fn library_path_without_target(var: &str) -> String {
    let target = repo_root().join("target");
    std::env::var_os(var)
        .map(|v| {
            let kept: Vec<PathBuf> = std::env::split_paths(&v)
                .filter(|p| !p.starts_with(&target))
                .collect();
            std::env::join_paths(kept).unwrap().into_string().unwrap()
        })
        .unwrap_or_default()
}

/// 雛形の scripts/<name> を実行する。Windows では .ps1 の方を PowerShell で実行し、
/// 引数を .ps1 の形 (`--no-build` → `-NoBuild`、`--release` → `-Release`) に直す (#588)
fn script(app: &Path, name: &str, args: &[&str]) {
    let mut cmd = if cfg!(windows) {
        let ps1 = app.join("scripts").join(name.replace(".sh", ".ps1"));
        // PowerShell 7 (pwsh) があればそれを使う。無ければ Windows PowerShell 5.1
        let shell = if Command::new("pwsh").arg("-Version").output().is_ok() { "pwsh" } else { "powershell" };
        let mut c = Command::new(shell);
        c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]).arg(ps1);
        for a in args {
            c.arg(match *a {
                "--no-build" => "-NoBuild",
                "--release" => "-Release",
                other => panic!("no .ps1 form for {other}"),
            });
        }
        c
    } else {
        let mut c = Command::new(app.join("scripts").join(name));
        c.args(args);
        c
    };
    let out = cmd
        .env("LD_LIBRARY_PATH", library_path_without_target("LD_LIBRARY_PATH"))
        .env("DYLD_LIBRARY_PATH", library_path_without_target("DYLD_LIBRARY_PATH"))
        .current_dir(app)
        .env("PATH", path_with_juiz())
        .env("CARGO_TARGET_DIR", repo_root().join("target").join("pkg-test"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{name}: {out:?}");
}

/// juiz を起動し、id (Process か ContainerProcess) が引数なしで呼べるようになるまで待つ
fn start(cmd_args: &[&str], app: &Path, log: PathBuf, first_id: &str) -> Daemon {
    let mut cmd = juiz();
    cmd.args(cmd_args).current_dir(app);
    let d = Daemon::spawn_command(cmd, log);
    let deadline = Instant::now() + Duration::from_secs(60);
    let kind = if first_id.contains("/container_process/") {
        "container-process"
    } else {
        "process"
    };
    loop {
        let out = juiz()
            .args([kind, "call", first_id, "{}", "-p"])
            .current_dir(app)
            .output()
            .unwrap();
        if out.status.success() && String::from_utf8_lossy(&out.stdout).contains("Value(") {
            return d;
        }
        if Instant::now() > deadline {
            panic!("{first_id} did not start. log:\n{}", d.log_text());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn call(app: &Path, kind: &str, id: &str, args: &str) -> String {
    let out = juiz()
        .args([kind, "call", id, args, "-p"])
        .current_dir(app)
        .output()
        .unwrap();
    assert!(out.status.success(), "{id}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

/// §4 の型を全部呼んで、決めた値が返ることを確かめる
fn call_all(app: &Path, name: &str, port: u16) {
    let h = format!("http://127.0.0.1:{port}");
    let add = format!("{h}/process/{name}_add0::{name}_add");
    assert!(call(app, "process", &add, r#"{"a": 1, "b": 2}"#).contains("Value(3)"));
    assert!(call(app, "process", &add, "{}").contains("Value(0)"));
    let inc = format!("{h}/container_process/{name}_increment0::{name}_increment:{name}0");
    let get = format!("{h}/container_process/{name}_get0::{name}_get:{name}0");
    assert!(call(app, "container-process", &get, "{}").contains("Value(0)"));
    assert!(call(app, "container-process", &inc, r#"{"step": 2}"#).contains("Value(2)"));
    assert!(call(app, "container-process", &inc, "{}").contains("Value(3)"));
    assert!(call(app, "container-process", &get, "{}").contains("Value(3)"));
    // _get は readonly
    let out = juiz()
        .args(["-s", &h, "container-process", "list", "--long"])
        .current_dir(app)
        .output()
        .unwrap();
    let list = String::from_utf8_lossy(&out.stdout);
    assert!(list.contains("readonly"), "{list}");
}

/// `juiz pkg add` (§13、issue #461) で Process <name>_sum と Container <name>_arm を足す
fn add_types(app: &Path, name: &str) {
    let sum = format!("{name}_sum");
    let arm = format!("{name}_arm");
    for (kind, t) in [("process", sum.as_str()), ("container", arm.as_str())] {
        let out = run(&["pkg", "add", kind, t, "-C", path_str(app)]);
        assert!(out.status.success(), "{out:?}");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.starts_with(&format!("Added {kind} {t} to {name} ")), "{stdout}");
    }
    // juizdesign.yaml にも書き足し、書き足した文書は検査を通る (design.md §8.3、設計文書 Step 4)
    let doc_path = app.join("juizdesign.yaml");
    let doc = std::fs::read_to_string(&doc_path).unwrap();
    for t in [&sum, &arm, &format!("{arm}_increment"), &format!("{arm}_get")] {
        assert!(doc.contains(&format!("- name: {t}\n")), "{t}\n{doc}");
    }
    let out = run(&["pkg", "design", "check", path_str(&doc_path)]);
    assert!(out.status.success(), "{out:?}");
    // 同じ名前をもう一度足すとエラーで、何も変えない
    let conf = std::fs::read_to_string(app.join("app").join("app.conf")).unwrap();
    let out = run(&["pkg", "add", "process", &sum, "-C", path_str(app)]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("already"), "{out:?}");
    assert_eq!(conf, std::fs::read_to_string(app.join("app").join("app.conf")).unwrap());
    assert_eq!(doc, std::fs::read_to_string(&doc_path).unwrap());
}

/// add_types で足した型を呼ぶ
fn call_added(app: &Path, name: &str, port: u16) {
    let h = format!("http://127.0.0.1:{port}");
    let sum = format!("{h}/process/{name}_sum0::{name}_sum");
    assert!(call(app, "process", &sum, r#"{"a": 4, "b": 5}"#).contains("Value(9)"));
    let inc = format!("{h}/container_process/{name}_arm_increment0::{name}_arm_increment:{name}_arm0");
    let get = format!("{h}/container_process/{name}_arm_get0::{name}_arm_get:{name}_arm0");
    assert!(call(app, "container-process", &inc, r#"{"step": 5}"#).contains("Value(5)"));
    assert!(call(app, "container-process", &get, "{}").contains("Value(5)"));
}

/// 雛形を作るディレクトリ。Windows では juiz のソースと同じドライブにする: Zig の build.zig.zon は
/// zigjuiz を相対パスで指すので、TEMP (C:) とチェックアウト (CI では D:) が違うと作れない (#588)
fn work_dir(name: &str) -> PathBuf {
    if !cfg!(windows) {
        return temp_dir(name);
    }
    // canonicalize が付ける \\?\ を外す (PowerShell の Set-Location が \\?\ 付きのパスを辿れない)
    let root = repo_root();
    let root = root.to_str().unwrap();
    let dir = PathBuf::from(root.strip_prefix(r"\\?\").unwrap_or(root))
        .join("target")
        .join("pkg-test-work")
        .join(format!("{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn create_build_run_pack(lang: &str, name: &str, port: u16, build: bool, extra: &[&str]) {
    let dir = work_dir(&format!("pkg_cli_{lang}"));
    let app = create(&dir, name, lang, extra);
    set_port(&app, port);
    add_types(&app, name);
    if build {
        script(&app, "dev.sh", &[]);
        assert!(app
            .join("app")
            .join("plugins")
            .join(juiz_core::HOST_TARGET)
            .is_dir());
    }
    let add = format!("http://127.0.0.1:{port}/process/{name}_add0::{name}_add");

    let mut d = start(&["-f", "app/app.conf", "-d"], &app, dir.join("dev.log"), &add);
    call_all(&app, name, port);
    call_added(&app, name, port);
    assert!(d.stop() || cfg!(not(unix)), "{}", d.log_text());

    // dev.sh でビルド済みなので、pack はビルドを飛ばす (release ビルドの時間を省く)
    script(&app, "pack.sh", &["--no-build"]);
    let pkg = app.join("dist").join(format!("{name}-0.1.0.juizapp"));
    assert!(pkg.is_file());
    check_package(&pkg, lang);
    let mut d = start(
        &["run", path_str(&pkg)],
        &app,
        dir.join("run.log"),
        &add,
    );
    call_all(&app, name, port);
    call_added(&app, name, port);
    assert!(d.stop() || cfg!(not(unix)), "{}", d.log_text());
    let _ = std::fs::remove_dir_all(&dir);
}

/// できた .juizapp の中身を確かめる (C++ は libjuiz_sdk を同梱し、同じ版の juiz を求める)
fn check_package(pkg: &Path, lang: &str) {
    let file = std::fs::File::open(pkg).unwrap();
    let mut zip = zip::ZipArchive::new(file).unwrap();
    let names: Vec<String> = (0..zip.len())
        .map(|i| zip.by_index(i).unwrap().name().unwrap().into_owned())
        .collect();
    let manifest = {
        let mut f = zip.by_name("juizapp.yaml").unwrap();
        let mut s = String::new();
        std::io::Read::read_to_string(&mut f, &mut s).unwrap();
        s
    };
    assert!(names.iter().any(|n| n == "LICENSE-MIT"), "{names:?}");
    if lang == "cpp" || lang == "haskell" || lang == "zig" {
        let sdk = format!(
            "plugins/{}/{}juiz_sdk.{}",
            juiz_core::HOST_TARGET,
            if cfg!(windows) { "" } else { "lib" },
            if cfg!(target_os = "macos") { "dylib" } else if cfg!(windows) { "dll" } else { "so" }
        );
        assert!(names.contains(&sdk), "{sdk} is not in {names:?}");
        assert!(
            manifest.contains(&format!("juiz: \"={}\"", env!("CARGO_PKG_VERSION"))),
            "{manifest}"
        );
    }
    if lang == "haskell" {
        // hsjuiz と GHC の RTS も scripts/bundle-plugin-deps.sh で同梱する
        let dir = format!("plugins/{}/", juiz_core::HOST_TARGET);
        for lib in ["libHShsjuiz-", "libHSrts-", "libHSbase-"] {
            assert!(
                names.iter().any(|n| n.starts_with(&format!("{dir}{lib}"))),
                "{lib}* is not in {names:?}"
            );
        }
    }
}

/// --idl の雛形 (§6.5): juiz_std と別ファイルの record を使う宣言から型付きの関数を作り、
/// 本体を書き換え (`bodies` に (雛形のソース, 元の行, 書き換えた行) を並べる)、`juiz pkg add` で型を足してから、
/// ビルド→起動→call→型の確認→pack→juiz run を通す
fn idl_build_run_pack(
    lang: &str,
    name: &str,
    port: u16,
    build: bool,
    extra: &[&str],
    source: &str,
    bodies: &[(&str, &str)],
) {
    let dir = work_dir(&format!("pkg_cli_{lang}_idl"));
    let idl = dir.join("idl_src");
    std::fs::create_dir_all(idl.join("common")).unwrap();
    std::fs::write(
        idl.join("robot.jidl"),
        r#"namespace pkg_idl;
import idl "juiz_std/geometry.jidl";
import idl "common/goal.jidl";
/** Remaining angle to the goal */
process double pkg_idl_remaining(pkg_idl.common.Goal goal, juiz_std.geometry.Pose2D from_pose);
container pkg_idl_arm { container_process long pkg_idl_double(long step = 1); }
"#,
    )
    .unwrap();
    std::fs::write(
        idl.join("common").join("goal.jidl"),
        r#"namespace pkg_idl.common;
import idl "juiz_std/geometry.jidl";
record Goal { juiz_std.geometry.Pose2D pose; double tolerance = 0.5; }
"#,
    )
    .unwrap();
    let jidl = idl.join("robot.jidl");
    let mut args = vec!["--idl", path_str(&jidl)];
    args.extend_from_slice(extra);
    let app = create(&dir, name, lang, &args);
    set_port(&app, port);
    assert!(app.join("idl").join("common").join("goal.jidl").is_file());
    // .jidl から作った設計文書が雛形の直下にあり、検査を通る (設計文書 Step 4)
    let doc = std::fs::read_to_string(app.join("juizdesign.yaml")).unwrap();
    assert!(doc.contains("idl:\n  - idl/robot.jidl\n") && doc.contains("  - name: pkg_idl_remaining\n"), "{doc}");
    let out = run(&["pkg", "design", "check", path_str(&app.join("juizdesign.yaml"))]);
    assert!(out.status.success(), "{out:?}");

    // 本体を書く (利用者がすること)
    let src = app.join(source);
    let mut code = std::fs::read_to_string(&src).unwrap();
    for (from, to) in bodies {
        assert!(code.contains(from), "{from}\n---\n{code}");
        code = code.replace(from, to);
    }
    assert!(!code.contains("write the body of pkg_idl"), "{code}");
    std::fs::write(&src, code).unwrap();
    // juiz pkg add も --idl の雛形に使える
    add_types(&app, name);

    if build {
        script(&app, "dev.sh", &[]);
    }
    let h = format!("http://127.0.0.1:{port}");
    let remaining = format!("{h}/process/pkg_idl_remaining0::pkg_idl_remaining");
    let double = format!("{h}/container_process/pkg_idl_double0::pkg_idl_double:pkg_idl_arm0");
    let check = |app: &Path| {
        let r = call(
            app,
            "process",
            &remaining,
            r#"{"goal": {"pose": {"theta": 2.0}}, "from_pose": {"theta": 0.5}}"#,
        );
        assert!(r.contains("Value(2.0)"), "{r}");
        assert!(call(app, "container-process", &double, r#"{"step": 3}"#).contains("Value(6)"));
        assert!(call(app, "container-process", &double, "{}").contains("Value(2)"));
        // 引数の型が .jidl の型になっている (型の一覧に、使っている Process と一緒に出る)
        let out = juiz()
            .args(["-s", &h, "type", "list", "--long"])
            .current_dir(app)
            .output()
            .unwrap();
        let list = String::from_utf8_lossy(&out.stdout);
        assert!(
            list.contains("pkg_idl.common.Goal")
                && list.contains("juiz_std.geometry.Pose2D")
                && list.contains("pkg_idl_remaining"),
            "{list}"
        );
        call_added(app, name, port);
    };
    let mut d = start(&["-f", "app/app.conf", "-d"], &app, dir.join("dev.log"), &double);
    check(&app);
    // Windows には TERM が無く kill で止めるので、正常に終わったかは Unix でだけ見る
    assert!(d.stop() || cfg!(not(unix)), "{}", d.log_text());

    script(&app, "pack.sh", &["--no-build"]);
    let pkg = app.join("dist").join(format!("{name}-0.1.0.juizapp"));
    check_package(&pkg, lang);
    let mut d = start(&["run", path_str(&pkg)], &app, dir.join("run.log"), &double);
    check(&app);
    // Windows には TERM が無く kill で止めるので、正常に終わったかは Unix でだけ見る
    assert!(d.stop() || cfg!(not(unix)), "{}", d.log_text());
    let _ = std::fs::remove_dir_all(&dir);
}

/// `create -d` の文書 (設計文書 Step 2・3、design.md §12): .jidl の record、Container の引数・状態・readonly。
/// Zig は文字列の引数で状態を始められない (引数のメモリは Container を作ると解放される) ので、label は既定値で始める
fn design_doc(name: &str, lang: &str) -> String {
    let label_state = if lang == "zig" {
        "{ name: label, type: string, default: \"c\" }"
    } else {
        "{ name: label, type: string, init: label }"
    };
    format!(
        r#"format: 1
component:
  name: {name}
  description: "Scales vectors and counts."
  language: {lang}
idl:
  - idl/types.jidl
processes:
  - name: pkg_design_scale
    description: "Scales v by k."
    arguments:
      - {{ name: v, type: pkg_design.types.Vec2 }}
      - {{ name: k, type: double, default: 2.0 }}
    returns: pkg_design.types.Vec2
containers:
  - name: pkg_design_counter
    description: "Counts from start."
    arguments:
      - {{ name: start, type: long, default: 10 }}
      - {{ name: label, type: string, default: "c" }}
    state:
      - {{ name: count, type: long, init: start, description: "the count" }}
      - {label_state}
      - {{ name: history, type: "array<long>" }}
      - {{ name: last, type: "pkg_design.types.Vec2?" }}
    processes:
      - name: pkg_design_counter_add
        arguments:
          - {{ name: step, type: long, default: 1 }}
        returns: long
      - name: pkg_design_counter_get
        readonly: true
        returns: long
"#
    )
}

/// 雛形のプラグイン (dev.sh でビルドしたもの、Python はソース)
fn skeleton_plugin(app: &Path, name: &str, lang: &str) -> PathBuf {
    if lang == "python" {
        return app.join("app").join("python").join(format!("{name}.py"));
    }
    let ext = if cfg!(target_os = "macos") { "dylib" } else if cfg!(windows) { "dll" } else { "so" };
    let dir = app.join("app").join("plugins").join(juiz_core::HOST_TARGET);
    [format!("lib{name}.{ext}"), format!("{name}.{ext}")]
        .iter()
        .map(|f| dir.join(f))
        .find(|p| p.is_file())
        .unwrap_or_else(|| panic!("no plugin of {name} in {}", dir.display()))
}

/// 設計文書 Step 5 (design.md §8.5・§8.6): 雛形の文書がビルドしたプラグインと合い (`design check --plugin`)、
/// プラグインから起こした文書 (`design --from-plugin`) もそのプラグインと合う
fn check_against_plugin(app: &Path, dir: &Path, name: &str, lang: &str) {
    let lib = skeleton_plugin(app, name, lang);
    let out = juiz()
        .args(["pkg", "design", "check", "juizdesign.yaml", "--plugin", path_str(&lib)])
        .current_dir(app)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success() && stdout.contains("matches the plugin"), "{out:?}");
    assert!(!String::from_utf8_lossy(&out.stderr).contains("error"), "{out:?}");

    let from = dir.join("from_plugin");
    let doc = from.join("juizdesign.yaml");
    let out = juiz()
        .args(["pkg", "design", "--from-plugin", path_str(&lib), "-b", lang, "-o", path_str(&doc)])
        .current_dir(app)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let text = std::fs::read_to_string(&doc).unwrap();
    // record は .jidl に、Container の状態はプラグインに無いので書かない
    assert!(text.contains(&format!("  - idl/{name}.jidl\n")) && text.contains("type: pkg_design.types.Vec2"), "{text}");
    assert!(std::fs::read_to_string(from.join("idl").join(format!("{name}.jidl"))).unwrap().contains("record Vec2"));
    assert!(!text.contains("state:"), "{text}");
    let out = juiz()
        .args(["pkg", "design", "check", path_str(&doc), "--plugin", path_str(&lib)])
        .current_dir(app)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(!String::from_utf8_lossy(&out.stderr).contains("error"), "{out:?}");
    let _ = std::fs::remove_dir_all(&from);
}

/// `create -d` (pkg.md §2.5、design.md §12 の Step 2): 文書から作った雛形の本体を書き、`juiz pkg add` で型を足してから、
/// ビルド→起動→call→型と readonly の確認→pack→juiz run を通す。文書は雛形の外のディレクトリに置く
fn design_build_run_pack(
    lang: &str,
    name: &str,
    port: u16,
    build: bool,
    extra: &[&str],
    source: &str,
    bodies: &[(&str, &str)],
) {
    let dir = work_dir(&format!("pkg_cli_{lang}_design"));
    let design = dir.join("design");
    std::fs::create_dir_all(design.join("idl")).unwrap();
    std::fs::write(
        design.join("idl").join("types.jidl"),
        "namespace pkg_design.types;\nrecord Vec2 { double x = 0.0; double y = 0.0; }\n",
    )
    .unwrap();
    let doc = design.join("juizdesign.yaml");
    std::fs::write(&doc, design_doc(name, lang)).unwrap();
    let r = run(&["pkg", "design", "check", path_str(&doc)]);
    assert!(r.status.success(), "{r:?}");

    let app = dir.join(name);
    let repo = repo_root();
    let mut args = vec![
        "pkg", "create", "-d", path_str(&doc), "-o", path_str(&app),
        "--juiz-source", path_str(&repo), "--license", "mit-apache", "--author", "juiz test",
    ];
    args.extend_from_slice(extra);
    let out = run(&args);
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains(&format!("Created {name} ({lang}, from juizdesign.yaml)")), "{stdout}");
    assert!(stdout.contains("# write the bodies of the functions"), "{stdout}");
    // 文書と .jidl は雛形の直下に写す (§8.2)
    assert_eq!(std::fs::read_to_string(app.join("juizdesign.yaml")).unwrap(), design_doc(name, lang));
    assert!(app.join("idl").join("types.jidl").is_file());
    set_port(&app, port);
    let conf = std::fs::read_to_string(app.join("app").join("app.conf")).unwrap();
    assert!(conf.contains("    \"start\": 10\n    \"label\": \"c\"\n"), "{conf}");

    // 本体を書く (利用者がすること)
    let src = app.join(source);
    let mut code = std::fs::read_to_string(&src).unwrap();
    for (from, to) in bodies {
        assert!(code.contains(from), "{from}\n---\n{code}");
        code = code.replace(from, to);
    }
    assert!(!code.contains("write the body of pkg_design"), "{code}");
    std::fs::write(&src, code).unwrap();
    add_types(&app, name);

    if build {
        script(&app, "dev.sh", &[]);
    }
    check_against_plugin(&app, &dir, name, lang);
    let h = format!("http://127.0.0.1:{port}");
    let scale = format!("{h}/process/pkg_design_scale0::pkg_design_scale");
    let add = format!("{h}/container_process/pkg_design_counter_add0::pkg_design_counter_add:pkg_design_counter0");
    let get = format!("{h}/container_process/pkg_design_counter_get0::pkg_design_counter_get:pkg_design_counter0");
    let check = |app: &Path| {
        let r = call(app, "process", &scale, r#"{"v": {"x": 1.5, "y": -2.0}}"#);
        assert!(r.contains("3.0") && r.contains("-4.0"), "{r}");
        // 状態は Container の引数 start (conf の 10) から
        assert!(call(app, "container-process", &get, "{}").contains("Value(10)"));
        assert!(call(app, "container-process", &add, r#"{"step": 5}"#).contains("Value(15)"));
        assert!(call(app, "container-process", &add, "{}").contains("Value(16)"));
        assert!(call(app, "container-process", &get, "{}").contains("Value(16)"));
        // 引数・戻り値は型付き (§6.1)、_get は readonly
        let out = juiz().args(["-s", &h, "type", "list", "--long"]).current_dir(app).output().unwrap();
        let list = String::from_utf8_lossy(&out.stdout);
        assert!(list.contains("pkg_design.types.Vec2") && list.contains("pkg_design_scale"), "{list}");
        let out = juiz().args(["-s", &h, "container-process", "list", "--long"]).current_dir(app).output().unwrap();
        let list = String::from_utf8_lossy(&out.stdout);
        // readonly は ContainerProcess の行の次の行に出る
        let lines: Vec<&str> = list.lines().collect();
        let readonly = |id: &str| {
            let i = lines.iter().position(|l| l.contains(id)).unwrap();
            lines.get(i + 1).is_some_and(|l| l.trim() == "readonly")
        };
        assert!(readonly("pkg_design_counter_get0") && !readonly("pkg_design_counter_add0"), "{list}");
        call_added(app, name, port);
    };
    let mut d = start(&["-f", "app/app.conf", "-d"], &app, dir.join("dev.log"), &get);
    check(&app);
    assert!(d.stop() || cfg!(not(unix)), "{}", d.log_text());

    script(&app, "pack.sh", &["--no-build"]);
    let pkg = app.join("dist").join(format!("{name}-0.1.0.juizapp"));
    check_package(&pkg, lang);
    let mut d = start(&["run", path_str(&pkg)], &app, dir.join("run.log"), &get);
    check(&app);
    assert!(d.stop() || cfg!(not(unix)), "{}", d.log_text());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Python の create -d (設計文書 Step 2)
#[cfg(all(unix, feature = "embed-python"))]
#[test]
fn pkg_create_python_design_runs_and_packs() {
    design_build_run_pack(
        "python",
        "pkg_py_design",
        8102,
        false,
        &[],
        "app/python/pkg_py_design.py",
        &[
            (
                "    raise NotImplementedError(\"write the body of pkg_design_scale\")",
                "    return Vec2(x=v.x * k, y=v.y * k)",
            ),
            (
                "    raise NotImplementedError(\"write the body of pkg_design_counter_add\")",
                "    container.count += step\n    container.history.append(step)\n    return container.count",
            ),
            (
                "    raise NotImplementedError(\"write the body of pkg_design_counter_get\")",
                "    return container.count",
            ),
        ],
    );
}

/// Python の --idl (issue #468): 型のパッケージは app/python/ に作ってあり、juiz_std は pyjuiz のもの
#[cfg(all(unix, feature = "embed-python"))]
#[test]
fn pkg_create_python_idl_runs_and_packs() {
    idl_build_run_pack(
        "python",
        "pkg_py_idl",
        8083,
        false,
        &[],
        "app/python/pkg_py_idl.py",
        &[
            (
                "    raise NotImplementedError(\"write the body of pkg_idl_remaining\")",
                "    return goal.pose.theta - from_pose.theta + goal.tolerance",
            ),
            (
                "    raise NotImplementedError(\"write the body of pkg_idl_double\")",
                "    return step * 2",
            ),
        ],
    );
}

#[cfg(all(unix, feature = "embed-python"))]
#[test]
fn pkg_create_python_component_runs_and_packs() {
    create_build_run_pack("python", "pkg_py_robot", 8079, false, &[]);
}

/// ワークスペースでビルドした libjuiz_sdk を、配布と同じ形 (scripts/package-juiz-sdk.sh のアーカイブ) にして展開する。
/// 雛形の CMakeLists.txt は、これを --juiz-sdk-dir で指して find_package(juiz_sdk <版> EXACT) する
fn sdk_from_workspace(dir: &Path) -> PathBuf {
    let lib = if cfg!(target_os = "macos") {
        "libjuiz_sdk.dylib"
    } else {
        "libjuiz_sdk.so"
    };
    // juiz と同じ所にあればそれを使う (scripts/test.sh の cargo build で target/debug にできる)
    let bin_dir = assert_cmd::cargo::cargo_bin("juiz").parent().unwrap().to_path_buf();
    let lib_dir = [bin_dir, repo_root().join("target").join("debug")]
        .into_iter()
        .find(|d| d.join(lib).is_file())
        .unwrap_or_else(|| panic!("{lib} is not built. Run cargo build (scripts/test.sh does) first"));
    let out = Command::new(repo_root().join("scripts").join("package-juiz-sdk.sh"))
        .arg(juiz_core::HOST_TARGET)
        .arg(dir)
        .env("JUIZ_SDK_LIB_DIR", &lib_dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "package-juiz-sdk.sh: {out:?}");
    let archive = String::from_utf8(out.stdout).unwrap().trim().to_owned();
    let out = Command::new("tar")
        .args(["-xzf", &archive, "-C"])
        .arg(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "tar: {out:?}");
    let sdk = dir.join(format!(
        "juiz_sdk-{}-{}",
        env!("CARGO_PKG_VERSION"),
        juiz_core::HOST_TARGET
    ));
    assert!(sdk.join("lib").join("cmake").join("juiz_sdk").is_dir(), "{}", sdk.display());
    sdk
}

/// Zig の雛形のテストに使う libjuiz_sdk。Unix は sdk_from_workspace (配布と同じ形)。
/// Windows は package-juiz-sdk.sh (Git Bash と 7z が要る) を使わず、zigjuiz が見る 2 つのファイルだけを
/// 配布と同じ置き場所 (bin/juiz_sdk.dll と lib/juiz_sdk.dll.lib) に写す (#588)
fn zig_sdk_from_workspace(dir: &Path) -> PathBuf {
    if !cfg!(windows) {
        return sdk_from_workspace(dir);
    }
    let bin_dir = assert_cmd::cargo::cargo_bin("juiz").parent().unwrap().to_path_buf();
    let lib_dir = [bin_dir, repo_root().join("target").join("debug")]
        .into_iter()
        .find(|d| d.join("juiz_sdk.dll").is_file() && d.join("juiz_sdk.dll.lib").is_file())
        .unwrap_or_else(|| panic!("juiz_sdk.dll is not built. Run cargo build (scripts/test.sh does) first"));
    let sdk = dir.join(format!(
        "juiz_sdk-{}-{}",
        env!("CARGO_PKG_VERSION"),
        juiz_core::HOST_TARGET
    ));
    for (sub, file) in [("bin", "juiz_sdk.dll"), ("lib", "juiz_sdk.dll.lib")] {
        std::fs::create_dir_all(sdk.join(sub)).unwrap();
        std::fs::copy(lib_dir.join(file), sdk.join(sub).join(file)).unwrap();
    }
    sdk
}

/// GHC・cabal (Linux では patchelf も) が無ければ飛ばす。JUIZ_REQUIRE_HASKELL=1 なら飛ばさずに落とす (scripts/test.sh と同じ)
fn haskell_tools_missing() -> Option<String> {
    let has = |cmd: &str| {
        Command::new(cmd)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    let mut missing: Vec<&str> = ["ghc", "cabal"].into_iter().filter(|c| !has(c)).collect();
    if cfg!(target_os = "linux") && !has("patchelf") {
        missing.push("patchelf");
    }
    if missing.is_empty() {
        return None;
    }
    let msg = format!("{missing:?} not found (needed for the Haskell skeleton test)");
    if std::env::var("JUIZ_REQUIRE_HASKELL").as_deref() == Ok("1") {
        panic!("{msg}");
    }
    Some(msg)
}

/// Zig の雛形を作れるか。zig は環境変数 ZIG で変えられる (例: ZIG="python3 -m ziglang"、scripts/test.sh と同じ)。
/// Zig 0.17 が無ければ飛ばし、JUIZ_REQUIRE_ZIG=1 なら飛ばさずに落とす
fn zig_missing() -> Option<String> {
    let zig = std::env::var("ZIG").unwrap_or_else(|_| "zig".to_owned());
    let mut words = zig.split_whitespace();
    let ok = words.next().is_some_and(|cmd| {
        Command::new(cmd)
            .args(words)
            .arg("version")
            .output()
            .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).starts_with("0.17."))
            .unwrap_or(false)
    });
    if ok {
        return None;
    }
    let msg = format!("Zig 0.17 ({zig}) not found (needed for the Zig skeleton test)");
    if std::env::var("JUIZ_REQUIRE_ZIG").as_deref() == Ok("1") {
        panic!("{msg}");
    }
    Some(msg)
}

/// 雛形を cargo・cmake・cabal・zig でビルドするテスト。1 件 10〜50 秒かかるので、`--skip slow::` で外せるようにまとめる
/// (CONTRIBUTING.md の「カテゴリと置き場所」の slow)
mod slow {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn pkg_create_rust_component_builds_runs_and_packs() {
        create_build_run_pack("rust", "pkg_rust_robot", 8078, true, &[]);
    }

    #[cfg(unix)]
    #[test]
    fn pkg_create_rust_idl_builds_runs_and_packs() {
        idl_build_run_pack(
            "rust",
            "pkg_rust_idl",
            8082,
            true,
            &[],
            "src/lib.rs",
            &[
                (
                    "    let _ = (goal, from_pose);\n    todo!(\"write the body of pkg_idl_remaining\")",
                    "    Ok(goal.pose.theta - from_pose.theta + goal.tolerance)",
                ),
                (
                    "    let _ = (container, step);\n    todo!(\"write the body of pkg_idl_double\")",
                    "    let _ = container;\n    Ok(step * 2)",
                ),
            ],
        );
    }

    /// Rust の create -d (設計文書 Step 2)
    #[cfg(unix)]
    #[test]
    fn pkg_create_rust_design_builds_runs_and_packs() {
        design_build_run_pack(
            "rust",
            "pkg_rust_design",
            8090,
            true,
            &[],
            "src/lib.rs",
            &[
                (
                    "    let _ = (v, k);\n    todo!(\"write the body of pkg_design_scale\")",
                    "    Ok(crate::pkg_design::types::Vec2 { x: v.x * k, y: v.y * k })",
                ),
                (
                    "    let _ = (container, step);\n    todo!(\"write the body of pkg_design_counter_add\")",
                    "    container.count += step;\n    container.history.push(step);\n    Ok(container.count)",
                ),
                (
                    "    let _ = container;\n    todo!(\"write the body of pkg_design_counter_get\")",
                    "    Ok(container.count)",
                ),
            ],
        );
    }

    #[cfg(unix)]
    #[test]
    fn pkg_create_cpp_component_builds_runs_and_packs() {
        let dir = temp_dir("pkg_cli_cpp_sdk");
        let sdk = sdk_from_workspace(&dir);
        create_build_run_pack("cpp", "pkg_cpp_robot", 8077, true, &["--juiz-sdk-dir", path_str(&sdk)]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Haskell (§6.4、issue #459): C++ と同じく配布と同じ形の libjuiz_sdk を --juiz-sdk-dir で指し、
    /// hsjuiz は --juiz-source の bindings/hsjuiz を使う。pack は bundle-plugin-deps.sh で GHC のライブラリごと集める
    #[cfg(unix)]
    #[test]
    fn pkg_create_haskell_component_builds_runs_and_packs() {
        if let Some(reason) = haskell_tools_missing() {
            eprintln!("skip: {reason}");
            return;
        }
        let dir = temp_dir("pkg_cli_haskell_sdk");
        let sdk = sdk_from_workspace(&dir);
        create_build_run_pack("haskell", "pkg_hs_robot", 8076, true, &["--juiz-sdk-dir", path_str(&sdk)]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Zig (§6.6、issue #577): C++ と同じく配布と同じ形の libjuiz_sdk を --juiz-sdk-dir で指し、
    /// zigjuiz は --juiz-source の bindings/zigjuiz を使う。zigjuiz の installBundle が libjuiz_sdk をプラグインの隣に置く。
    /// Windows でも走らせ、scripts/dev.ps1・pack.ps1 を通す (#588)
    #[test]
    fn pkg_create_zig_component_builds_runs_and_packs() {
        if let Some(reason) = zig_missing() {
            eprintln!("skip: {reason}");
            return;
        }
        let dir = work_dir("pkg_cli_zig_sdk");
        let sdk = zig_sdk_from_workspace(&dir);
        create_build_run_pack("zig", "pkg_zig_robot", 8081, true, &["--juiz-sdk-dir", path_str(&sdk)]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Zig の --idl (issue #577): build.zig の juiz.addTypes が `juiz idl -g zig` で型を作る
    /// (PATH に juiz-idl が無ければ juiz の `juiz idl`)。Windows でも走らせる (#588)
    #[test]
    fn pkg_create_zig_idl_builds_runs_and_packs() {
        if let Some(reason) = zig_missing() {
            eprintln!("skip: {reason}");
            return;
        }
        let dir = work_dir("pkg_cli_zig_idl_sdk");
        let sdk = zig_sdk_from_workspace(&dir);
        idl_build_run_pack(
            "zig",
            "pkg_zig_idl",
            8088,
            true,
            &["--juiz-sdk-dir", path_str(&sdk)],
            "src/pkg_zig_idl.zig",
            &[
                (
                    "    _ = args;\n    // write the body of pkg_idl_remaining\n    return error.NotImplemented;",
                    "    return args.goal.pose.theta - args.from_pose.theta + args.goal.tolerance;",
                ),
                (
                    "    _ = self;\n    _ = args;\n    // write the body of pkg_idl_double\n    return error.NotImplemented;",
                    "    _ = self;\n    return args.step * 2;",
                ),
            ],
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// C++ の create -d (設計文書 Step 3): CMake の juiz_generate_types が `idl:` の型を juiz_idl.h に作る
    #[cfg(unix)]
    #[test]
    fn pkg_create_cpp_design_builds_runs_and_packs() {
        let dir = temp_dir("pkg_cli_cpp_design_sdk");
        let sdk = sdk_from_workspace(&dir);
        design_build_run_pack(
            "cpp",
            "pkg_cpp_design",
            8103,
            true,
            &["--juiz-sdk-dir", path_str(&sdk)],
            "src/pkg_cpp_design.cpp",
            &[
                (
                    "    (void)v;\n    (void)k;\n    throw std::runtime_error(\"write the body of pkg_design_scale\");",
                    "    ::pkg_design::types::Vec2 r;\n    r.x = v.x * k;\n    r.y = v.y * k;\n    return r;",
                ),
                (
                    "    (void)container;\n    (void)step;\n    throw std::runtime_error(\"write the body of pkg_design_counter_add\");",
                    "    container->count += step;\n    container->history.push_back(step);\n    return container->count;",
                ),
                (
                    "    (void)container;\n    throw std::runtime_error(\"write the body of pkg_design_counter_get\");",
                    "    return container->count;",
                ),
            ],
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Haskell の create -d (設計文書 Step 3): scripts/dev.sh が `idl:` の型を gen/ に作る
    #[cfg(unix)]
    #[test]
    fn pkg_create_haskell_design_builds_runs_and_packs() {
        if let Some(reason) = haskell_tools_missing() {
            eprintln!("skip: {reason}");
            return;
        }
        let dir = temp_dir("pkg_cli_haskell_design_sdk");
        let sdk = sdk_from_workspace(&dir);
        design_build_run_pack(
            "haskell",
            "pkg_hs_design",
            8104,
            true,
            &["--juiz-sdk-dir", path_str(&sdk)],
            "src/PkgHsDesign.hs",
            &[
                (
                    "pkg_design_scale _v _k = error \"write the body of pkg_design_scale\"",
                    "pkg_design_scale v k =\n  Pkg_design.Types.Vec2 (Pkg_design.Types.vec2X v * k) (Pkg_design.Types.vec2Y v * k)",
                ),
                (
                    "pkg_design_counter_add (PkgDesignCounterState _countRef _labelRef _historyRef _lastRef) _step = error \"write the body of pkg_design_counter_add\"",
                    "pkg_design_counter_add (PkgDesignCounterState countRef _labelRef historyRef _lastRef) step = do\n  modifyIORef' historyRef (++ [step])\n  atomicModifyIORef' countRef (\\v -> (v + step, v + step))",
                ),
                (
                    "pkg_design_counter_get (PkgDesignCounterState _countRef _labelRef _historyRef _lastRef) = error \"write the body of pkg_design_counter_get\"",
                    "pkg_design_counter_get (PkgDesignCounterState countRef _labelRef _historyRef _lastRef) = readIORef countRef",
                ),
            ],
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Zig の create -d (設計文書 Step 3): build.zig の juiz.addTypes が `idl:` の型を作る。Windows でも走らせる
    #[test]
    fn pkg_create_zig_design_builds_runs_and_packs() {
        if let Some(reason) = zig_missing() {
            eprintln!("skip: {reason}");
            return;
        }
        let dir = work_dir("pkg_cli_zig_design_sdk");
        let sdk = zig_sdk_from_workspace(&dir);
        design_build_run_pack(
            "zig",
            "pkg_zig_design",
            8105,
            true,
            &["--juiz-sdk-dir", path_str(&sdk)],
            "src/pkg_zig_design.zig",
            &[
                (
                    "    _ = args;\n    // write the body of pkg_design_scale\n    return error.NotImplemented;",
                    "    return .{ .x = args.v.x * args.k, .y = args.v.y * args.k };",
                ),
                (
                    "    _ = self;\n    _ = args;\n    // write the body of pkg_design_counter_add\n    return error.NotImplemented;",
                    "    self.count += args.step;\n    return self.count;",
                ),
                (
                    "    _ = self;\n    // write the body of pkg_design_counter_get\n    return error.NotImplemented;",
                    "    return self.count;",
                ),
            ],
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// C++ の --idl (issue #468): CMake の juiz_generate_types が `juiz idl -g cpp` で juiz_idl.h を作る
    #[cfg(unix)]
    #[test]
    fn pkg_create_cpp_idl_builds_runs_and_packs() {
        let dir = temp_dir("pkg_cli_cpp_idl_sdk");
        let sdk = sdk_from_workspace(&dir);
        idl_build_run_pack(
            "cpp",
            "pkg_cpp_idl",
            8084,
            true,
            &["--juiz-sdk-dir", path_str(&sdk)],
            "src/pkg_cpp_idl.cpp",
            &[
                (
                    "    (void)goal;\n    (void)from_pose;\n    throw std::runtime_error(\"write the body of pkg_idl_remaining\");",
                    "    return goal.pose.theta - from_pose.theta + goal.tolerance;",
                ),
                (
                    "    (void)step;\n    throw std::runtime_error(\"write the body of pkg_idl_double\");",
                    "    return step * 2;",
                ),
            ],
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Haskell の --idl (issue #468): scripts/dev.sh が `juiz idl -g haskell` で gen/ に型のモジュールを作る
    #[cfg(unix)]
    #[test]
    fn pkg_create_haskell_idl_builds_runs_and_packs() {
        if let Some(reason) = haskell_tools_missing() {
            eprintln!("skip: {reason}");
            return;
        }
        let dir = temp_dir("pkg_cli_haskell_idl_sdk");
        let sdk = sdk_from_workspace(&dir);
        idl_build_run_pack(
            "haskell",
            "pkg_hs_idl",
            8085,
            true,
            &["--juiz-sdk-dir", path_str(&sdk)],
            "src/PkgHsIdl.hs",
            &[
                (
                    "pkg_idl_remaining _goal _fromPose = error \"write the body of pkg_idl_remaining\"",
                    "pkg_idl_remaining goal fromPose =\n  Juiz_std.Geometry.pose2DTheta (Pkg_idl.Common.goalPose goal)\n    - Juiz_std.Geometry.pose2DTheta fromPose + Pkg_idl.Common.goalTolerance goal",
                ),
                (
                    "pkg_idl_double _ _step = error \"write the body of pkg_idl_double\"",
                    "pkg_idl_double _ step = pure (step * 2)",
                ),
            ],
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
