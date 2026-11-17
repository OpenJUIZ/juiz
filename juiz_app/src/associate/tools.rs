//! macOS・Linux の関連付けで呼ぶ OS のコマンド(#396)
//!
//! テストでは偽物に差し替えて、どの OS でも呼ぶ順番と引数を確かめられるようにしている。

use std::ffi::OsStr;

/// コマンドを実行した結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ToolOutcome {
    /// 成功した
    Ok,
    /// コマンドが見つからない
    NotFound,
    /// 失敗した(終了コードと標準エラー)
    Failed(String),
}

/// OS のコマンドを呼ぶ
pub(crate) trait Tools {
    fn run(&mut self, program: &str, args: &[&OsStr]) -> ToolOutcome;
}

/// 本物のコマンドを呼ぶ
pub(crate) struct SystemTools;

impl Tools for SystemTools {
    fn run(&mut self, program: &str, args: &[&OsStr]) -> ToolOutcome {
        match std::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()
        {
            Ok(o) if o.status.success() => ToolOutcome::Ok,
            Ok(o) => {
                let err = String::from_utf8_lossy(&o.stderr).trim().to_owned();
                ToolOutcome::Failed(if err.is_empty() {
                    format!("{}", o.status)
                } else {
                    format!("{}: {err}", o.status)
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => ToolOutcome::NotFound,
            Err(e) => ToolOutcome::Failed(e.to_string()),
        }
    }
}

/// 失敗しても続けるコマンドの結果を警告の文にする。成功なら None
pub(crate) fn warning(program: &str, outcome: &ToolOutcome, consequence: &str) -> Option<String> {
    let name = std::path::Path::new(program)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| program.to_owned());
    match outcome {
        ToolOutcome::Ok => None,
        ToolOutcome::NotFound => Some(format!("{name} was not found; {consequence}")),
        ToolOutcome::Failed(e) => Some(format!("{name} failed ({e}); {consequence}")),
    }
}

#[cfg(test)]
pub(crate) mod fake {
    use super::*;

    /// 呼ばれたコマンドを覚え、決めた結果を返す偽物
    #[derive(Default)]
    pub(crate) struct FakeTools {
        /// 呼ばれた順の「コマンド 引数…」
        pub calls: Vec<Vec<String>>,
        /// コマンド名ごとの結果。無ければ Ok
        pub outcomes: std::collections::HashMap<String, ToolOutcome>,
        /// 呼ばれたときに走らせる処理(osacompile がアプリを作るのを真似るなど)
        #[allow(clippy::type_complexity)]
        pub effects: Vec<(String, Box<dyn FnMut(&[String])>)>,
    }

    impl Tools for FakeTools {
        fn run(&mut self, program: &str, args: &[&OsStr]) -> ToolOutcome {
            let args: Vec<String> = args
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect();
            let mut call = vec![program.to_owned()];
            call.extend(args.iter().cloned());
            self.calls.push(call);
            let outcome = self
                .outcomes
                .get(program)
                .cloned()
                .unwrap_or(ToolOutcome::Ok);
            if outcome == ToolOutcome::Ok {
                for (p, f) in self.effects.iter_mut() {
                    if p == program {
                        f(&args);
                    }
                }
            }
            outcome
        }
    }

    /// テスト用の空のディレクトリ
    pub(crate) fn temp_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("juiz_associate_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
}
