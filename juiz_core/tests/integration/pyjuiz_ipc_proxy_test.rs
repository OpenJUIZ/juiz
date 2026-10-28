//! IPC ブローカーを立てた System に、pyjuiz の IPC クライアント (juiz.proxy.IpcProxy) で繋ぐテスト (issue #236)
//!
//! HTTP ブローカーは止めたまま System を起動し、別プロセスの Python で
//! bindings/pyjuiz/test/ipc_proxy_test.py を実行する。Process / Container の呼び出しに加えて、
//! 画像 (形式ごと) と数の配列の引数と戻り値、エラー応答を確かめる。
//! Process と conf は ipc_broker_api_test のものを使う。比べるため、同じ Process を HTTP でも呼ぶ。

use juiz_core::prelude::*;

use crate::ipc_broker_api_test::{server_manifest, setup_test_processes};

const PORT: i64 = 8030;

#[test]
fn pyjuiz_proxy_over_ipc_broker() -> JuizResult<()> {
    crate::common::logger::init();
    let ipc_name = "pyjuiz_ipc_test";
    let mut manifest = server_manifest("pyjuiz_ipc_proxy_test", ipc_name);
    manifest["option"]["http_broker"] = jvalue!({ "start": true, "port": PORT });
    System::new(manifest)?
        .setup()?
        .run_and_do_once(|system| {
            setup_test_processes(system)?;
            let python = std::env::var("PYO3_PYTHON").unwrap_or_else(|_| "python3".to_owned());
            let output = std::process::Command::new(python)
                .current_dir("../bindings/pyjuiz")
                .env("PYTHONPATH", ".")
                .args(["test/ipc_proxy_test.py", ipc_name, PORT.to_string().as_str()])
                .output()?;
            assert!(
                output.status.success(),
                "ipc_proxy_test.py failed\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            println!("{}", String::from_utf8_lossy(&output.stdout));
            Ok(())
        })
}
