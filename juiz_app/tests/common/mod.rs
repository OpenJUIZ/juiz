//! juiz コマンドを起動して叩くテスト (issue #222) で共有する道具。
//!
//! プラグインはワークスペースの cargo build で ../target/debug にできている前提 (scripts/test.sh)。
#![allow(dead_code)]

use assert_cmd::prelude::*;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output};
use std::time::{Duration, Instant};

/// 起動した juiz をテストの終わりに止める。
///
/// Unix では SIGTERM で止めて、juiz が自分で終わるのを待つ (カバレッジの記録が書き出されるように)。
/// 終わらなければ kill する。
pub struct Daemon {
    child: Child,
    pub log: PathBuf,
}

impl Daemon {
    /// juiz を起動する。標準出力と標準エラーは log に書く
    pub fn spawn(args: &[&str], log: PathBuf) -> Daemon {
        let file = std::fs::File::create(&log).unwrap();
        let child = juiz()
            .args(args)
            .stdout(file.try_clone().unwrap())
            .stderr(file)
            .spawn()
            .unwrap();
        Daemon { child, log }
    }

    pub fn log_text(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    /// log に text が出るまで待つ
    pub fn wait_log(&self, text: &str) {
        let deadline = Instant::now() + Duration::from_secs(60);
        while !self.log_text().contains(text) {
            if Instant::now() > deadline {
                panic!("'{text}' did not appear. log:\n{}", self.log_text());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// 止めて、終わるまで待つ。Unix では SIGTERM で正常に終わったかを返す
    pub fn stop(&mut self) -> bool {
        #[cfg(unix)]
        {
            let _ = Command::new("kill")
                .args(["-TERM", &self.child.id().to_string()])
                .status();
            let deadline = Instant::now() + Duration::from_secs(10);
            while Instant::now() < deadline {
                if let Ok(Some(status)) = self.child.try_wait() {
                    return status.success();
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        false
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            self.stop();
        }
    }
}

pub fn juiz() -> Command {
    Command::cargo_bin("juiz").unwrap()
}

/// ワークスペースの cargo build でできるプラグインの場所 (他のテストの "../target/debug" と同じ)。
/// cargo llvm-cov ではテストの実行ファイルが別の target にできるが、プラグインはここを使う
pub fn plugin_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("target")
        .join("debug")
}

/// conf に書くためのプラグインのディレクトリ (Windows でも / 区切り)
pub fn plugin_dir_str() -> String {
    plugin_dir().display().to_string().replace('\\', "/")
}

/// プラグインのファイル。名前は OS ごとに組み立てる (libfoo.so / libfoo.dylib / foo.dll)
pub fn plugin_file(name: &str) -> String {
    use std::env::consts::{DLL_PREFIX, DLL_SUFFIX};
    plugin_dir()
        .join(format!("{DLL_PREFIX}{name}{DLL_SUFFIX}"))
        .display()
        .to_string()
}

/// テストごとの作業ディレクトリ。前回の残りは消す
pub fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("juiz_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// juiz を実行して出力を返す
pub fn run(args: &[&str]) -> Output {
    juiz().args(args).output().unwrap()
}

/// juiz を実行し、成功したことを確かめて標準出力を返す
pub fn run_ok(args: &[&str]) -> String {
    let out = run(args);
    assert!(out.status.success(), "{args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

pub fn path_str(p: &Path) -> &str {
    p.to_str().unwrap()
}
