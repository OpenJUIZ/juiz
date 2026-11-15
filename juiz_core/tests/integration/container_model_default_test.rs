//! 型 (Factory) の既定のモデル (ContainerModel Step 8、issue #387、仕様 docs/spec/container-model.md §5.6) のテスト
//!
//! テスト用のプラグイン model_container (juiz_sdk/test_packages) をワークスペースの cargo build で
//! 作っておく必要がある。プラグインを一時ディレクトリに写し、その横に models/ を置く。

use std::path::PathBuf;

use juiz_core::models::sha256_hex;
use juiz_core::prelude::*;

/// テスト用の一時ディレクトリ (System の working_dir にする)
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let p = std::env::temp_dir().join(format!("juiz_{name}_{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(p.join("plugin/models")).unwrap();
        std::fs::create_dir_all(p.join("models")).unwrap();
        TempDir(p)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.0.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    /// ワークスペースの target/debug にできたプラグインを plugin/ に写す
    fn copy_plugin(&self) {
        let file = format!(
            "{}model_container{}",
            std::env::consts::DLL_PREFIX,
            std::env::consts::DLL_SUFFIX
        );
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../target/debug")
            .join(&file);
        std::fs::copy(&src, self.0.join("plugin").join(&file)).unwrap_or_else(|e| {
            panic!("{src:?} can not be copied ({e}). Run `cargo build` in the workspace first")
        });
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn binary_stl(triangles: u32) -> Vec<u8> {
    let mut v = vec![0u8; 80];
    v.extend_from_slice(&triangles.to_le_bytes());
    v.extend(std::iter::repeat(0u8).take(50 * triangles as usize));
    v
}

fn glb_bytes(payload_len: usize) -> Vec<u8> {
    let mut v = b"glTF".to_vec();
    v.extend_from_slice(&2u32.to_le_bytes());
    v.extend_from_slice(&((12 + payload_len) as u32).to_le_bytes());
    v.extend((0..payload_len).map(|i| (i % 251) as u8));
    v
}

fn manifest(containers: Value, option: Value) -> Value {
    jvalue!({
        "name": "container_model_default_test",
        "option": option,
        "plugins": {
            "container_factories": {
                "model_container": { "path": "plugin" }
            }
        },
        "containers": containers,
    })
}

fn no_http() -> Value {
    jvalue!({ "http_broker": { "start": false } })
}

fn juiz_error_kind(e: &juiz_sdk::anyhow::Error) -> String {
    match e.chain().find_map(|c| c.downcast_ref::<JuizError>()) {
        Some(JuizError::ContainerModelInvalidError { .. }) => "invalid".to_owned(),
        Some(JuizError::ContainerModelFileNotFoundError { .. }) => "not_found".to_owned(),
        _ => format!("other: {e:#}"),
    }
}

fn info(system: &System, name: &str) -> JuizResult<Value> {
    system.core_broker().lock()?.container_model_info(name)
}

/// conf のインスタンスに `model` が無ければ型の既定、あれば conf の値、`model: null` ならモデル無し。
/// API で作った Container にも型の既定が付く
#[test]
fn container_model_default_of_factory() -> JuizResult<()> {
    crate::common::logger::init();
    let dir = TempDir::new("container_model_default");
    dir.copy_plugin();
    let stl = binary_stl(3);
    dir.write("plugin/models/model_container.stl", &stl);
    let glb = glb_bytes(100);
    dir.write("models/own.glb", &glb);
    let containers = jvalue!([
        { "type_name": "model_container", "name": "c0", "initial_value": 1 },
        { "type_name": "model_container", "name": "c1", "initial_value": 1, "model": { "path": "models/own.glb" } },
        { "type_name": "model_container", "name": "c2", "initial_value": 1, "model": null },
        { "type_name": "model_container", "name": "c3", "namespace": "robot1", "initial_value": 1 },
    ]);
    Ok(System::new(manifest(containers, no_http()))?
        .set_working_dir(&dir.0)
        .setup()?
        .run_and_do_once(|system| {
            // 型の既定 (プラグインのファイルからの相対、manifest の scale・color・origin)
            let i0 = info(system, "c0")?;
            assert_eq!(i0["format"], "stl", "{i0}");
            assert_eq!(i0["size"], stl.len() as u64);
            assert_eq!(i0["sha256"], sha256_hex(&stl));
            assert_eq!(i0["file_name"], "model_container.stl");
            assert_eq!(i0["scale"], 0.001);
            assert_eq!(i0["color"], "#336699");
            assert_eq!(i0["origin"]["translation"]["z"], -0.02, "{i0}");
            // ネームスペースで複製しても同じ
            let i3 = info(system, "robot1/c3")?;
            assert_eq!(i3["sha256"], sha256_hex(&stl), "{i3}");
            // インスタンスの model が丸ごと置き換える (scale などは混ぜない)
            let i1 = info(system, "c1")?;
            assert_eq!(i1["format"], "glb", "{i1}");
            assert_eq!(i1["sha256"], sha256_hex(&glb));
            assert_eq!(i1["scale"], 1.0);
            assert_eq!(i1["color"], "");
            // model: null は既定を使わない
            assert!(info(system, "c2")?.is_null());
            // 中身も取り出せる
            let data = system.core_broker().lock()?.container_model("c0")?;
            assert_eq!(data["info"]["sha256"], sha256_hex(&stl));

            // API で作った Container にも型の既定が付く
            let m = ContainerManifest::new("model_container").name("api0");
            system
                .core_broker()
                .lock_mut()?
                .container_create(&m, CapsuleMap::new())?;
            let ia = info(system, "api0")?;
            assert_eq!(ia["sha256"], sha256_hex(&stl), "{ia}");

            // Factory の profile に manifest に書いた model が載る
            let fp = system
                .core_broker()
                .lock()?
                .worker()
                .store()
                .containers
                .factory("model_container")?
                .lock()?
                .profile_full()?;
            assert_eq!(
                fp["container_factory"]["model"]["path"], "models/model_container.stl",
                "{fp}"
            );
            Ok(())
        })
        .expect("Error in System::run_and_do()"))
}

/// 既定のモデルのファイルが無ければプラグインの読み込みをエラーに、
/// option.model.max_bytes を超えれば Container の作成をエラーにする
#[test]
fn container_model_default_errors() -> JuizResult<()> {
    crate::common::logger::init();
    let containers =
        jvalue!([{ "type_name": "model_container", "name": "c0", "initial_value": 1 }]);
    let setup = |dir: &TempDir, option: Value| -> String {
        match System::new(manifest(containers.clone(), option))
            .and_then(|s| s.set_working_dir(&dir.0).setup())
        {
            Ok(_) => panic!("setup must fail"),
            Err(e) => juiz_error_kind(&e),
        }
    };
    let dir = TempDir::new("container_model_default_errors");
    dir.copy_plugin();
    assert_eq!(setup(&dir, no_http()), "not_found");

    dir.write("plugin/models/model_container.stl", &binary_stl(3));
    let mut small = no_http();
    small["model"] = jvalue!({ "max_bytes": 100 });
    assert_eq!(setup(&dir, small), "invalid");

    dir.write("plugin/models/model_container.stl", b"not an stl");
    assert_eq!(setup(&dir, no_http()), "invalid");
    Ok(())
}

/// Python のデコレータ `@juiz_container(model=...)` で書いた既定のモデル。
/// パスは Python ファイルのあるディレクトリからの相対
#[test]
fn container_model_default_of_python_factory() -> JuizResult<()> {
    crate::common::logger::init();
    let dir = TempDir::new("container_model_default_py");
    let stl = binary_stl(2);
    dir.write("plugin/models/py_model.stl", &stl);
    dir.write(
        "plugin/py_model_container.py",
        br##"from juiz import *

class PyModelContainer:
    def __init__(self, value):
        self.value = value

@juiz_container(model={"path": "models/py_model.stl", "color": "#112233"})
def py_model_container(initial_value: int = 0):
    return PyModelContainer(initial_value)
"##,
    );
    let pyjuiz = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../bindings/pyjuiz");
    let conf = jvalue!({
        "name": "container_model_default_py_test",
        "option": { "http_broker": { "start": false }, "pythonpath": [pyjuiz.to_string_lossy()] },
        "plugins": {
            "container_factories": {
                "py_model_container": { "language": "python", "path": "plugin" }
            }
        },
        "containers": [
            { "type_name": "py_model_container", "name": "p0", "initial_value": 1 },
        ],
    });
    Ok(System::new(conf)?
        .set_working_dir(&dir.0)
        .setup()?
        .run_and_do_once(|system| {
            let i0 = info(system, "p0")?;
            assert_eq!(i0["format"], "stl", "{i0}");
            assert_eq!(i0["sha256"], sha256_hex(&stl));
            assert_eq!(i0["color"], "#112233");
            Ok(())
        })
        .expect("Error in System::run_and_do()"))
}
