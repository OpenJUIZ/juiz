// juiz をビルドしたときの target triple を JUIZ_TARGET として埋め込む。
// conf の plugins の path に書いた {target} をこれに置き換える(docs/spec/app-package.md §5、#284)。
fn main() {
    let target = std::env::var("TARGET").expect("cargo sets TARGET for build scripts");
    println!("cargo:rustc-env=JUIZ_TARGET={target}");
    println!("cargo:rerun-if-changed=build.rs");
}
