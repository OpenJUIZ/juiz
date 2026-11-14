//! HTTP ブローカーを立てた System に、pyjuiz のプロキシ (juiz.proxy) で繋ぐテスト (issue #79)
//!
//! System を起動したまま別プロセスの Python で bindings/pyjuiz/test/proxy_test.py を実行し、
//! ProcessProxy(name, host, port) / ContainerProxy の call / execute / p_apply を確かめる。
//! ネームスペース robot1 にも同じ名前の inc0 と c0 を置き、名前の取り違えが無いことも確かめる (issue #152)。
//! ルートの c0 には 3D モデル (STL) を持たせ、model_info / model を確かめる (issue #382)。

use crate::broker_api;

use broker_api::*;
use juiz_core::prelude::*;

const PORT: i64 = 8014;

#[test]
fn pyjuiz_proxy_over_http_broker() -> JuizResult<()> {
    crate::common::logger::init();
    let mut manifest = system_manifest("pyjuiz_proxy_test", PORT);
    let model = write_model_file("pyjuiz_proxy_test");
    manifest["containers"][0]["model"] = jvalue!({ "path": model.to_string_lossy(), "scale": 0.001 });
    manifest["processes"].as_array_mut().unwrap().push(jvalue!(
        { "type_name": "increment_process", "name": "inc0", "namespace": "robot1" }
    ));
    manifest["containers"].as_array_mut().unwrap().push(jvalue!(
        { "type_name": "example_container", "name": "c0", "namespace": "robot1",
          "processes": [
              { "type_name": "example_container_increment", "name": "c0inc" },
              { "type_name": "example_container_get", "name": "c0get" } ] }
    ));
    System::new(manifest)?
        .setup()?
        .run_and_do_once(|_system| {
            let python = std::env::var("PYO3_PYTHON").unwrap_or_else(|_| "python3".to_owned());
            let output = std::process::Command::new(python)
                .current_dir("../bindings/pyjuiz")
                .env("PYTHONPATH", ".")
                .arg("test/proxy_test.py")
                .arg(PORT.to_string())
                .arg(&model)
                .output()?;
            assert!(
                output.status.success(),
                "proxy_test.py failed\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            Ok(())
        })
}

/// テスト用の小さなバイナリ STL (三角形 2 つ) を一時ディレクトリに書き、そのパスを返す
pub(crate) fn write_model_file(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("juiz_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut v = vec![0u8; 80];
    v.extend_from_slice(&2u32.to_le_bytes());
    v.extend((0..100).map(|i| i as u8));
    let path = dir.join("c0.stl");
    std::fs::write(&path, v).unwrap();
    path
}
