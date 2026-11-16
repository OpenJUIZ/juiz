//! juiz_core が Rust のプラグインを C ABI で読み込むことのテスト (docs/spec/rust-plugin-abi.md §11、issue #330)
//!
//! - ホストと違う rustc でビルドしたことにしたプラグイン (../target/fake-rustc/debug、scripts/test.sh が
//!   `-V` だけ偽の版を返す juiz_sdk/test_packages/fake_rustc を RUSTC にして作る) が読めて呼べる
//! - コンテナとコンテナプロセスが違う rustc でビルドされていれば、型タグの照合でエラーになる
//! - プラグインの panic がホストまで届かず、`RustPluginCallError` になる
//! - C ABI の入口を持たない古い形のプラグインも、今の Rust ABI で読める
//! - conf の `abi: rust` で、C ABI の入口があっても Rust ABI で読む

use juiz_core::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;

const FAKE: &str = "../target/fake-rustc/debug";
const HOST: &str = "../target/debug";

fn system_conf(plugins: Value, processes: Value, containers: Value) -> Value {
    jvalue!({
        "name": "rust_cabi_load_test",
        "option": { "http_broker": { "start": false } },
        "plugins": plugins,
        "processes": processes,
        "containers": containers,
    })
}

fn with_system(conf: Value, f: impl FnOnce(&mut System) -> JuizResult<()>) -> JuizResult<()> {
    crate::common::logger::init();
    System::new(conf)?.setup()?.run_and_do_once(f)
}

fn call(system: &System, id: &str, args: Vec<(&str, Value)>) -> JuizResult<CapsulePtr> {
    let id: ProcessIdentifier = id.try_into()?;
    system.core_broker().lock_mut()?.process_call(&id, args.into())
}

fn call_cp(system: &System, id: &str, args: Vec<(&str, Value)>) -> JuizResult<CapsulePtr> {
    let id: ProcessIdentifier = id.try_into()?;
    system
        .core_broker()
        .lock_mut()?
        .container_process_call(&id, args.into())
}

fn int_of(c: CapsulePtr) -> i64 {
    c.lock_as_value(|v| v.as_i64().unwrap()).unwrap()
}

/// エラーの連鎖の中から JuizError を探す
fn juiz_error(e: &juiz_sdk::anyhow::Error) -> Option<&JuizError> {
    e.chain().find_map(|c| c.downcast_ref::<JuizError>())
}

fn process_conf(type_name: &str, path: &str, extra: Value) -> (Value, Value) {
    let mut plugin = jvalue!({ "path": path });
    if let (Some(p), Some(x)) = (plugin.as_object_mut(), extra.as_object()) {
        for (k, v) in x {
            p.insert(k.clone(), v.clone());
        }
    }
    let mut factories = serde_json::Map::new();
    factories.insert(type_name.to_owned(), plugin);
    (
        jvalue!({ "process_factories": factories }),
        jvalue!([{ "type_name": type_name, "name": format!("{type_name}0") }]),
    )
}

#[test]
fn fake_rustc_plugin_is_built_with_another_rustc() {
    // 前提の確認: fake-rustc のプラグインのビルド情報にはホストと違う rustc が入っている
    let path = std::path::Path::new(FAKE).join(juiz_core_plugin_file("increment_process"));
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("can not read {path:?} ({e}). Run scripts/test.sh --prepare-only first."));
    let needle = b"rustc=rustc 0.0.0-fake";
    assert!(
        bytes.windows(needle.len()).any(|w| w == needle),
        "{path:?} is not built with fake_rustc"
    );
}

fn juiz_core_plugin_file(name: &str) -> String {
    #[cfg(target_os = "macos")]
    return format!("lib{name}.dylib");
    #[cfg(target_os = "linux")]
    return format!("lib{name}.so");
    #[cfg(target_os = "windows")]
    return format!("{name}.dll");
}

#[test]
fn plugin_of_another_rustc_is_loaded_through_c_abi() -> JuizResult<()> {
    let (plugins, processes) = process_conf("increment_process", FAKE, jvalue!({}));
    with_system(system_conf(plugins, processes, jvalue!([])), |system| {
        let id = "core://core/process/increment_process0::increment_process";
        assert_eq!(int_of(call(system, id, vec![("arg1", jvalue!(41))])?), 42);
        Ok(())
    })
}

#[test]
fn plugin_of_another_rustc_is_rejected_through_rust_abi() {
    // abi: rust では今までどおり BuildInfo の検査で弾かれる (C ABI で読めることとの対比)
    let (plugins, processes) =
        process_conf("increment_process", FAKE, jvalue!({ "abi": "rust" }));
    crate::common::logger::init();
    let err = System::new(system_conf(plugins, processes, jvalue!([])))
        .and_then(|s| s.setup())
        .err()
        .expect("loading a plugin of another rustc through the Rust ABI must fail");
    assert!(
        matches!(
            juiz_error(&err),
            Some(JuizError::PluginBuildInfoMismatchError { .. })
        ),
        "{err:?}"
    );
}

fn container_conf(container_path: &str, process_path: &str) -> Value {
    system_conf(
        jvalue!({
            "container_factories": {
                "example_container": {
                    "path": container_path,
                    "processes": { "example_container_increment": { "path": process_path } }
                }
            }
        }),
        jvalue!([]),
        jvalue!([{
            "type_name": "example_container",
            "name": "container_a",
            "processes": [ { "type_name": "example_container_increment", "name": "increment_a" } ]
        }]),
    )
}

const INCREMENT_A: &str =
    "core://core/container_process/increment_a::example_container_increment:container_a";

#[test]
fn container_of_another_rustc_is_loaded_through_c_abi() -> JuizResult<()> {
    with_system(container_conf(FAKE, FAKE), |system| {
        let a = int_of(call_cp(system, INCREMENT_A, vec![("arg1", jvalue!(5))])?);
        let b = int_of(call_cp(system, INCREMENT_A, vec![("arg1", jvalue!(7))])?);
        assert_eq!(b - a, 7, "the container keeps its state between calls");
        Ok(())
    })
}

#[test]
fn container_and_process_of_different_rustc_are_rejected_by_type_tag() -> JuizResult<()> {
    with_system(container_conf(FAKE, HOST), |system| {
        let err = call_cp(system, INCREMENT_A, vec![("arg1", jvalue!(1))]).unwrap_err();
        match juiz_error(&err) {
            Some(JuizError::ContainerTypeMismatchError { expected, actual }) => {
                assert!(actual.contains("0.0.0-fake"), "{actual}");
                assert!(!expected.contains("0.0.0-fake"), "{expected}");
            }
            other => panic!("unexpected error {other:?} ({err:#})"),
        }
        Ok(())
    })
}

#[test]
fn panic_in_plugin_does_not_reach_the_host() -> JuizResult<()> {
    let (plugins, processes) = process_conf("panic_proc", HOST, jvalue!({}));
    with_system(system_conf(plugins, processes, jvalue!([])), |system| {
        let id = "core://core/process/panic_proc0::panic_proc";
        let err = call(system, id, vec![("arg1", jvalue!(-1))]).unwrap_err();
        match juiz_error(&err) {
            Some(JuizError::RustPluginCallError {
                symbol, message, ..
            }) => {
                assert_eq!(symbol, "process_factory_cabi.call");
                assert!(message.starts_with("panicked: arg1 is negative (-1)"), "{message}");
            }
            other => panic!("unexpected error {other:?} ({err:#})"),
        }
        // 利用者の関数の Err も文が届く
        let err = call(system, id, vec![("arg1", jvalue!(0))]).unwrap_err();
        match juiz_error(&err) {
            Some(JuizError::RustPluginCallError { message, .. }) => {
                assert!(message.contains("arg1 must not be 0"), "{message}");
            }
            other => panic!("unexpected error {other:?} ({err:#})"),
        }
        // panic の後もホストは動き、プラグインも続けて呼べる
        assert_eq!(int_of(call(system, id, vec![("arg1", jvalue!(3))])?), 6);
        Ok(())
    })
}

#[test]
fn old_plugin_without_c_abi_is_loaded_through_rust_abi() -> JuizResult<()> {
    let (plugins, processes) = process_conf("rust_abi_only_proc", HOST, jvalue!({}));
    with_system(system_conf(plugins, processes, jvalue!([])), |system| {
        let id = "core://core/process/rust_abi_only_proc0::rust_abi_only_proc";
        assert_eq!(int_of(call(system, id, vec![("arg1", jvalue!(1))])?), 101);
        Ok(())
    })
}

#[test]
fn abi_rust_option_loads_through_rust_abi() -> JuizResult<()> {
    let (plugins, processes) =
        process_conf("increment_process", HOST, jvalue!({ "abi": "rust" }));
    with_system(system_conf(plugins, processes, jvalue!([])), |system| {
        let id = "core://core/process/increment_process0::increment_process";
        assert_eq!(int_of(call(system, id, vec![("arg1", jvalue!(1))])?), 2);
        Ok(())
    })
}

#[test]
fn unknown_abi_option_is_an_error() {
    let (plugins, processes) =
        process_conf("increment_process", HOST, jvalue!({ "abi": "cpp" }));
    crate::common::logger::init();
    let err = System::new(system_conf(plugins, processes, jvalue!([])))
        .and_then(|s| s.setup())
        .err()
        .expect("an unknown abi must be an error");
    assert!(
        matches!(juiz_error(&err), Some(JuizError::InvalidSettingError { .. })),
        "{err:?}"
    );
}

#[test]
fn images_pass_through_c_abi() -> JuizResult<()> {
    use juiz_sdk::image::{ImageBuffer, Luma, Rgb};
    let (plugins, processes) = process_conf("cabi_image_proc", HOST, jvalue!({}));
    with_system(system_conf(plugins, processes, jvalue!([])), |system| {
        let id: ProcessIdentifier = "core://core/process/cabi_image_proc0::cabi_image_proc".try_into()?;
        let images = [
            DynamicImage::ImageRgb8(ImageBuffer::from_fn(64, 48, |x, y| {
                Rgb([x as u8, y as u8, (x + y) as u8])
            })),
            DynamicImage::ImageLuma16(ImageBuffer::from_fn(33, 17, |x, y| {
                Luma([(x * 1000 + y) as u16])
            })),
        ];
        for img in images {
            let mut args = CapsuleMap::new();
            args.insert("img".to_owned(), Capsule::from(img.clone()).into());
            let result = system.core_broker().lock_mut()?.process_call(&id, args)?;
            let mut expected = img.clone();
            expected.invert();
            result.lock_as_image(|r| assert_eq!(r, &expected))?;
        }
        Ok(())
    })
}

/// 画像の領域を使い回しても (§5.5、#339)、毎回の戻り値は正しく、呼び手が持ち続ける引数は書き換わらない
#[test]
fn reused_image_buffers_keep_results_and_held_arguments_intact() -> JuizResult<()> {
    use juiz_sdk::image::{ImageBuffer, Rgb, Rgba};
    let (plugins, processes) = process_conf("cabi_image_proc", HOST, jvalue!({}));
    with_system(system_conf(plugins, processes, jvalue!([])), |system| {
        let id: ProcessIdentifier = "core://core/process/cabi_image_proc0::cabi_image_proc".try_into()?;
        let rgb = |seed: u32| {
            DynamicImage::ImageRgb8(ImageBuffer::from_fn(40, 30, move |x, y| {
                Rgb([(x + seed) as u8, (y * seed) as u8, (x ^ y ^ seed) as u8])
            }))
        };
        let rgba = |seed: u32| {
            DynamicImage::ImageRgba8(ImageBuffer::from_fn(40, 30, move |x, y| {
                Rgba([(x * seed) as u8, y as u8, seed as u8, (x + y) as u8])
            }))
        };
        // 同じ大きさ・形式の続け、形式の違うものとの交互
        let inputs = [rgb(1), rgb(2), rgb(3), rgba(4), rgb(5), rgba(6), rgba(7), rgb(8)];
        let mut held = Vec::new();
        let mut results = Vec::new();
        for (i, img) in inputs.iter().enumerate() {
            let arg: CapsulePtr = Capsule::from(img.clone()).into();
            if i % 2 == 0 {
                // 呼び手も持ち続ける引数 (使い回してはいけない)
                held.push((arg.clone(), img.clone()));
            }
            let mut args = CapsuleMap::new();
            args.insert("img".to_owned(), arg);
            let result = system.core_broker().lock_mut()?.process_call(&id, args)?;
            let mut expected = img.clone();
            expected.invert();
            result.lock_as_image(|r| assert_eq!(r, &expected, "result {i}"))?;
            results.push((result, expected));
        }
        for (i, (result, expected)) in results.iter().enumerate() {
            result.lock_as_image(|r| assert_eq!(r, expected, "result {i} after the other calls"))?;
        }
        for (arg, img) in held.iter() {
            arg.lock_as_image(|a| assert_eq!(a, img, "an argument the caller holds"))?;
        }
        Ok(())
    })
}
