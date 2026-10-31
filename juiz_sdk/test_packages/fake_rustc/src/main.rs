//! `-V` だけ偽の版を返す rustc のラッパー (docs/spec/rust-plugin-abi.md §14、issue #330)
//!
//! juiz_sdk の build.rs は `$RUSTC -V` の出力をビルド情報 (`juiz_sdk::build_info`) とコンテナの型タグに入れる。
//! RUSTC にこれを指定してプラグインをビルドすると、ホストと違う rustc でビルドしたことになる。
//! コンパイルそのものは本物の rustc (環境変数 JUIZ_REAL_RUSTC、無ければ PATH の rustc) がするので、
//! できるプラグインはホストと同じ rustc でビルドしたものと中身は同じ。
//!
//! `-V` 以外 (cargo が使う `-vV` を含む) はそのまま本物に渡す。

/// 偽の版。テストはこの文字列がビルド情報に入っていることを確かめる
const FAKE_VERSION: &str = "rustc 0.0.0-fake (000000000 2000-01-01)";

fn main() {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "-V" {
        println!("{FAKE_VERSION}");
        return;
    }
    let real = std::env::var_os("JUIZ_REAL_RUSTC").unwrap_or_else(|| "rustc".into());
    let status = std::process::Command::new(&real)
        .args(&args)
        .status()
        .unwrap_or_else(|e| {
            eprintln!("fake_rustc: can not run {real:?} ({e})");
            std::process::exit(127)
        });
    std::process::exit(status.code().unwrap_or(1));
}
