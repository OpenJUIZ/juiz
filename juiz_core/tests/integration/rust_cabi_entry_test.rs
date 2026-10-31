//! juiz_macro が生成する Rust プラグインの C ABI の入口 `<symbol>_cabi` を、ビルドしたプラグインから
//! 直接呼んで確かめるテスト (docs/spec/rust-plugin-abi.md、issue #329)
//!
//! ここでは juiz_core の読み込みを通さず、libloading で開いて入口の表を読み、juiz_sdk::cabi::host の道具で
//! フレームを渡す。juiz_core の読み込み (#330) を通したテストは rust_cabi_load_test にある。
//! プラグインはワークスペースの cargo build で ../target/debug にできている前提 (scripts/test.sh)。

use std::ffi::{c_void, CStr};

use juiz_core::prelude::*;
use juiz_sdk::cabi::host::{collect_bytes, FrameArgs, FrameResult};
use juiz_sdk::cabi::*;
use juiz_sdk::serde_json;

fn plugin_file(name: &str) -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    let file = format!("lib{name}.dylib");
    #[cfg(target_os = "linux")]
    let file = format!("lib{name}.so");
    #[cfg(target_os = "windows")]
    let file = format!("{name}.dll");
    std::path::PathBuf::from("../target/debug").join(file)
}

struct Plugin {
    lib: libloading::Library,
}

impl Plugin {
    fn open(name: &str) -> Plugin {
        let path = plugin_file(name);
        let lib = unsafe { libloading::Library::new(&path) }
            .unwrap_or_else(|e| panic!("can not open {path:?} ({e}). Run cargo build first."));
        Plugin { lib }
    }

    /// `<symbol>_cabi` の表。表はプラグインを開いている間有効
    fn entry(&self, symbol: &str) -> &JuizRustEntry {
        let name = format!("{symbol}{CABI_SYMBOL_SUFFIX}");
        let f: libloading::Symbol<extern "C" fn() -> *const JuizRustEntry> =
            unsafe { self.lib.get(name.as_bytes()) }.unwrap_or_else(|e| panic!("{name}: {e}"));
        let p = f();
        assert!(!p.is_null(), "{name} returned null");
        let e = unsafe { &*p };
        assert_eq!(e.abi_version, 1, "{name}");
        assert_eq!(e.struct_size as usize, std::mem::size_of::<JuizRustEntry>(), "{name}");
        let marker = unsafe { CStr::from_ptr(e.abi_marker) }.to_str().unwrap();
        assert_eq!(marker, ABI_MARKER);
        e
    }
}

fn manifest_json(entry: &JuizRustEntry) -> Value {
    let (code, json) = collect_bytes(|out| (entry.manifest.unwrap())(out));
    assert_eq!(code, JUIZ_OK);
    serde_json::from_slice(&json.unwrap()).unwrap()
}

fn info_json(entry: &JuizRustEntry) -> Value {
    let (code, json) = collect_bytes(|out| (entry.info.unwrap())(out));
    assert_eq!(code, JUIZ_OK);
    serde_json::from_slice(&json.unwrap()).unwrap()
}

fn call(entry: &JuizRustEntry, map: CapsuleMap) -> (i64, FrameResult) {
    let args = FrameArgs::new(&map).unwrap();
    let frame_in = args.frame_in();
    let mut result = FrameResult::new();
    let out = result.frame_out();
    let code = (entry.call.unwrap())(&frame_in, &out);
    (code, result)
}

fn map_of(pairs: &[(&str, Value)]) -> CapsuleMap {
    let mut m = CapsuleMap::new();
    for (k, v) in pairs {
        m.insert((*k).to_owned(), v.clone().into());
    }
    m
}

#[test]
fn process_entry_is_callable_through_c_abi() {
    crate::common::logger::init();
    let plugin = Plugin::open("increment_process");
    let entry = plugin.entry("process_factory");
    assert_eq!(entry.kind, KIND_PROCESS);
    assert!(entry.call.is_some() && entry.create.is_none() && entry.container_call.is_none());

    let manifest: ProcessManifest = serde_json::from_value(manifest_json(entry)).unwrap();
    assert_eq!(manifest.type_name, "increment_process");

    let info = info_json(entry);
    assert!(info["build_info"].as_str().unwrap().contains("rustc="), "{info}");
    assert!(info["native_abi_tag"].as_str().unwrap().starts_with("rust:"), "{info}");

    let (code, result) = call(entry, map_of(&[("arg1", jvalue!(41))]));
    assert_eq!(code, JUIZ_OK, "{:?}", result.error());
    assert_eq!(result.into_capsule_ptr().unwrap().extract_value().unwrap(), jvalue!(42));

    // 引数が無ければ利用者の関数がエラーを返し、文が届く
    let (code, result) = call(entry, CapsuleMap::new());
    assert_eq!(code, JUIZ_RUST_CALL_ERROR);
    assert!(result.error().unwrap().contains("arg1"), "{:?}", result.error());

    // 補助の関数 juiz_rust_abi_version も cdylib から見える (§9.2)
    let v: libloading::Symbol<extern "C" fn() -> u32> =
        unsafe { plugin.lib.get(b"juiz_rust_abi_version") }.unwrap();
    assert_eq!(v(), 1);
}

#[test]
fn container_and_container_process_in_different_plugins() {
    crate::common::logger::init();
    let container_lib = Plugin::open("example_container");
    let process_lib = Plugin::open("example_container_increment");
    let c = container_lib.entry("container_factory");
    let p = process_lib.entry("container_process_factory");
    assert_eq!(c.kind, KIND_CONTAINER);
    assert_eq!(p.kind, KIND_CONTAINER_PROCESS);

    let cm: ContainerManifest = serde_json::from_value(manifest_json(c)).unwrap();
    assert_eq!(cm.type_name, "example_container");
    let pm: ProcessManifest = serde_json::from_value(manifest_json(p)).unwrap();
    assert_eq!(pm.type_name, "example_container_increment");

    // 同じワークスペースでビルドしたので型タグが一致する (§8.2)
    let tag = info_json(c)["container_tag"].as_str().unwrap().to_owned();
    assert_eq!(info_json(p)["container_tag"].as_str().unwrap(), tag);
    assert!(tag.contains("ExampleContainer"), "{tag}");

    let manifest = serde_json::to_vec(&cm.clone().name("c0")).unwrap();
    let args = FrameArgs::new(&map_of(&[("initial_value", jvalue!(10))])).unwrap();
    let frame_in = args.frame_in();
    let mut handle: *mut c_void = std::ptr::null_mut();
    let (code, error) = collect_bytes(|error| {
        (c.create.unwrap())(JuizBytes::from_slice(&manifest), &frame_in, &mut handle, error)
    });
    assert_eq!(code, JUIZ_OK, "{error:?}");
    assert!(!handle.is_null());

    for expected in [13, 16] {
        let args = FrameArgs::new(&map_of(&[("arg1", jvalue!(3))])).unwrap();
        let frame_in = args.frame_in();
        let mut result = FrameResult::new();
        let out = result.frame_out();
        let code = (p.container_call.unwrap())(handle, &frame_in, &out);
        assert_eq!(code, JUIZ_OK, "{:?}", result.error());
        assert_eq!(result.into_capsule_ptr().unwrap().extract_value().unwrap(), jvalue!(expected));
    }
    (c.destroy.unwrap())(handle);
}

#[test]
fn component_entries_and_native_results() {
    crate::common::logger::init();
    let plugin = Plugin::open("native_pose_component");
    let comp = plugin.entry("component_manifest");
    assert_eq!(comp.kind, KIND_COMPONENT);
    assert!(comp.call.is_none() && comp.create.is_none());
    let m = manifest_json(comp);
    assert_eq!(m["type_name"], "native_pose_component", "{m}");
    assert!(info_json(comp).get("native_abi_tag").is_none());

    // Pose2D はネイティブ値で返るが、この Step では encode して辞書で届く (§5.3 の 5)
    let make = plugin.entry("native_pose_make_factory");
    assert_eq!(make.kind, KIND_PROCESS);
    let (code, result) = call(make, map_of(&[("x", jvalue!(1.5))]));
    assert_eq!(code, JUIZ_OK, "{:?}", result.error());
    let v = result.into_capsule_ptr().unwrap().extract_value().unwrap();
    assert_eq!(v["position"]["x"], jvalue!(1.5), "{v}");
}
