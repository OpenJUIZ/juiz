//! HTTP ブローカーを立てた System に、pyjuiz のプロキシ (juiz.proxy) で繋ぐテスト (issue #79)
//!
//! System を起動したまま別プロセスの Python で bindings/pyjuiz/test/proxy_test.py を実行し、
//! ProcessProxy(name, host, port) / ContainerProxy の call / execute / p_apply を確かめる。
//! ネームスペース robot1 にも同じ名前の inc0 と c0 を置き、名前の取り違えが無いことも確かめる (issue #152)。

use crate::broker_api;

use broker_api::*;
use juiz_core::prelude::*;

const PORT: i64 = 8014;

#[test]
fn pyjuiz_proxy_over_http_broker() -> JuizResult<()> {
    crate::common::logger::init();
    let mut manifest = system_manifest("pyjuiz_proxy_test", PORT);
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
                .args(["test/proxy_test.py", PORT.to_string().as_str()])
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
