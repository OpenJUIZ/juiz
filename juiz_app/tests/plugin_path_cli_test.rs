//! conf の plugins の path の `{target}` 置換と、ブローカー・EC のプラグインを conf の親ディレクトリから
//! 探すことのテスト (issue #284、docs/spec/app-package.md §5.2・§5.3)
//!
//! プラグイン (qmp_broker, increment_process, timer_ec) は scripts/test.sh で ../target/debug にビルドされている前提。
mod common;

use common::{juiz, plugin_file, temp_dir};
use std::path::Path;

/// パッケージと同じ形 (app.conf と plugins/<target>/) のディレクトリを作る
fn write_app(dir: &Path) {
    let plugins = dir.join("plugins").join(juiz_core::HOST_TARGET);
    std::fs::create_dir_all(&plugins).unwrap();
    for name in ["qmp_broker", "increment_process", "timer_ec"] {
        let src = plugin_file(name);
        let file = Path::new(&src).file_name().unwrap();
        std::fs::copy(&src, plugins.join(file)).unwrap();
    }
    std::fs::write(
        dir.join("app.conf"),
        r#"name: plugin_path_cli_test
option:
  http_broker:
    start: false
plugins:
  broker_factories:
    qmp_broker:
      path: "plugins/{target}"
  process_factories:
    increment_process:
      path: "plugins/{target}"
  ec_factories:
    timer_ec:
      path: "plugins/{target}"
processes:
  - type_name: increment_process
    name: inc0
"#,
    )
    .unwrap();
}

#[test]
fn target_placeholder_and_broker_path_relative_to_conf() {
    let dir = temp_dir("plugin_path_cli");
    let app = dir.join("app");
    let other = dir.join("other");
    write_app(&app);
    std::fs::create_dir_all(&other).unwrap();

    // conf のあるディレクトリとは別のディレクトリから相対パスで起動する
    let out = juiz()
        .current_dir(&other)
        .env("RUST_LOG", "info")
        .args(["-f", "../app/app.conf"])
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "{out:?}");
    // ブローカーは組み込みの qmp ではなく、conf の隣の plugins/<target>/ から読み込まれる
    assert!(
        log.contains("BrokerFactory (name=qmp_broker) Loaded"),
        "{log}"
    );
    assert!(
        log.contains("ExecutionContextFactory (name=timer_ec) Loaded"),
        "{log}"
    );
    assert!(log.contains(juiz_core::HOST_TARGET), "{log}");
    assert!(!log.contains("{target}"), "{log}");
    assert!(
        log.contains("core://core/process/inc0::increment_process"),
        "{log}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
