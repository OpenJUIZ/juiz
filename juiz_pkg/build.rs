// 1. ビルドしたときの target triple を JUIZ_TARGET として埋め込む (`juiz pkg target`、docs/spec/pkg.md §2.3)。
//    juiz_core の build.rs と同じ。juiz と一緒にビルドすれば juiz_core::HOST_TARGET と一致する。
// 2. templates/ の下のファイルを走査し、(相対パス, 中身) の表を OUT_DIR/templates.rs に書く
//    (include_str! でバイナリに埋め込む、docs/spec/pkg.md §10.1)。
use std::path::{Path, PathBuf};

fn collect(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(root, &path, out);
        } else {
            out.push(path.strip_prefix(root).unwrap().to_path_buf());
        }
    }
}

fn main() {
    let target = std::env::var("TARGET").expect("cargo sets TARGET for build scripts");
    println!("cargo:rustc-env=JUIZ_TARGET={target}");

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest_dir.join("templates");
    let mut files = Vec::new();
    collect(&root, &root, &mut files);
    let mut code = String::from("pub(crate) static TEMPLATES: &[(&str, &str)] = &[\n");
    for rel in files {
        // 表のキーは / 区切りにする (Windows でも同じ)
        let key = rel
            .components()
            .map(|c| c.as_os_str().to_str().expect("template path must be UTF-8"))
            .collect::<Vec<_>>()
            .join("/");
        let abs = root.join(&rel);
        code.push_str(&format!(
            "    ({key:?}, include_str!({:?})),\n",
            abs.to_str().expect("template path must be UTF-8")
        ));
    }
    code.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("templates.rs");
    std::fs::write(out, code).unwrap();
    println!("cargo:rerun-if-changed=templates");
    println!("cargo:rerun-if-changed=build.rs");
}
