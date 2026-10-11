//! Capsule / CapsulePtr / CapsuleMap のテスト

use juiz_sdk::image::{DynamicImage, RgbImage};
use juiz_sdk::prelude::*;
use juiz_sdk::value::CapsuleValue;

fn small_image() -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::new(2, 3))
}

#[test]
fn capsule_value_variants() {
    let v = CapsuleValue::from(jvalue!(1));
    assert!(v.is_value());
    assert!(!v.is_empty());
    assert!(!v.is_image());
    assert_eq!(v.as_value(), Some(&jvalue!(1)));
    assert!(v.as_image().is_none());
    assert_eq!(format!("{}", v), "Value(1)");
    assert_eq!(v.clone().to_value(), Some(jvalue!(1)));
    assert!(v.to_image().is_none());

    let mut v = CapsuleValue::from(jvalue!({"a": 1}));
    v.as_value_mut().unwrap()["a"] = jvalue!(2);
    assert_eq!(v.as_value(), Some(&jvalue!({"a": 2})));

    let img = CapsuleValue::from(small_image());
    assert!(img.is_image());
    assert!(!img.is_value());
    assert!(img.as_value().is_none());
    assert_eq!(format!("{}", img), "DynamicImage");
    assert_eq!(img.as_image().unwrap().width(), 2);
    assert!(img.clone().to_value().is_none());
    assert_eq!(img.to_image().unwrap().height(), 3);

    let empty = CapsuleValue::Empty(());
    assert!(empty.is_empty());
    assert_eq!(format!("{}", empty), "Empty");
    let mut empty = empty;
    assert!(empty.as_value_mut().is_none());
}

#[test]
fn capsule_from_primitives() {
    assert_eq!(Capsule::from(true).as_value(), Some(&jvalue!(true)));
    assert_eq!(Capsule::from(3u64).as_value(), Some(&jvalue!(3)));
    assert_eq!(Capsule::from(-3i64).as_value(), Some(&jvalue!(-3)));
    assert_eq!(Capsule::from(1.5f64).as_value(), Some(&jvalue!(1.5)));
    assert_eq!(Capsule::from("s").as_value(), Some(&jvalue!("s")));
    assert_eq!(
        Capsule::from("s".to_owned()).as_value(),
        Some(&jvalue!("s"))
    );
    let mut map = serde_json::Map::new();
    map.insert("k".to_owned(), jvalue!(1));
    assert_eq!(Capsule::from(map).as_value(), Some(&jvalue!({"k": 1})));
    assert_eq!(
        Capsule::from(vec![jvalue!(1), jvalue!(2)]).as_value(),
        Some(&jvalue!([1, 2]))
    );
}

#[test]
fn capsule_options_and_value_round_trip() -> JuizResult<()> {
    let mut c = Capsule::from(jvalue!(42));
    c.set_option("unit", "mm").set_option("frame", "base");
    c.set_function_name("measure");
    assert_eq!(c.get_option("unit").map(|s| s.as_str()), Some("mm"));
    assert_eq!(
        c.get_option("function_name").map(|s| s.as_str()),
        Some("measure")
    );
    assert!(c.get_option("missing").is_none());
    assert_eq!(c.get_options().len(), 3);
    assert!(format!("{}", c).contains("unit:mm"));

    // Capsule -> Value は __value__ と __option__ にくるまれ、Value -> Capsule で元に戻る
    let v: Value = c.try_into()?;
    assert_eq!(v["__value__"], jvalue!(42));
    assert_eq!(v["__option__"]["unit"], jvalue!("mm"));
    let back = Capsule::from(v);
    assert_eq!(back.as_value(), Some(&jvalue!(42)));
    assert_eq!(back.get_option("frame").map(|s| s.as_str()), Some("base"));
    Ok(())
}

#[test]
fn capsule_from_object_without_envelope_keeps_object() {
    // __value__ だけ、あるいは __option__ だけのオブジェクトはそのまま値として扱う
    let c = Capsule::from(jvalue!({"__value__": 1}));
    assert_eq!(c.as_value(), Some(&jvalue!({"__value__": 1})));
    assert!(c.get_options().is_empty());
}

#[test]
fn capsule_empty_and_image() {
    let e = Capsule::empty();
    assert!(e.is_empty());
    assert!(!e.is_value());
    assert!(Value::try_from(e).is_err());

    let mut img = Capsule::from(small_image());
    assert!(img.is_image());
    assert!(img.as_image().is_some());
    assert!(img.as_value_mut().is_none());
    assert!(Value::try_from(img.clone()).is_err());
    assert!(img.clone().to_value().is_none());
    assert_eq!(img.to_image().unwrap().width(), 2);
}

#[test]
fn capsule_ptr_basic_accessors() -> JuizResult<()> {
    let p = CapsulePtr::new();
    assert!(p.is_empty()?);
    assert!(!p.is_value()?);
    assert!(!p.is_image()?);
    assert!(p.lock_as_value(|_| ()).is_err());

    let mut p: CapsulePtr = jvalue!({"a": 1}).into();
    assert!(p.is_value()?);
    p.set_function_name("f")?;
    p.set_class_name("Process")?;
    assert_eq!(p.get_function_name()?, "f");
    assert_eq!(p.get_class_name()?, "Process");
    assert!(p.get_option("missing").is_err());
    assert!(format!("{}", p).starts_with("CapsulePtr("));

    assert_eq!(p.lock_as_value(|v| v["a"].as_i64())?, Some(1));
    assert_eq!(p.lock_as_map(|m| m.len())?, 1);
    assert!(p.lock_as_str(|_| ()).is_err());
    p.lock_modify_as_value(|v| v["a"] = jvalue!(5))?;
    assert_eq!(
        p.lock_as_value_and_opt(|v, opt| (v["a"].clone(), opt.get("function_name").cloned()))?,
        (jvalue!(5), Some("f".to_owned()))
    );

    let s: CapsulePtr = jvalue!("text").into();
    assert_eq!(s.lock_as_str(|s| s.len())?, 4);
    assert!(s.lock_as_map(|_| ()).is_err());

    let img: CapsulePtr = small_image().into();
    assert!(img.lock_as_str(|_| ()).is_err());
    assert!(img.lock_as_map(|_| ()).is_err());
    assert_eq!(img.lock_as_image(|i| i.width())?, 2);
    Ok(())
}

#[test]
fn capsule_ptr_unwraps_envelope_value() -> JuizResult<()> {
    let p: CapsulePtr = jvalue!({"__value__": 7, "__option__": {}}).into();
    assert_eq!(p.lock_as_value(|v| v.clone())?, jvalue!(7));
    let p = value_to_capsule(jvalue!(8));
    assert_eq!(p.lock_as_value(|v| v.clone())?, jvalue!(8));
    Ok(())
}

#[test]
fn capsule_ptr_to_value_and_back() -> JuizResult<()> {
    let mut p: CapsulePtr = jvalue!(3).into();
    p.set_option("k", "v")?;
    let v = capsule_to_value(p.clone())?;
    assert_eq!(v, jvalue!({"__value__": 3, "__option__": {"k": "v"}}));
    let v2: Value = p.try_into()?;
    assert_eq!(v2, v);
    Ok(())
}

#[test]
fn capsule_ptr_replace_shares_and_moves_value() -> JuizResult<()> {
    let a: CapsulePtr = jvalue!(1).into();
    let alias = a.clone();
    let b: CapsulePtr = jvalue!(2).into();
    a.replace(b.clone());
    // クローンは同じ実体を指しているので置換が見える
    assert_eq!(alias.lock_as_value(|v| v.clone())?, jvalue!(2));
    // 置換元は値を取り出されて空になる
    assert!(b.is_empty()?);

    let mut c = CapsulePtr::new();
    c.replace_with_value(jvalue!("x"));
    assert_eq!(c.lock_as_str(|s| s.to_owned())?, "x");
    Ok(())
}

#[test]
fn capsule_ptr_extract_value_and_image() -> JuizResult<()> {
    // 唯一の所有者なら中身をそのまま取り出す
    let p: CapsulePtr = jvalue!({"a": 1}).into();
    assert_eq!(p.extract_value()?, jvalue!({"a": 1}));
    // 共有されていればクローンを返す
    let p: CapsulePtr = jvalue!(5).into();
    let _keep = p.clone();
    assert_eq!(p.extract_value()?, jvalue!(5));

    let img: CapsulePtr = small_image().into();
    assert_eq!(img.extract_image()?.width(), 2);
    let img: CapsulePtr = small_image().into();
    let _keep = img.clone();
    assert_eq!(img.extract_image()?.height(), 3);
    // 共有されている値を画像として取り出そうとするとエラー
    let v: CapsulePtr = jvalue!(1).into();
    let _keep = v.clone();
    assert!(v.extract_image().is_err());

    let img: CapsulePtr = small_image().into();
    let d: DynamicImage = img.try_into()?;
    assert_eq!(d.width(), 2);
    Ok(())
}

#[test]
fn capsule_ptr_try_into_primitives() -> JuizResult<()> {
    let i: i64 = CapsulePtr::from(jvalue!(-4)).try_into()?;
    assert_eq!(i, -4);
    let u: u64 = CapsulePtr::from(jvalue!(4)).try_into()?;
    assert_eq!(u, 4);
    let f: f64 = CapsulePtr::from(jvalue!(0.25)).try_into()?;
    assert_eq!(f, 0.25);
    let s: String = CapsulePtr::from(jvalue!("s")).try_into()?;
    assert_eq!(s, "s");
    let b: bool = CapsulePtr::from(jvalue!(true)).try_into()?;
    assert!(b);
    let vv: Vec<Value> = CapsulePtr::from(jvalue!([1, "a"])).try_into()?;
    assert_eq!(vv, vec![jvalue!(1), jvalue!("a")]);
    let vi: Vec<i64> = CapsulePtr::from(jvalue!([1, 2])).try_into()?;
    assert_eq!(vi, vec![1, 2]);
    let vf: Vec<f64> = CapsulePtr::from(jvalue!([1.5, 2.5])).try_into()?;
    assert_eq!(vf, vec![1.5, 2.5]);

    let r: JuizResult<i64> = CapsulePtr::from(jvalue!("x")).try_into();
    assert!(r.is_err());
    let r: JuizResult<u64> = CapsulePtr::from(jvalue!(-1)).try_into();
    assert!(r.is_err());
    let r: JuizResult<f64> = CapsulePtr::from(jvalue!("x")).try_into();
    assert!(r.is_err());
    let r: JuizResult<String> = CapsulePtr::from(jvalue!(1)).try_into();
    assert!(r.is_err());
    let r: JuizResult<bool> = CapsulePtr::from(jvalue!(1)).try_into();
    assert!(r.is_err());
    let r: JuizResult<Vec<Value>> = CapsulePtr::from(jvalue!(1)).try_into();
    assert!(r.is_err());
    let r: JuizResult<Vec<i64>> = CapsulePtr::from(jvalue!([1, "a"])).try_into();
    assert!(r.is_err());
    let r: JuizResult<Vec<f64>> = CapsulePtr::from(jvalue!({})).try_into();
    assert!(r.is_err());
    let r: JuizResult<i64> = CapsulePtr::new().try_into();
    assert!(r.is_err());
    Ok(())
}

#[test]
fn capsule_map_accessors() -> JuizResult<()> {
    let mut m = CapsuleMap::new();
    m.insert("i".to_owned(), jvalue!(1).into());
    m.put("s", jvalue!("str").into());
    let old = m.put("o", jvalue!({"k": 1}).into());
    assert!(old.is_none());
    m.set_param("broker", "core");

    assert_eq!(m.get_int("i")?, 1);
    assert_eq!(m.get_str("s")?, "str");
    assert_eq!(m.get_str_then("s", |s| s.len())?, 3);
    assert_eq!(m.get_map_then("o", |o| o.len())?, 1);
    assert!(m.get("missing").is_err());
    assert!(m.get_str_then("i", |_| ()).is_err());
    assert_eq!(m.get_param("broker").map(|s| s.as_str()), Some("core"));
    assert!(m.get_param("missing").is_none());
    assert_eq!(m.get_params().len(), 1);
    assert_eq!(m.get_map().len(), 3);
    assert_eq!(m.iter().count(), 3);
    assert!(format!("{}", m).contains("broker"));
    Ok(())
}

#[test]
fn capsule_map_from_value_plain_and_enveloped() -> JuizResult<()> {
    let m: CapsuleMap = jvalue!({"a": 1, "b": "x"}).try_into()?;
    assert_eq!(m.get_int("a")?, 1);
    assert_eq!(m.get_str("b")?, "x");
    assert!(m.get_params().is_empty());

    let m: CapsuleMap = jvalue!({"__map__": {"a": 2}, "__param__": {"p": "q"}}).try_into()?;
    assert_eq!(m.get_int("a")?, 2);
    assert_eq!(m.get_param("p").map(|s| s.as_str()), Some("q"));

    let r: JuizResult<CapsuleMap> = jvalue!([1]).try_into();
    assert!(r.is_err());
    Ok(())
}

#[test]
fn capsule_map_to_value() {
    let m: CapsuleMap = vec![("a", jvalue!(1))].into();
    let v: Value = m.into();
    assert_eq!(v["__map__"], jvalue!({"a": 1}));
}

#[test]
fn capsule_map_from_vectors_and_slices() -> JuizResult<()> {
    let m: CapsuleMap = vec![("a".to_owned(), CapsulePtr::from(jvalue!(1)))].into();
    assert_eq!(m.get_int("a")?, 1);
    let items = [("b", jvalue!(2))];
    let m: CapsuleMap = (&items[..]).into();
    assert_eq!(m.get_int("b")?, 2);
    Ok(())
}
