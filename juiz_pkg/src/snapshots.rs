//! `juiz pkg create -t` の生成物のスナップショット (docs/spec/design.md §7.3)
//!
//! 設計文書 Step 2・3 で `-t` を文書経由に切り替えるとき、切り替えの前後で生成物がどう変わったかを
//! PR の差分で見られるように、生成するファイルを全部 1 つのテキストにして tests/snapshots/ に置く。
//! 版・絶対パス・Zig の fingerprint は決まった文字に置き換える。
//!
//! 生成物を変えたときは `JUIZ_PKG_UPDATE_SNAPSHOTS=1 cargo test -p juiz_pkg snapshots` で書き直し、
//! 差分の理由を PR に書く。

use std::path::{Path, PathBuf};

use clap::Parser;

use crate::create::Plan;
use crate::{Lang, Template, JUIZ_VERSION};

const LANGS: [Lang; 5] = [Lang::Rust, Lang::Python, Lang::Cpp, Lang::Haskell, Lang::Zig];
const TEMPLATES: [Template; 3] = [Template::Component, Template::Process, Template::Container];

#[derive(Parser)]
struct Cli {
    #[command(flatten)]
    args: crate::cli::PkgArgs,
}

fn snapshot_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("snapshots")
}

/// 生成物を 1 つのテキストにする (ファイルごとに `==== <パス> ====` の見出し)
pub(crate) fn render_plan(plan: &Plan) -> String {
    let mut out = String::new();
    for f in plan.files().unwrap() {
        out.push_str(&format!("==== {} ====\n{}", f.path, f.content));
        if !f.content.ends_with('\n') {
            out.push_str("\n<no newline at end of file>\n");
        }
    }
    out.push_str(&format!("==== <summary> ====\n{}", plan.summary()));
    normalize(&out)
}

fn normalize(s: &str) -> String {
    s.replace(JUIZ_VERSION, "<version>")
}

/// 置き換えの効く決まった場所で計画を作る
pub(crate) fn plan_for(args: &[&str]) -> Plan {
    let mut v = vec!["juiz-pkg"];
    v.extend_from_slice(args);
    let c = match Cli::try_parse_from(v).unwrap().args.command {
        crate::cli::PkgCommand::Create(c) => c,
        _ => panic!(),
    };
    let (mut p, _) = Plan::from_args_with(&c, || Some("Tester".to_owned()), false).unwrap();
    p.zig_fingerprint = 0x1234_5678_0000_0001;
    p.year = 2026;
    p
}

fn snapshot_args(lang: Lang, template: Template) -> Vec<String> {
    vec![
        "create".into(),
        "my_robot".into(),
        "-b".into(),
        lang.dir().into(),
        "-t".into(),
        template.dir().into(),
        "-o".into(),
        "/work/my_robot".into(),
        "--juiz-sdk-dir".into(),
        "/opt/juiz-sdk".into(),
        "--hsjuiz-dir".into(),
        "/opt/juiz/bindings/hsjuiz".into(),
        "--zigjuiz-dir".into(),
        "/opt/juiz/bindings/zigjuiz".into(),
    ]
}

/// 書いてあるスナップショットと比べる。`JUIZ_PKG_UPDATE_SNAPSHOTS=1` なら書き直す
#[track_caller]
pub(crate) fn assert_snapshot(name: &str, actual: &str) {
    let path = snapshot_dir().join(format!("{name}.txt"));
    if std::env::var("JUIZ_PKG_UPDATE_SNAPSHOTS").as_deref() == Ok("1") {
        std::fs::create_dir_all(snapshot_dir()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} (run with JUIZ_PKG_UPDATE_SNAPSHOTS=1 to write it)", path.display()))
        .replace("\r\n", "\n");
    if expected != actual {
        let (e, a): (Vec<&str>, Vec<&str>) = (expected.lines().collect(), actual.lines().collect());
        let at = e.iter().zip(a.iter()).position(|(x, y)| x != y).unwrap_or(e.len().min(a.len()));
        panic!(
            "the output differs from {} at line {}:\n  expected: {:?}\n  actual:   {:?}\n(run with JUIZ_PKG_UPDATE_SNAPSHOTS=1 to write it, and explain the difference in the PR)",
            path.display(),
            at + 1,
            e.get(at),
            a.get(at)
        );
    }
}

/// -t の 5 言語 × 3 種類 (絶対パスを使うので Unix だけ。Windows では /work が絶対パスにならない)
#[cfg(unix)]
#[test]
fn create_template_snapshots() {
    for lang in LANGS {
        for template in TEMPLATES {
            let args = snapshot_args(lang, template);
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let plan = plan_for(&args);
            assert_snapshot(&format!("{}-{}", lang.dir(), template.dir()), &render_plan(&plan));
        }
    }
}

/// `create -d` で §3.1 の 2 つめの例 (rover) から作る雛形 (5 言語)
#[cfg(unix)]
#[test]
fn create_design_snapshots() {
    let dir = std::env::temp_dir().join(format!("juiz_pkg_snapshot_rover_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("idl")).unwrap();
    std::fs::write(dir.join("idl/rover.jidl"), crate::design::tests::ROVER_JIDL).unwrap();
    let doc = dir.join("juizdesign.yaml");
    std::fs::write(&doc, crate::design::tests::ROVER).unwrap();
    for lang in ["rust", "python", "cpp", "haskell", "zig"] {
        let plan = plan_for(&["create", "-d", doc.to_str().unwrap(), "-b", lang, "-o", "/work/rover"]);
        assert_snapshot(&format!("{lang}-design-rover"), &render_plan(&plan));
    }
    let _ = std::fs::remove_dir_all(&dir);
}
