//! language: "zig" のプラグイン (zigjuiz) のテスト。
//! Zig の例 (examples/zig) は scripts/test.sh (CI も同じ) で ../target にビルドされている前提。
//! zig が無い環境では scripts/test.sh がビルドを飛ばすので、ライブラリが無ければテストも飛ばす。
//! CI では JUIZ_REQUIRE_ZIG=1 にして、飛ばさずに失敗させる。

mod component;
mod container;
mod process;

pub fn plugin_file(name: &str) -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    let file = format!("lib{name}.dylib");
    #[cfg(target_os = "linux")]
    let file = format!("lib{name}.so");
    #[cfg(target_os = "windows")]
    let file = format!("{name}.dll");
    std::path::PathBuf::from("../target").join(file)
}

/// Zig の例 names がビルドされていなければ true (テストを飛ばす)
pub fn skip_without_zig_examples(names: &[&str]) -> bool {
    let missing: Vec<_> = names.iter().map(|n| plugin_file(n)).filter(|p| !p.exists()).collect();
    if missing.is_empty() {
        return false;
    }
    if std::env::var("JUIZ_REQUIRE_ZIG").map_or(false, |v| v == "1") {
        panic!("Zig の例がビルドされていません: {missing:?}");
    }
    eprintln!(
        "skip: Zig の例がビルドされていません ({missing:?})。Zig 0.17 を入れて scripts/test.sh を実行してください。"
    );
    true
}
