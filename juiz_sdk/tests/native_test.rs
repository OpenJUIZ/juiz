//! Capsule::Native (遅延評価、issue #175) のテスト
//!
//! 仕様は docs/spec/lazy-evaluation.md の §3 (API)、§6 (ロックの順序)。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use juiz_sdk::anyhow::anyhow;
use juiz_sdk::image::{DynamicImage, RgbImage};
use juiz_sdk::prelude::*;
use juiz_sdk::value::capsule_converter::*;
use juiz_sdk::value::capsule_ptr_converter::*;
use juiz_sdk::value::converter_error::*;

/// encode した回数を数えるネイティブ値
#[derive(Debug)]
struct Counted {
    value: Value,
    encodes: Arc<AtomicUsize>,
}

impl Counted {
    fn new(value: Value) -> (Self, Arc<AtomicUsize>) {
        let encodes = Arc::new(AtomicUsize::new(0));
        (
            Self {
                value,
                encodes: encodes.clone(),
            },
            encodes,
        )
    }
}

impl NativeValue for Counted {
    fn language(&self) -> &str {
        "test"
    }
    fn kind(&self) -> NativeKind {
        NativeKind::Value
    }
    fn type_name(&self) -> String {
        "Counted".to_owned()
    }
    fn encode(&self) -> JuizResult<EncodedValue> {
        self.encodes.fetch_add(1, Ordering::SeqCst);
        Ok(EncodedValue::Value(self.value.clone()))
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// encode に失敗するネイティブ値
#[derive(Debug)]
struct Broken;

impl NativeValue for Broken {
    fn language(&self) -> &str {
        "test"
    }
    fn kind(&self) -> NativeKind {
        NativeKind::Value
    }
    fn type_name(&self) -> String {
        "Broken".to_owned()
    }
    fn encode(&self) -> JuizResult<EncodedValue> {
        Err(anyhow!("cannot encode Broken"))
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// 画像になるネイティブ値
#[derive(Debug)]
struct NativeImage;

impl NativeValue for NativeImage {
    fn language(&self) -> &str {
        "test"
    }
    fn kind(&self) -> NativeKind {
        NativeKind::Image
    }
    fn type_name(&self) -> String {
        "NativeImage".to_owned()
    }
    fn encode(&self) -> JuizResult<EncodedValue> {
        Ok(EncodedValue::Image(DynamicImage::ImageRgb8(RgbImage::new(
            2, 3,
        ))))
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[test]
fn kind_checks_do_not_encode() -> JuizResult<()> {
    let (v, encodes) = Counted::new(jvalue!({"a": 1}));
    let p = CapsulePtr::from(Capsule::from_native(v));
    assert!(p.is_native()?);
    assert!(p.is_value()?);
    assert!(!p.is_image()?);
    assert!(!p.is_empty()?);
    // Display / Debug も encode しない
    assert!(format!("{p}").contains("Native(test:Counted)"));
    assert!(format!("{p:?}").contains("test:Counted"));
    assert_eq!(encodes.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn reading_encodes_once_and_caches() -> JuizResult<()> {
    let (v, encodes) = Counted::new(jvalue!({"a": 1}));
    let p = CapsulePtr::from(Capsule::from_native(v));
    let q = p.clone();
    assert_eq!(p.lock_as_value(|v| v.clone())?, jvalue!({"a": 1}));
    assert_eq!(q.lock_as_map(|m| m.len())?, 1);
    assert_eq!(capsule_to_value(q.clone())?["__value__"], jvalue!({"a": 1}));
    // 中身は Native のまま
    assert!(p.is_native()?);
    assert_eq!(encodes.load(Ordering::SeqCst), 1);
    // ほかに参照があれば複製して取り出す
    assert_eq!(q.extract_value()?, jvalue!({"a": 1}));
    assert_eq!(encodes.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test]
fn same_language_receiver_gets_native_value() -> JuizResult<()> {
    let (v, encodes) = Counted::new(jvalue!(3));
    let p = CapsulePtr::from(Capsule::from_native(v));
    let cell = p.native_cell()?.expect("must be native");
    let counted = cell.value().as_any().downcast_ref::<Counted>().unwrap();
    assert_eq!(counted.value, jvalue!(3));
    assert_eq!(p.lock_as_native(|n| n.type_name())?, "Counted");
    assert_eq!(encodes.load(Ordering::SeqCst), 0);
    // Native でなければエラー
    assert!(CapsulePtr::from(jvalue!(1)).lock_as_native(|_| ()).is_err());
    assert!(CapsulePtr::from(jvalue!(1)).native_cell()?.is_none());
    Ok(())
}

#[test]
fn capsule_api_on_native() -> JuizResult<()> {
    let (v, encodes) = Counted::new(jvalue!([1, 2]));
    let c = Capsule::from_native(v);
    assert!(c.is_native());
    assert_eq!(c.as_native().unwrap().language(), "test");
    let d = c.clone();
    assert_eq!(c.as_value(), Some(&jvalue!([1, 2])));
    assert_eq!(c.try_as_value()?, Some(&jvalue!([1, 2])));
    assert!(c.as_image().is_none());
    // clone は値を共有するので、encode はまだ 1 回
    assert_eq!(d.as_value(), Some(&jvalue!([1, 2])));
    assert_eq!(encodes.load(Ordering::SeqCst), 1);
    let value: Value = d.try_into()?;
    assert_eq!(value["__value__"], jvalue!([1, 2]));
    assert_eq!(c.to_value(), Some(jvalue!([1, 2])));
    Ok(())
}

#[test]
fn modifying_drops_native_value() -> JuizResult<()> {
    let (v, _) = Counted::new(jvalue!({"a": 1}));
    let p = CapsulePtr::from(Capsule::from_native(v));
    p.lock_modify_as_value(|v| v["b"] = jvalue!(2))?;
    assert!(!p.is_native()?);
    assert_eq!(p.lock_as_value(|v| v.clone())?, jvalue!({"a": 1, "b": 2}));
    Ok(())
}

#[test]
fn image_kind() -> JuizResult<()> {
    let p = CapsulePtr::from(Capsule::from_native(NativeImage));
    assert!(p.is_image()?);
    assert!(!p.is_value()?);
    assert_eq!(p.lock_as_image(|i| i.width())?, 2);
    assert!(p.lock_as_value(|_| ()).is_err());
    assert_eq!(p.extract_image()?.height(), 3);
    Ok(())
}

#[test]
fn encode_failure_is_error_not_panic() -> JuizResult<()> {
    let p = CapsulePtr::from(Capsule::from_native(Broken));
    assert!(p.is_value()?);
    let err = p.lock_as_value(|_| ()).unwrap_err();
    assert!(
        format!("{err:#}").contains("cannot encode Broken"),
        "{err:#}"
    );
    assert!(p.lock_modify_as_value(|_| ()).is_err());
    assert!(capsule_to_value(p.clone()).is_err());
    assert!(p.clone().extract_value().is_err());
    let c = Capsule::from_native(Broken);
    assert!(c.as_value().is_none());
    assert!(c.try_as_value().is_err());
    Ok(())
}

#[test]
fn c_abi_accessors_encode() {
    unsafe {
        let (v, _) = Counted::new(jvalue!(42));
        let mut p = CapsulePtr::from(Capsule::from_native(v));
        let mut i: i64 = 0;
        assert_eq!(capsule_ptr_get_int(&mut p, &mut i), JUIZ_OK);
        assert_eq!(i, 42);
        let inner = capsule_ptr_get_value(&mut p);
        assert!(arc_capsule_is_value(inner));
        assert!(arc_capsule_is_int(inner));
        assert!(!arc_capsule_is_string(inner));

        let (v, _) = Counted::new(jvalue!(7));
        let mut c = Capsule::from_native(v);
        assert!(capsule_is_int(&mut c));
        assert_eq!(capsule_get_int(&mut c, &mut i), JUIZ_OK);
        assert_eq!(i, 7);

        let mut b = Capsule::from_native(Broken);
        assert_eq!(capsule_get_int(&mut b, &mut i), JUIZ_CAPSULE_NO_VALUE);
        let mut a = Arc::new(Mutex::new(Capsule::from_native(Broken)));
        assert!(!arc_capsule_is_int(&mut a));
    }
}

/// GIL の代わりのロック。encode の中で取る
static INTERPRETER: Mutex<()> = Mutex::new(());

/// encode の中でインタプリタのロックを取るネイティブ値 (Python の GIL と同じ)
#[derive(Debug)]
struct NeedsInterpreter {
    started: Mutex<Option<mpsc::Sender<()>>>,
}

impl NativeValue for NeedsInterpreter {
    fn language(&self) -> &str {
        "test"
    }
    fn kind(&self) -> NativeKind {
        NativeKind::Value
    }
    fn type_name(&self) -> String {
        "NeedsInterpreter".to_owned()
    }
    fn encode(&self) -> JuizResult<EncodedValue> {
        if let Some(tx) = self.started.lock().unwrap().take() {
            tx.send(()).unwrap();
        }
        let _interpreter = INTERPRETER.lock().unwrap();
        Ok(EncodedValue::Value(jvalue!("encoded")))
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// encode は CapsulePtr のロックの外で行う (§6)。
/// インタプリタのロックを持ったスレッドが CapsulePtr をロックしても、相互待ちにならない
#[test]
fn encode_runs_outside_capsule_lock() -> JuizResult<()> {
    let (tx, rx) = mpsc::channel();
    let p = CapsulePtr::from(Capsule::from_native(NeedsInterpreter {
        started: Mutex::new(Some(tx)),
    }));
    // インタプリタのロックを先に取っておく (Python の Process が GIL を持っている状態)
    let interpreter = INTERPRETER.lock().unwrap();
    let reader = {
        let p = p.clone();
        thread::spawn(move || p.lock_as_value(|v| v.clone()))
    };
    // 読む側が encode を始めた (インタプリタのロックを待っている) ところで、CapsulePtr をロックする
    rx.recv().unwrap();
    assert!(p.is_value()?);
    assert!(p.native_cell()?.is_some());
    drop(interpreter);
    assert_eq!(reader.join().unwrap()?, jvalue!("encoded"));
    Ok(())
}
