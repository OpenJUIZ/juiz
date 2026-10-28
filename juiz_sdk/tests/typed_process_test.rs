//! マクロが JuizType の引数・戻り値を受けるテスト (#192) と、Rust のネイティブ値 (遅延評価 L2 #176)
//!
//! 仕様は docs/spec/datatype.md §6・§17、docs/spec/lazy-evaluation.md §8・§9。
//! コンポーネント用のマクロはファクトリ関数の名前が関数ごとに違うので、一つのテストバイナリに複数置ける。

use std::cell::Cell;

use juiz_sdk::prelude::*;
use juiz_sdk::types::{rust_abi_tag, RustNative, TypeTag};

thread_local! {
    /// このスレッドで Pose2D を clone した回数 (テストは別々のスレッドで動くので、互いに影響しない)
    static CLONES: Cell<usize> = const { Cell::new(0) };
}

fn clones() -> usize {
    CLONES.with(|c| c.get())
}

#[derive(Clone, Debug, PartialEq, JuizType)]
#[juiz(namespace = "robot.geometry")]
pub struct Point2D {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, PartialEq, JuizType)]
#[juiz(namespace = "robot.geometry")]
pub struct Pose2D {
    pub position: Point2D,
    #[juiz(default = 0.0)]
    pub theta: f64,
}

impl Clone for Pose2D {
    fn clone(&self) -> Self {
        CLONES.with(|c| c.set(c.get() + 1));
        Pose2D {
            position: self.position.clone(),
            theta: self.theta,
        }
    }
}

/// 同じ形 (同じ完全名・指紋) の別の Rust の型。ダウンキャストできないので辞書を通る
mod other {
    use juiz_sdk::prelude::*;

    #[derive(Clone, Debug, PartialEq, JuizType)]
    #[juiz(namespace = "robot.geometry")]
    pub struct Point2D {
        pub x: f64,
        pub y: f64,
    }

    #[derive(Clone, Debug, PartialEq, JuizType)]
    #[juiz(namespace = "robot.geometry")]
    pub struct Pose2D {
        pub position: Point2D,
        #[juiz(default = 0.0)]
        pub theta: f64,
    }

    #[juiz_component_process]
    pub fn other_theta(pose: Pose2D) -> JuizResult<f64> {
        Ok(pose.theta)
    }
}

#[juiz_component_process(
    description = "moves a pose"
    arguments = {
        default = {
            pose = { position = { x = 0.0, y = 0.0 } }
            dx = 1.0
        }
        description = {
            pose = "pose to move"
        }
    }
)]
fn move_pose(pose: Pose2D, dx: f64) -> JuizResult<Pose2D> {
    Ok(Pose2D {
        position: Point2D {
            x: pose.position.x + dx,
            y: pose.position.y,
        },
        theta: pose.theta,
    })
}

#[juiz_component_process]
fn sum_values(values: Vec<i32>, scale: Option<i64>) -> JuizResult<i64> {
    Ok(values.iter().map(|v| *v as i64).sum::<i64>() * scale.unwrap_or(1))
}

#[juiz_component_process]
fn echo_value(v: Value) -> JuizResult<Value> {
    Ok(v)
}

#[juiz_component_process]
fn nothing(pose: Pose2D) -> JuizResult<()> {
    let _ = pose;
    Ok(())
}

#[juiz_component_process(arguments = { default = { pose = 1 } })]
fn bad_default(pose: Pose2D) -> JuizResult<Pose2D> {
    Ok(pose)
}

pub struct PoseStore {
    pub last: Option<Pose2D>,
}

#[juiz_component_container]
fn pose_store(origin: Point2D) -> JuizResult<Box<PoseStore>> {
    Ok(Box::new(PoseStore {
        last: Some(Pose2D {
            position: origin,
            theta: 0.0,
        }),
    }))
}

#[juiz_component_container_process(container_type = "pose_store")]
fn pose_store_swap(container: &mut ContainerImpl<PoseStore>, pose: Pose2D) -> JuizResult<Pose2D> {
    let last = container.last.replace(pose).unwrap();
    Ok(last)
}

fn pose(x: f64, y: f64, theta: f64) -> Pose2D {
    Pose2D {
        position: Point2D { x, y },
        theta,
    }
}

fn pose_value(x: f64, y: f64, theta: f64) -> Value {
    jvalue!({"position": {"x": x, "y": y}, "theta": theta})
}

fn args(items: Vec<(&str, CapsulePtr)>) -> CapsuleMap {
    let mut m = CapsuleMap::new();
    for (k, v) in items {
        m.insert(k.to_owned(), v);
    }
    m
}

fn native_ptr(p: Pose2D) -> CapsulePtr {
    p.into_capsule().unwrap().into()
}

#[test]
fn manifest_has_schemas() {
    let m = move_pose_manifest();
    assert_eq!(m.description.text, "moves a pose");
    let a = &m.arguments[0];
    assert_eq!(a.name, "pose");
    assert_eq!(a.type_name.as_str(), "Object");
    assert_eq!(a.description.text, "pose to move");
    assert_eq!(a.schema.as_ref().unwrap().name, "robot.geometry.Pose2D");
    assert_eq!(a.default, jvalue!({"position": {"x": 0.0, "y": 0.0}}));
    // 今までの型は型なしのまま
    let dx = &m.arguments[1];
    assert_eq!(dx.type_name.as_str(), "Float");
    assert!(dx.schema.is_none());
    assert_eq!(dx.default, jvalue!(1.0));
    assert_eq!(
        m.output_schema.as_ref().unwrap().fingerprint,
        Pose2D::data_type().fingerprint()
    );
    assert!(m.outputs.is_some());
    m.check_schemas().unwrap();

    let s = sum_values_manifest();
    assert_eq!(s.arguments[0].type_name.as_str(), "Array");
    assert_eq!(s.arguments[0].schema.as_ref().unwrap().name, "array<int>");
    // 既定値を書かなければ「既定値なし」
    assert_eq!(s.arguments[0].default, Value::Null);
    assert_eq!(s.arguments[1].schema.as_ref().unwrap().name, "long?");
    assert_eq!(s.output_schema.as_ref().unwrap().name, "long");

    // Value を返す関数は型なしのまま
    assert!(echo_value_manifest().output_schema.is_none());
    assert_eq!(
        nothing_manifest().output_schema.as_ref().unwrap().name,
        "null"
    );
}

#[test]
#[should_panic(expected = "manifest of 'bad_default' is invalid")]
fn invalid_default_panics_with_function_name() {
    let _ = bad_default_manifest();
}

#[test]
fn typed_return_is_native() -> JuizResult<()> {
    let r = move_pose(args(vec![
        ("pose", pose_value(1.0, 2.0, 0.5).into()),
        ("dx", jvalue!(3.0).into()),
    ]))?;
    let cell = r.native_cell().expect("record is returned as native");
    assert_eq!(cell.describe(), "rust:robot.geometry.Pose2D");
    assert!(!cell.is_encoded());
    assert_eq!(cell.value().type_tag(), Pose2D::type_tag().as_ref());
    // Rust 以外が読むときは encode して辞書になる
    assert_eq!(r.as_value().unwrap(), &pose_value(4.0, 2.0, 0.5));
    Ok(())
}

#[test]
fn native_argument_is_moved_when_not_shared() -> JuizResult<()> {
    let start = clones();
    let r = move_pose(args(vec![
        ("pose", native_ptr(pose(1.0, 0.0, 0.0))),
        ("dx", jvalue!(1.0).into()),
    ]))?;
    assert_eq!(clones(), start, "the pose must be moved, not cloned");
    // 2 段目も移す (Composite の中間値と同じ)
    let r2 = move_pose(args(vec![("pose", r.into()), ("dx", jvalue!(1.0).into())]))?;
    assert_eq!(clones(), start);
    let cell = r2.native_cell().unwrap();
    assert!(!cell.is_encoded());
    assert_eq!(
        cell.value()
            .as_any()
            .downcast_ref::<RustNative<Pose2D>>()
            .unwrap()
            .value(),
        &pose(3.0, 0.0, 0.0)
    );
    Ok(())
}

#[test]
fn shared_native_argument_is_cloned_without_encoding() -> JuizResult<()> {
    let shared = native_ptr(pose(1.0, 0.0, 0.0));
    let cell = shared.native_cell()?.unwrap();
    let start = clones();
    let r = move_pose(args(vec![
        ("pose", shared.clone()),
        ("dx", jvalue!(1.0).into()),
    ]))?;
    assert_eq!(clones(), start + 1);
    assert!(!cell.is_encoded(), "same type tag must not encode");
    assert_eq!(r.as_value().unwrap(), &pose_value(2.0, 0.0, 0.0));
    Ok(())
}

#[test]
fn different_abi_tag_goes_through_dictionary() -> JuizResult<()> {
    let tag = TypeTag {
        abi: "rust:another-plugin".to_owned(),
        ..Pose2D::type_tag().unwrap()
    };
    let c: CapsulePtr =
        Capsule::from_native(RustNative::with_tag(pose(1.0, 0.0, 0.25), tag)).into();
    let cell = c.native_cell()?.unwrap();
    let start = clones();
    let r = move_pose(args(vec![("pose", c), ("dx", jvalue!(1.0).into())]))?;
    assert!(cell.is_encoded(), "a value from another plugin is encoded");
    assert_eq!(clones(), start);
    assert_eq!(r.as_value().unwrap(), &pose_value(2.0, 0.0, 0.25));
    Ok(())
}

#[test]
fn same_schema_different_rust_type_goes_through_dictionary() -> JuizResult<()> {
    let c = native_ptr(pose(1.0, 0.0, 0.75));
    let cell = c.native_cell()?.unwrap();
    let r = other::other_theta(args(vec![("pose", c)]))?;
    assert!(cell.is_encoded());
    assert_eq!(r.as_value().unwrap(), &jvalue!(0.75));
    Ok(())
}

#[test]
fn non_record_types_are_values() -> JuizResult<()> {
    let r = sum_values(args(vec![
        ("values", jvalue!([1, 2, 3]).into()),
        ("scale", Value::Null.into()),
    ]))?;
    assert!(!r.is_native());
    assert_eq!(r.as_value().unwrap(), &jvalue!(6));
    let r = sum_values(args(vec![
        ("values", jvalue!([1, 2]).into()),
        ("scale", jvalue!(10).into()),
    ]))?;
    assert_eq!(r.as_value().unwrap(), &jvalue!(30));
    // 型に合わない値はエラー
    assert!(sum_values(args(vec![
        ("values", jvalue!(["a"]).into()),
        ("scale", Value::Null.into()),
    ]))
    .is_err());

    let r = echo_value(args(vec![("v", jvalue!({"a": 1}).into())]))?;
    assert_eq!(r.as_value().unwrap(), &jvalue!({"a": 1}));
    assert!(nothing(args(vec![("pose", pose_value(0.0, 0.0, 0.0).into())]))?.is_empty());
    Ok(())
}

#[test]
fn container_takes_typed_arguments() -> JuizResult<()> {
    let cm = pose_store_manifest();
    assert_eq!(
        cm.arguments[0].schema.as_ref().unwrap().name,
        "robot.geometry.Point2D"
    );
    let pm = pose_store_swap_manifest();
    assert_eq!(
        pm.arguments[0].schema.as_ref().unwrap().name,
        "robot.geometry.Pose2D"
    );
    assert_eq!(
        pm.output_schema.as_ref().unwrap().name,
        "robot.geometry.Pose2D"
    );

    let mut c = ContainerImpl::new(
        cm.name("store0"),
        Box::new(PoseStore {
            last: Some(pose(0.0, 0.0, 0.0)),
        }),
    )?;
    let r = pose_store_swap(
        &mut c,
        args(vec![("pose", native_ptr(pose(5.0, 0.0, 0.0)))]),
    )?;
    assert!(r.is_native());
    assert_eq!(r.as_value().unwrap(), &pose_value(0.0, 0.0, 0.0));
    assert_eq!(c.last, Some(pose(5.0, 0.0, 0.0)));
    Ok(())
}

#[test]
fn abi_tag_is_stable_in_one_binary() {
    assert_eq!(rust_abi_tag(), rust_abi_tag());
    assert!(rust_abi_tag().starts_with("rust:rustc "));
    assert_eq!(Pose2D::abi_tag().as_deref(), Some(rust_abi_tag()));
}

/// `typed = "all"` なら今までの型も型付きになる。既定値は JSON の文字列でも書ける (juiz-idl の雛形が使う)
#[juiz_component_process(
    typed = "all"
    arguments = {
        default = "{\"offset\": -1, \"names\": [\"a\"], \"note\": null}"
    }
)]
fn all_typed_args(
    offset: i64,
    names: Vec<String>,
    note: Option<String>,
    v: Value,
) -> JuizResult<Value> {
    Ok(jvalue!({"offset": offset, "names": names, "note": note, "v": v}))
}

#[test]
fn typed_all_gives_schemas_to_every_argument() -> JuizResult<()> {
    let m = all_typed_args_manifest();
    let names: Vec<&str> = m
        .arguments
        .iter()
        .map(|a| a.schema.as_ref().unwrap().name.as_str())
        .collect();
    assert_eq!(names, vec!["long", "array<string>", "string?", "any"]);
    assert_eq!(m.arguments[0].default, jvalue!(-1));
    assert_eq!(m.arguments[1].default, jvalue!(["a"]));
    assert_eq!(m.arguments[3].default, Value::Null);
    assert_eq!(m.output_schema.as_ref().unwrap().name, "any");

    let r = all_typed_args(args(vec![
        ("offset", jvalue!(3).into()),
        ("names", jvalue!(["x"]).into()),
        ("note", Value::Null.into()),
        ("v", jvalue!({"k": 1}).into()),
    ]))?;
    assert_eq!(
        r.as_value().unwrap(),
        &jvalue!({"offset": 3, "names": ["x"], "note": null, "v": {"k": 1}})
    );
    // 型付きなので、型に合わない値はエラー
    assert!(all_typed_args(args(vec![
        ("offset", jvalue!("3").into()),
        ("names", jvalue!([]).into()),
        ("note", Value::Null.into()),
        ("v", Value::Null.into()),
    ]))
    .is_err());
    Ok(())
}
