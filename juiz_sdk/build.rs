//! juiz_sdk をビルドしたコンパイラの版を JUIZ_SDK_RUSTC_VERSION に入れる。
//!
//! プラグインとホスト(juiz)はそれぞれ自分の juiz_sdk をビルドするので、
//! この値を比べるとプラグインとホストのコンパイラが同じかどうかが分かる(build_info.rs)。
use std::process::Command;

fn main() {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned());
    let version = Command::new(rustc)
        .arg("-V")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|v| v.trim().to_owned())
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=JUIZ_SDK_RUSTC_VERSION={version}");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=RUSTC");
}
