//! グルーの単体テスト (docs/spec/rust-plugin-abi.md §14 の Step 1)。
//! ホストの代わりに、引数のフレームを作り、戻り値の書き込み口を用意して入口を呼ぶ

use super::*;
use crate::frame::{read_frame_parts, BodyDesc, FrameHeader, SPARE_IMAGES_MAX};
use image::{ImageBuffer, Rgb};
use std::sync::atomic::{AtomicUsize, Ordering};

/// ホストの書き込み口の代わり。ヘッダを読んで本体の領域を確保する
#[derive(Default)]
struct HostOut {
    header: Option<Vec<u8>>,
    bodies: Vec<Vec<u8>>,
    error: Option<String>,
    error_calls: usize,
    refuse_begin: bool,
    refuse_body: bool,
    /// 本体の長さをわざと間違える
    wrong_len: bool,
}

extern "C" fn host_begin(ctx: *mut c_void, header: JuizBytes) -> i64 {
    let host = unsafe { &mut *(ctx as *mut HostOut) };
    if host.refuse_begin {
        return -1;
    }
    let bytes = unsafe { header.as_slice() }.unwrap().to_vec();
    let h: FrameHeader = match serde_json::from_slice(&bytes) {
        Ok(h) => h,
        Err(_) => return -2,
    };
    host.bodies = h
        .bodies
        .iter()
        .map(|d| vec![0u8; d.body_len().unwrap() as usize])
        .collect();
    host.header = Some(bytes);
    0
}

extern "C" fn host_body(ctx: *mut c_void, index: usize, ptr: *mut *mut u8, len: *mut usize) -> i64 {
    let host = unsafe { &mut *(ctx as *mut HostOut) };
    if host.refuse_body {
        return -1;
    }
    let Some(b) = host.bodies.get_mut(index) else {
        return -3;
    };
    unsafe {
        *ptr = b.as_mut_ptr();
        *len = if host.wrong_len { b.len() + 1 } else { b.len() };
    }
    0
}

extern "C" fn host_error(ctx: *mut c_void, message: JuizBytes) {
    let host = unsafe { &mut *(ctx as *mut HostOut) };
    host.error_calls += 1;
    host.error = Some(String::from_utf8(unsafe { message.as_slice() }.unwrap().to_vec()).unwrap());
}

extern "C" fn sink_write(ctx: *mut c_void, bytes: JuizBytes) {
    let dst = unsafe { &mut *(ctx as *mut Option<Vec<u8>>) };
    assert!(dst.is_none(), "write is called only once");
    *dst = Some(unsafe { bytes.as_slice() }.unwrap().to_vec());
}

fn frame_out(host: &mut HostOut) -> JuizFrameOut {
    JuizFrameOut {
        ctx: host as *mut HostOut as *mut c_void,
        begin: Some(host_begin),
        body: Some(host_body),
        native: None,
        error: Some(host_error),
    }
}

/// 引数のフレーム (ヘッダと本体) を持つ
struct HostArgs {
    header: Vec<u8>,
    bodies: Vec<Vec<u8>>,
    descs: Vec<JuizBytes>,
}

impl HostArgs {
    fn new(map: &CapsuleMap) -> Self {
        let frame = OutgoingFrame::call_request(map).unwrap();
        let mut bodies = Vec::new();
        for i in 0..frame.body_count() {
            let mut b = vec![0u8; frame.body_len(i).unwrap() as usize];
            frame.copy_body_into(i, &mut b).unwrap();
            bodies.push(b);
        }
        Self::raw(frame.header_bytes().unwrap(), bodies)
    }

    fn raw(header: Vec<u8>, bodies: Vec<Vec<u8>>) -> Self {
        let descs = bodies.iter().map(|b| JuizBytes::from_slice(b)).collect();
        HostArgs {
            header,
            bodies,
            descs,
        }
    }

    fn frame_in(&self) -> JuizFrameIn {
        JuizFrameIn {
            header: JuizBytes::from_slice(&self.header),
            bodies: if self.descs.is_empty() {
                std::ptr::null()
            } else {
                self.descs.as_ptr()
            },
            body_count: self.descs.len(),
            natives: std::ptr::null(),
            native_count: 0,
        }
    }
}

/// 戻り値のフレームをカプセルにする (ホストが確保した領域から組み立てる)
fn host_result(host: &HostOut) -> CapsulePtr {
    let bodies: Vec<&[u8]> = host.bodies.iter().map(|b| b.as_slice()).collect();
    read_frame_parts(host.header.as_ref().unwrap(), &bodies, &FrameLimits::default())
        .unwrap()
        .into_capsule_ptr()
        .unwrap()
}

fn call_with(
    args: &HostArgs,
    host: &mut HostOut,
    f: impl Fn(CapsuleMap) -> JuizResult<Capsule>,
) -> i64 {
    let frame_in = args.frame_in();
    let out = frame_out(host);
    call_process(&frame_in, &out, f)
}

fn image(w: u32, h: u32) -> DynamicImage {
    DynamicImage::ImageRgb8(ImageBuffer::from_fn(w, h, |x, y| {
        Rgb([x as u8, y as u8, (x * y) as u8])
    }))
}

fn brighten(mut map: CapsuleMap) -> JuizResult<Capsule> {
    let gain: f64 = map.take("gain")?.extract_value()?.as_f64().unwrap();
    let img = map.take("image")?.extract_image()?;
    let mut rgb = img.to_rgb8();
    for p in rgb.pixels_mut() {
        p.0[0] = (p.0[0] as f64 * gain).min(255.0) as u8;
    }
    let mut c = Capsule::from(DynamicImage::ImageRgb8(rgb));
    c.set_option("note", "brightened");
    Ok(c)
}

#[test]
fn process_call_roundtrips_values_and_images() {
    let mut map = CapsuleMap::new();
    map.insert("gain".to_owned(), jvalue!(2.0).into());
    map.insert("image".to_owned(), image(32, 8).into());
    let args = HostArgs::new(&map);
    // param が無ければヘッダに入れない (§5.1)
    let header: serde_json::Value = serde_json::from_slice(&args.header).unwrap();
    assert!(header.get("param").is_none(), "{header}");
    assert_eq!(header["id"], 0);

    let mut host = HostOut::default();
    assert_eq!(call_with(&args, &mut host, brighten), JUIZ_OK);
    assert_eq!(host.error_calls, 0);
    let got = host_result(&host);
    assert_eq!(got.get_option("note").unwrap(), "brightened");
    let mut expected = image(32, 8).to_rgb8();
    for p in expected.pixels_mut() {
        p.0[0] = (p.0[0] as f64 * 2.0).min(255.0) as u8;
    }
    assert_eq!(got.extract_image().unwrap(), DynamicImage::ImageRgb8(expected));
}

#[test]
fn process_call_keeps_integers_params_and_empty() {
    let mut map = CapsuleMap::new();
    map.set_param("who", "test");
    map.insert("n".to_owned(), jvalue!(3).into());
    map.insert("big".to_owned(), jvalue!((0..2000).collect::<Vec<i64>>()).into());
    let args = HostArgs::new(&map);
    let mut host = HostOut::default();
    let code = call_with(&args, &mut host, |mut m| {
        assert_eq!(m.get_param("who").unwrap(), "test");
        let n = m.take("n")?.extract_value()?;
        assert!(n.is_i64(), "{n}");
        let big = m.take("big")?.extract_value()?;
        assert_eq!(big.as_array().unwrap().len(), 2000);
        assert!(big.as_array().unwrap().iter().all(|v| v.is_i64()));
        Ok(jvalue!(n.as_i64().unwrap() + 1).into())
    });
    assert_eq!(code, JUIZ_OK);
    assert_eq!(host_result(&host).extract_value().unwrap(), jvalue!(4));

    let mut host = HostOut::default();
    assert_eq!(call_with(&args, &mut host, |_| Ok(Capsule::empty())), JUIZ_OK);
    assert!(host_result(&host).is_empty().unwrap());
}

#[test]
fn error_and_panic_become_codes() {
    let args = HostArgs::new(&CapsuleMap::new());

    let mut host = HostOut::default();
    let code = call_with(&args, &mut host, |_| {
        Err(anyhow::anyhow!(JuizError::InvalidValueError {
            message: "bad gain".to_owned()
        }))
    });
    assert_eq!(code, JUIZ_RUST_CALL_ERROR);
    assert_eq!(host.error_calls, 1);
    assert!(host.error.as_ref().unwrap().contains("bad gain"), "{:?}", host.error);
    assert!(host.header.is_none());

    let mut host = HostOut::default();
    let code = call_with(&args, &mut host, |_| panic!("boom {}", 1));
    assert_eq!(code, JUIZ_RUST_PANIC);
    assert_eq!(host.error.as_deref(), Some("panicked: boom 1"));

    let mut host = HostOut::default();
    let code = call_with(&args, &mut host, |_| std::panic::panic_any(42));
    assert_eq!(code, JUIZ_RUST_PANIC);
    assert_eq!(host.error.as_deref(), Some("panicked: <non-string panic payload>"));
}

#[test]
fn bad_frames_are_frame_errors() {
    let never = |_: CapsuleMap| -> JuizResult<Capsule> { panic!("must not be called") };
    let header = |v: serde_json::Value| serde_json::to_vec(&v).unwrap();

    let cases = vec![
        ("not json", HostArgs::raw(b"{{{".to_vec(), vec![])),
        (
            "body count",
            HostArgs::raw(
                header(jvalue!({"id": 0, "kind": "request", "map": {"a": {"body": 0}},
                    "bodies": [{"kind": "array", "dtype": "i64le", "count": 1}]})),
                vec![],
            ),
        ),
        (
            "body length",
            HostArgs::raw(
                header(jvalue!({"id": 0, "kind": "request", "map": {"a": {"body": 0}},
                    "bodies": [{"kind": "array", "dtype": "i64le", "count": 2}]})),
                vec![vec![0u8; 8]],
            ),
        ),
        (
            "padded stride",
            HostArgs::raw(
                header(jvalue!({"id": 0, "kind": "request", "map": {"a": {"body": 0}},
                    "bodies": [{"kind": "image", "width": 3, "height": 2, "format": "rgb8", "stride": 11}]})),
                vec![vec![0u8; 22]],
            ),
        ),
        (
            "shm",
            HostArgs::raw(
                header(jvalue!({"id": 0, "kind": "request", "map": {"a": {"body": 0}},
                    "bodies": [{"kind": "array", "dtype": "i64le", "count": 1,
                        "shm": {"region": "r", "slot": 0, "seq": 2, "len": 8}}]})),
                vec![vec![]],
            ),
        ),
        (
            "unknown body kind",
            HostArgs::raw(
                header(jvalue!({"id": 0, "kind": "request", "map": {},
                    "bodies": [{"kind": "tensor"}]})),
                vec![vec![]],
            ),
        ),
        (
            "response",
            HostArgs::raw(header(jvalue!({"id": 0, "kind": "response", "result": {"empty": true}})), vec![]),
        ),
    ];
    for (name, args) in cases {
        let mut host = HostOut::default();
        assert_eq!(call_with(&args, &mut host, never), JUIZ_RUST_FRAME_ERROR, "{name}");
        assert_eq!(host.error_calls, 1, "{name}");
    }

    // このプラグインの表で作っていない Native の引数は受け付けない (§10.3)
    let args = HostArgs::new(&CapsuleMap::new());
    let mut frame_in = args.frame_in();
    let native = JuizRustNative {
        handle: std::ptr::null_mut(),
        vtable: std::ptr::null(),
        type_tag: JuizBytes::empty(),
        kind: 0,
        owned: 0,
    };
    frame_in.natives = &native;
    frame_in.native_count = 1;
    let mut host = HostOut::default();
    let out = frame_out(&mut host);
    assert_eq!(call_process(&frame_in, &out, never), JUIZ_RUST_FRAME_ERROR);
}

#[test]
fn host_refusal_and_null_pointers() {
    let mut map = CapsuleMap::new();
    map.insert("image".to_owned(), image(4, 4).into());
    let args = HostArgs::new(&map);
    let echo = |mut m: CapsuleMap| -> JuizResult<Capsule> {
        Ok(m.take("image")?.extract_image()?.into())
    };

    for (refuse_begin, refuse_body) in [(true, false), (false, true)] {
        let mut host = HostOut {
            refuse_begin,
            refuse_body,
            ..Default::default()
        };
        assert_eq!(call_with(&args, &mut host, echo), JUIZ_RUST_HOST_REFUSED);
        assert_eq!(host.error_calls, 0, "error is not called when the host refused");
    }

    let mut host = HostOut {
        wrong_len: true,
        ..Default::default()
    };
    assert_eq!(call_with(&args, &mut host, echo), JUIZ_RUST_FRAME_ERROR);

    let mut host = HostOut::default();
    let out = frame_out(&mut host);
    assert_eq!(call_process(std::ptr::null(), &out, echo), JUIZ_RUST_NULL_ARGUMENT);
    let frame_in = args.frame_in();
    assert_eq!(call_process(&frame_in, std::ptr::null(), echo), JUIZ_RUST_NULL_ARGUMENT);
    // begin が無い書き込み口
    let mut host = HostOut::default();
    let mut out = frame_out(&mut host);
    out.begin = None;
    assert_eq!(call_process(&frame_in, &out, echo), JUIZ_RUST_NULL_ARGUMENT);
}

fn collect_sink(f: impl FnOnce(*const JuizBytesSink) -> i64) -> (i64, Option<Vec<u8>>) {
    let mut got: Option<Vec<u8>> = None;
    let sink = JuizBytesSink {
        ctx: &mut got as *mut Option<Vec<u8>> as *mut c_void,
        write: Some(sink_write),
    };
    let code = f(&sink);
    (code, got)
}

#[test]
fn manifest_is_json_and_panic_is_reported() {
    let (code, json) = collect_sink(|out| {
        write_manifest(out, || {
            ProcessManifest::new("brighten")
                .description("d")
                .add_float_arg("gain", "gain", 1.0)
        })
    });
    assert_eq!(code, JUIZ_OK);
    let m: ProcessManifest = serde_json::from_slice(&json.unwrap()).unwrap();
    assert_eq!(m.type_name, "brighten");
    assert_eq!(m.arguments.len(), 1);

    let (code, msg) = collect_sink(|out| {
        write_manifest(out, || -> ProcessManifest { panic!("default value does not match") })
    });
    assert_eq!(code, JUIZ_RUST_PANIC);
    assert_eq!(
        String::from_utf8(msg.unwrap()).unwrap(),
        "panicked: default value does not match"
    );

    assert_eq!(
        write_manifest(std::ptr::null(), || ProcessManifest::new("x")),
        JUIZ_RUST_NULL_ARGUMENT
    );
}

#[test]
fn info_has_build_info_and_tags() {
    let (code, json) = collect_sink(process_info);
    assert_eq!(code, JUIZ_OK);
    let info: serde_json::Value = serde_json::from_slice(&json.unwrap()).unwrap();
    let build = info["build_info"].as_str().unwrap();
    assert!(build.starts_with(&format!("juiz_sdk={};rustc=rustc ", env!("CARGO_PKG_VERSION"))), "{build}");
    assert_eq!(info["native_abi_tag"], crate::types::rust_abi_tag());
    assert!(info.get("container_tag").is_none());

    let (_, json) = collect_sink(component_info);
    let info: serde_json::Value = serde_json::from_slice(&json.unwrap()).unwrap();
    assert!(info.get("native_abi_tag").is_none());

    let (_, json) = collect_sink(|out| container_info(out, new_counter));
    let info: serde_json::Value = serde_json::from_slice(&json.unwrap()).unwrap();
    assert_eq!(info["container_tag"], container_tag::<Counter>());
    let (_, json) = collect_sink(|out| container_process_info(out, add));
    let info: serde_json::Value = serde_json::from_slice(&json.unwrap()).unwrap();
    assert_eq!(info["container_tag"], container_tag::<Counter>());
}

static DROPPED: AtomicUsize = AtomicUsize::new(0);

struct Counter {
    value: i64,
}

impl Drop for Counter {
    fn drop(&mut self) {
        DROPPED.fetch_add(1, Ordering::SeqCst);
    }
}

fn new_counter(mut map: CapsuleMap) -> JuizResult<Box<Counter>> {
    let v = map.take("initial_value")?.extract_value()?.as_i64().unwrap();
    if v < 0 {
        return Err(anyhow::anyhow!("initial_value must not be negative"));
    }
    Ok(Box::new(Counter { value: v }))
}

fn add(container: &mut ContainerImpl<Counter>, mut map: CapsuleMap) -> JuizResult<Capsule> {
    let d = map.take("arg1")?.extract_value()?.as_i64().unwrap();
    container.value += d;
    Ok(jvalue!(container.value).into())
}

fn create_with(initial: i64, handle: &mut *mut c_void) -> (i64, Option<Vec<u8>>) {
    let manifest = ContainerManifest::new("counter").name("c0");
    let json = serde_json::to_vec(&manifest).unwrap();
    let mut map = CapsuleMap::new();
    map.insert("initial_value".to_owned(), jvalue!(initial).into());
    let args = HostArgs::new(&map);
    let frame_in = args.frame_in();
    collect_sink(|error| {
        create_container(JuizBytes::from_slice(&json), &frame_in, handle, error, new_counter)
    })
}

#[test]
fn container_create_call_destroy() {
    let before = DROPPED.load(Ordering::SeqCst);
    let mut handle: *mut c_void = std::ptr::null_mut();
    let (code, error) = create_with(10, &mut handle);
    assert_eq!(code, JUIZ_OK, "{error:?}");
    assert!(!handle.is_null());

    for expected in [13, 16] {
        let mut map = CapsuleMap::new();
        map.insert("arg1".to_owned(), jvalue!(3).into());
        let args = HostArgs::new(&map);
        let frame_in = args.frame_in();
        let mut host = HostOut::default();
        let out = frame_out(&mut host);
        assert_eq!(call_container_process(handle, &frame_in, &out, add), JUIZ_OK);
        assert_eq!(host_result(&host).extract_value().unwrap(), jvalue!(expected));
    }
    // コンテナの名前は manifest_json のもの
    let c = unsafe { &*(handle as *const ContainerImpl<Counter>) };
    assert_eq!(c.identifier(), ContainerManifest::new("counter").name("c0").identifier().unwrap());

    destroy_container(handle, new_counter);
    assert_eq!(DROPPED.load(Ordering::SeqCst), before + 1);
    destroy_container(std::ptr::null_mut(), new_counter);

    // 作成関数のエラー
    let mut handle: *mut c_void = std::ptr::null_mut();
    let (code, error) = create_with(-1, &mut handle);
    assert_eq!(code, JUIZ_RUST_CALL_ERROR);
    assert!(String::from_utf8(error.unwrap()).unwrap().contains("must not be negative"));
    assert!(handle.is_null());

    // manifest が読めない
    let mut handle: *mut c_void = std::ptr::null_mut();
    let args = HostArgs::new(&CapsuleMap::new());
    let frame_in = args.frame_in();
    let (code, _) = collect_sink(|error| {
        create_container(JuizBytes::from_slice(b"[]"), &frame_in, &mut handle, error, new_counter)
    });
    assert_eq!(code, JUIZ_RUST_FRAME_ERROR);

    // null の handle
    let mut host = HostOut::default();
    let out = frame_out(&mut host);
    assert_eq!(
        call_container_process(std::ptr::null_mut(), &frame_in, &out, add),
        JUIZ_RUST_NULL_ARGUMENT
    );
}

#[test]
fn container_tag_has_rustc_sdk_type_size_align() {
    let tag = container_tag::<Counter>();
    assert!(tag.starts_with(&format!("rust:{}:{}:", env!("JUIZ_SDK_RUSTC_VERSION"), env!("CARGO_PKG_VERSION"))), "{tag}");
    assert!(tag.contains("ContainerImpl<"), "{tag}");
    assert!(tag.contains("Counter>"), "{tag}");
    assert!(tag.ends_with(&format!(
        ":{}:{}",
        std::mem::size_of::<ContainerImpl<Counter>>(),
        std::mem::align_of::<ContainerImpl<Counter>>()
    )));
    assert_ne!(tag, container_tag::<i64>());
}

extern "C" fn dummy_manifest(_: *const JuizBytesSink) -> i64 {
    0
}
extern "C" fn dummy_call(_: *const JuizFrameIn, _: *const JuizFrameOut) -> i64 {
    0
}

#[test]
fn entry_table_layout() {
    static ENTRY: JuizRustEntry = JuizRustEntry::process(dummy_manifest, dummy_manifest, dummy_call);
    assert_eq!(ENTRY.abi_version, 1);
    assert_eq!(ENTRY.kind, KIND_PROCESS);
    assert_eq!(ENTRY.reserved, 0);
    assert_eq!(ENTRY.struct_size as usize, std::mem::size_of::<JuizRustEntry>());
    assert!(ENTRY.call.is_some() && ENTRY.create.is_none() && ENTRY.container_call.is_none());
    let marker = unsafe { std::ffi::CStr::from_ptr(ENTRY.abi_marker) };
    assert_eq!(marker.to_str().unwrap(), ABI_MARKER);
    assert_eq!(juiz_rust_abi_version(), ABI_VERSION);
    // 4 つの u32、7 つのポインタ (C の構造体と同じ並び)
    assert_eq!(
        std::mem::size_of::<JuizRustEntry>(),
        16 + 7 * std::mem::size_of::<usize>()
    );
    assert_eq!(std::mem::size_of::<JuizBytes>(), 2 * std::mem::size_of::<usize>());
    assert_eq!(error_code_name(JUIZ_RUST_PANIC), "JUIZ_RUST_PANIC");
    assert_eq!(error_code_name(-1), "unknown");
}

#[test]
fn body_desc_len_is_used_by_host() {
    // ホストがヘッダから本体の長さを決められる (§5.3 の 2)
    let d = BodyDesc::Image {
        width: 4,
        height: 2,
        format: "rgb8".to_owned(),
        stride: 12,
        shm: None,
    };
    assert_eq!(d.body_len().unwrap(), 24);
}

// ---- Native (§10) ----

static NATIVE_DROPS: AtomicUsize = AtomicUsize::new(0);
static NATIVE_ENCODES: AtomicUsize = AtomicUsize::new(0);

/// テスト用の Native。encode と drop を数える
#[derive(Debug)]
struct Counted(i64);

impl Drop for Counted {
    fn drop(&mut self) {
        NATIVE_DROPS.fetch_add(1, Ordering::SeqCst);
    }
}

impl NativeValue for Counted {
    fn language(&self) -> &str {
        "rust"
    }
    fn kind(&self) -> NativeKind {
        NativeKind::Value
    }
    fn type_name(&self) -> String {
        "test.Counted".to_owned()
    }
    fn encode(&self) -> JuizResult<EncodedValue> {
        NATIVE_ENCODES.fetch_add(1, Ordering::SeqCst);
        Ok(EncodedValue::Value(jvalue!({ "n": self.0 })))
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn into_any(self: Box<Self>) -> Option<Box<dyn Any>> {
        Some(self)
    }
}

/// native の口も持つ書き込み口の代わり
#[derive(Default)]
struct NativeHost {
    inner: HostOut,
    natives: Vec<JuizRustNative>,
    tags: Vec<Vec<u8>>,
    refuse_native: bool,
}

extern "C" fn host_native(ctx: *mut c_void, index: usize, native: JuizRustNative) -> i64 {
    let host = unsafe { &mut *(ctx as *mut NativeHost) };
    if host.refuse_native || index != host.natives.len() {
        return -1;
    }
    host.tags.push(unsafe { native.type_tag.as_slice() }.unwrap().to_vec());
    host.natives.push(native);
    0
}

extern "C" fn native_host_begin(ctx: *mut c_void, header: JuizBytes) -> i64 {
    let host = unsafe { &mut *(ctx as *mut NativeHost) };
    host_begin(&mut host.inner as *mut HostOut as *mut c_void, header)
}

extern "C" fn native_host_body(ctx: *mut c_void, index: usize, ptr: *mut *mut u8, len: *mut usize) -> i64 {
    let host = unsafe { &mut *(ctx as *mut NativeHost) };
    host_body(&mut host.inner as *mut HostOut as *mut c_void, index, ptr, len)
}

extern "C" fn native_host_error(ctx: *mut c_void, message: JuizBytes) {
    let host = unsafe { &mut *(ctx as *mut NativeHost) };
    host_error(&mut host.inner as *mut HostOut as *mut c_void, message)
}

fn native_frame_out(host: &mut NativeHost) -> JuizFrameOut {
    JuizFrameOut {
        ctx: host as *mut NativeHost as *mut c_void,
        begin: Some(native_host_begin),
        body: Some(native_host_body),
        native: Some(host_native),
        error: Some(native_host_error),
    }
}

fn make_counted(_m: CapsuleMap) -> JuizResult<Capsule> {
    let mut c = Capsule::from_native(Counted(7));
    c.set_option("note", "native");
    Ok(c)
}

/// make_counted を native の口付きで呼び、受け取った Native を返す
fn call_native_result() -> (NativeHost, JuizRustNative) {
    let args = HostArgs::new(&CapsuleMap::new());
    let mut host = NativeHost::default();
    let out = native_frame_out(&mut host);
    assert_eq!(call_process(&args.frame_in(), &out, make_counted), JUIZ_OK);
    assert_eq!(host.natives.len(), 1);
    let n = host.natives[0];
    (host, n)
}

fn encode_native(n: &JuizRustNative) -> CapsulePtr {
    let mut host = HostOut::default();
    let out = frame_out(&mut host);
    let encode = unsafe { (*n.vtable).encode }.unwrap();
    assert_eq!(encode(n.handle, &out), JUIZ_OK);
    host_result(&host)
}

fn destroy_native(n: &JuizRustNative) {
    let destroy = unsafe { (*n.vtable).destroy }.unwrap();
    destroy(n.handle);
}

#[test]
fn native_results_are_passed_as_handles_and_encoded_through_the_table() {
    let _guard = NATIVE_LOCK.lock().unwrap();
    let drops = NATIVE_DROPS.load(Ordering::SeqCst);
    let (host, n) = call_native_result();
    // ヘッダは result: {"native": 0}、オプションも付く
    let header: serde_json::Value =
        serde_json::from_slice(host.inner.header.as_ref().unwrap()).unwrap();
    assert_eq!(header["result"], jvalue!({ "native": 0 }));
    assert_eq!(header["option"]["note"], "native");
    assert_eq!(n.kind, 0);
    assert_eq!(n.owned, 1);
    let tag: serde_json::Value = serde_json::from_slice(&host.tags[0]).unwrap();
    assert_eq!(tag, jvalue!({ "name": "test.Counted" }));
    let vt = unsafe { &*n.vtable };
    assert_eq!(vt.abi_version, ABI_VERSION);
    assert_eq!(vt.struct_size as usize, std::mem::size_of::<JuizRustNativeVtable>());

    // encode は共通形式のフレームを書く。プラグインの側では結果を覚えない
    let encodes = NATIVE_ENCODES.load(Ordering::SeqCst);
    assert_eq!(encode_native(&n).extract_value().unwrap(), jvalue!({ "n": 7 }));
    assert_eq!(encode_native(&n).extract_value().unwrap(), jvalue!({ "n": 7 }));
    assert_eq!(NATIVE_ENCODES.load(Ordering::SeqCst), encodes + 2);
    assert_eq!(NATIVE_DROPS.load(Ordering::SeqCst), drops);
    destroy_native(&n);
    assert_eq!(NATIVE_DROPS.load(Ordering::SeqCst), drops + 1);
}

#[test]
fn native_result_is_encoded_without_native_slot_and_freed_when_refused() {
    let _guard = NATIVE_LOCK.lock().unwrap();
    // native の口が無いホストには encode して書く
    let args = HostArgs::new(&CapsuleMap::new());
    let mut host = HostOut::default();
    assert_eq!(call_with(&args, &mut host, make_counted), JUIZ_OK);
    assert_eq!(host_result(&host).extract_value().unwrap(), jvalue!({ "n": 7 }));

    // ホストが native を断ったら、プラグインが解放する
    let drops = NATIVE_DROPS.load(Ordering::SeqCst);
    let mut host = NativeHost {
        refuse_native: true,
        ..Default::default()
    };
    let out = native_frame_out(&mut host);
    assert_eq!(call_process(&args.frame_in(), &out, make_counted), JUIZ_RUST_HOST_REFUSED);
    assert_eq!(NATIVE_DROPS.load(Ordering::SeqCst), drops + 1);
}

/// 引数に Native を 1 つ渡して f を呼ぶ
fn call_with_native_arg(
    native: JuizRustNative,
    f: impl Fn(CapsuleMap) -> JuizResult<Capsule>,
) -> (i64, HostOut) {
    let header = serde_json::to_vec(&jvalue!({
        "id": 0, "kind": "request", "map": { "v": { "native": 0 } }
    }))
    .unwrap();
    let args = HostArgs::raw(header, vec![]);
    let mut frame_in = args.frame_in();
    frame_in.natives = &native;
    frame_in.native_count = 1;
    let mut host = HostOut::default();
    let out = frame_out(&mut host);
    let code = call_process(&frame_in, &out, f);
    (code, host)
}

/// 引数の Native が Counted のまま届き、ほかに参照が無ければ移せることを返す
fn read_counted(mut m: CapsuleMap) -> JuizResult<Capsule> {
    let c = m.take("v")?;
    let cell = c.native_cell()?.expect("the argument is native");
    assert!(cell.value().as_any().is::<Counted>());
    drop(cell);
    let moved = match c.try_into_capsule() {
        Ok(c) => c.into_native_cell().ok().map(|cell| cell.try_into_value().is_ok()),
        Err(_) => None,
    };
    Ok(jvalue!(moved == Some(true)).into())
}

#[test]
fn owned_native_arguments_move_and_shared_ones_are_cloned() {
    let _guard = NATIVE_LOCK.lock().unwrap();
    let drops = NATIVE_DROPS.load(Ordering::SeqCst);
    let encodes = NATIVE_ENCODES.load(Ordering::SeqCst);

    // owned = 1: 持ち主がプラグインに移り、ほかに参照が無いので移せる。呼び出しの後に解放される
    let (_, n) = call_native_result();
    let (code, host) = call_with_native_arg(JuizRustNative { owned: 1, ..n }, read_counted);
    assert_eq!(code, JUIZ_OK);
    assert_eq!(host_result(&host).extract_value().unwrap(), jvalue!(true));
    assert_eq!(NATIVE_DROPS.load(Ordering::SeqCst), drops + 1);

    // owned = 0: ホストが持ち続けるので、プラグインは複製 (Arc) して使い、移せない
    let (_, n) = call_native_result();
    let (code, host) = call_with_native_arg(JuizRustNative { owned: 0, ..n }, read_counted);
    assert_eq!(code, JUIZ_OK);
    assert_eq!(host_result(&host).extract_value().unwrap(), jvalue!(false));
    assert_eq!(NATIVE_DROPS.load(Ordering::SeqCst), drops + 1);
    destroy_native(&n);
    assert_eq!(NATIVE_DROPS.load(Ordering::SeqCst), drops + 2);
    // どちらも encode しない
    assert_eq!(NATIVE_ENCODES.load(Ordering::SeqCst), encodes);
}

#[test]
fn owned_native_arguments_are_freed_when_the_call_fails() {
    let _guard = NATIVE_LOCK.lock().unwrap();
    let drops = NATIVE_DROPS.load(Ordering::SeqCst);
    // 利用者の関数がエラー
    let (_, n) = call_native_result();
    let (code, _) = call_with_native_arg(JuizRustNative { owned: 1, ..n }, |_| {
        Err(anyhow::anyhow!("fails"))
    });
    assert_eq!(code, JUIZ_RUST_CALL_ERROR);
    assert_eq!(NATIVE_DROPS.load(Ordering::SeqCst), drops + 1);

    // ヘッダが読めない (Native を受け取ってからフレームを読む)
    let (_, n) = call_native_result();
    let native = JuizRustNative { owned: 1, ..n };
    let args = HostArgs::raw(b"not json".to_vec(), vec![]);
    let mut frame_in = args.frame_in();
    frame_in.natives = &native;
    frame_in.native_count = 1;
    let mut host = HostOut::default();
    let out = frame_out(&mut host);
    assert_eq!(call_process(&frame_in, &out, never_called), JUIZ_RUST_FRAME_ERROR);
    assert_eq!(NATIVE_DROPS.load(Ordering::SeqCst), drops + 2);

    // 別の表 (別のプラグイン) の Native は受け付けず、owned = 1 ならその表の destroy で解放する
    static FOREIGN_DESTROYS: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn foreign_destroy(_h: *mut c_void) {
        FOREIGN_DESTROYS.fetch_add(1, Ordering::SeqCst);
    }
    static FOREIGN: JuizRustNativeVtable = JuizRustNativeVtable {
        abi_version: ABI_VERSION,
        struct_size: std::mem::size_of::<JuizRustNativeVtable>() as u32,
        encode: None,
        destroy: Some(foreign_destroy),
    };
    let mut dummy = 0u8;
    let foreign = JuizRustNative {
        handle: &mut dummy as *mut u8 as *mut c_void,
        vtable: &FOREIGN,
        type_tag: JuizBytes::empty(),
        kind: 0,
        owned: 1,
    };
    let (code, host) = call_with_native_arg(foreign, never_called);
    assert_eq!(code, JUIZ_RUST_FRAME_ERROR);
    assert!(host.error.unwrap().contains("not made by this plugin"));
    assert_eq!(FOREIGN_DESTROYS.load(Ordering::SeqCst), 1);
}

fn never_called(_m: CapsuleMap) -> JuizResult<Capsule> {
    panic!("must not be called")
}

/// Native の数を数えるテストを同時に走らせない
static NATIVE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn echo_image(mut map: CapsuleMap) -> JuizResult<Capsule> {
    Ok(map.take("image")?.extract_image()?.into())
}

thread_local! {
    static SEEN_ARG_PTR: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// 引数の画像の画素の先頭を覚えてから返す
fn echo_image_recording_ptr(mut map: CapsuleMap) -> JuizResult<Capsule> {
    let img = map.take("image")?.extract_image()?;
    SEEN_ARG_PTR.with(|p| p.set(img.as_bytes().as_ptr() as usize));
    Ok(img.into())
}

fn spare_ptrs() -> Vec<(u32, u32, usize)> {
    SPARE_IMAGES.with(|s| {
        s.borrow()
            .iter()
            .map(|i| (i.width(), i.height(), i.as_bytes().as_ptr() as usize))
            .collect()
    })
}

/// 書き終わった戻り値の画像を取っておき、次に同じ大きさ・形式の引数を読むときにその領域を使う (§5.5、#339)
#[test]
fn result_image_buffer_is_reused_for_the_next_argument() {
    SPARE_IMAGES.with(|s| s.borrow_mut().clear());
    let call = |img: DynamicImage, f: fn(CapsuleMap) -> JuizResult<Capsule>| {
        let mut map = CapsuleMap::new();
        map.insert("image".to_owned(), img.into());
        let args = HostArgs::new(&map);
        let mut host = HostOut::default();
        assert_eq!(call_with(&args, &mut host, f), JUIZ_OK);
        host_result(&host).extract_image().unwrap()
    };
    assert_eq!(call(image(16, 8), echo_image), image(16, 8));
    let spares = spare_ptrs();
    assert_eq!(spares.len(), 1);
    assert_eq!((spares[0].0, spares[0].1), (16, 8));

    // 中身の違う同じ大きさの画像: 取っておいた領域に読み、正しい中身を返す
    let other = DynamicImage::ImageRgb8(ImageBuffer::from_fn(16, 8, |x, y| Rgb([200, x as u8, y as u8])));
    assert_eq!(call(other.clone(), echo_image_recording_ptr), other);
    assert_eq!(SEEN_ARG_PTR.with(|p| p.get()), spares[0].2, "the spare buffer is used");

    // 大きさが違えば新しく確保し、取っておいたものは上限まで残す
    assert_eq!(call(image(8, 4), echo_image), image(8, 4));
    assert_eq!(spare_ptrs().len(), 2);
    for i in 0..SPARE_IMAGES_MAX + 2 {
        let img = image(4 + i as u32, 4);
        assert_eq!(call(img.clone(), echo_image), img);
    }
    assert_eq!(spare_ptrs().len(), SPARE_IMAGES_MAX);
    SPARE_IMAGES.with(|s| s.borrow_mut().clear());
}
