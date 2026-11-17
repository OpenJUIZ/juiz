//! `juiz sdk install` / `juiz sdk path` (issue #548) のテスト
//!
//! GitHub には取りに行かず、`JUIZ_SDK_BASE_URL=file://<dir>` に Releases と同じ並びのアーカイブを置いて確かめる。
//! アーカイブの形・照合・展開の細かい場合は juiz_app/src/sdk.rs の単体テストで確かめている。

use crate::common::{juiz, temp_dir};
use sha2::{Digest, Sha256};
use std::path::Path;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// この OS・CPU 向けのアーカイブのターゲット名 (sdk-release.yml の 5 種)
fn host_target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-gnu"),
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("windows", "x86_64") => Some("x86_64-pc-windows-msvc"),
        _ => None,
    }
}

fn make_release(base: &Path, target: &str) {
    let name = format!("juiz_sdk-{VERSION}-{target}");
    let files: [(String, &[u8]); 2] = [
        (format!("{name}/cjuiz/include/juiz_sdk.h"), b"// h\n"),
        (
            format!("{name}/lib/cmake/juiz_sdk/juiz_sdkConfig.cmake"),
            b"# c\n",
        ),
    ];
    let (file, data) = if target.contains("-windows-") {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (p, c) in &files {
            w.start_file(p.as_str(), zip::write::SimpleFileOptions::default())
                .unwrap();
            std::io::Write::write_all(&mut w, c).unwrap();
        }
        (format!("{name}.zip"), w.finish().unwrap().into_inner())
    } else {
        let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut b = tar::Builder::new(gz);
        for (p, c) in &files {
            let mut h = tar::Header::new_gnu();
            h.set_size(c.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            b.append_data(&mut h, p, *c).unwrap();
        }
        (
            format!("{name}.tar.gz"),
            b.into_inner().unwrap().finish().unwrap(),
        )
    };
    let dir = base.join(format!("v{VERSION}"));
    std::fs::create_dir_all(&dir).unwrap();
    let sum: String = Sha256::digest(&data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    std::fs::write(dir.join(&file), &data).unwrap();
    std::fs::write(
        dir.join(format!("{file}.sha256")),
        format!("{sum}  {file}\n"),
    )
    .unwrap();
}

#[test]
fn sdk_install_and_path() {
    let Some(target) = host_target() else {
        return;
    };
    let dir = temp_dir("sdk_cli");
    let home = dir.join("home");
    let base = dir.join("releases");
    make_release(&base, target);
    let cmd = |args: &[&str]| {
        juiz()
            .args(args)
            .env("JUIZ_SDK_HOME", &home)
            .env("JUIZ_SDK_BASE_URL", format!("file://{}", base.display()))
            .output()
            .unwrap()
    };

    // 入れる前は path が失敗し、install を案内する
    let out = cmd(&["sdk", "path"]);
    assert!(!out.status.success(), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("juiz sdk install"),
        "{out:?}"
    );

    // 既定の版 (juiz と同じ) を JUIZ_SDK_HOME/<版> に入れる
    let out = cmd(&["sdk", "install"]);
    assert!(out.status.success(), "{out:?}");
    let dest = home.join(VERSION);
    assert!(
        String::from_utf8_lossy(&out.stdout).contains(&format!(
            "installed libjuiz_sdk {VERSION} to {}",
            dest.display()
        )),
        "{out:?}"
    );
    assert!(dest.join("cjuiz/include/juiz_sdk.h").is_file());

    let out = cmd(&["sdk", "path"]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim_end(),
        dest.display().to_string()
    );
    let out = cmd(&["sdk", "path", "--cmake-dir"]);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim_end(),
        dest.join("lib")
            .join("cmake")
            .join("juiz_sdk")
            .display()
            .to_string()
    );

    // 2 回目は取りに行かない
    let out = cmd(&["sdk", "install"]);
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("is already installed"),
        "{out:?}"
    );

    // 公開されていない版は分かる言葉で失敗する
    let out = cmd(&["sdk", "install", "--version", "v99.0.0"]);
    assert!(!out.status.success(), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("cannot download libjuiz_sdk 99.0.0"),
        "{out:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
