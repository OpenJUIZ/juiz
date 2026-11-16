//! Composite の定義時の型検査 (Composition C8、issue #172) のテスト
//!
//! - 子の戻り値 (writer) と親の引数 (reader) を、接続時と同じ `check_connectable` で比べる。
//!   同じ・互換なら定義できる。互換でなければ定義時に `CompositeManifestInvalidError` (下に `TypeIncompatibleError`)
//! - 型なしの子とはいつでも組める
//! - `const` と `expose` の `default` は型と照らし合わせる (datatype.md §4・§8)
//! - 1 つの公開名に型の違う引数をまとめるとエラー
//! - Composite の manifest は公開引数の `schema` と根の `output_schema` を引き継ぐ
//!
//! 仕様は docs/spec/composition.md §3.3、docs/spec/datatype.md §8。
use std::collections::HashMap;

use juiz_core::prelude::*;
use juiz_sdk::manifests::{CompositeManifest, CompositeNode, ExposeSpec};
use juiz_sdk::processes::ProcessBodyFunctionType;
use juiz_sdk::types::{DataType, RecordField, RecordType, TypeSchema};

fn point2d() -> DataType {
    DataType::Record(
        RecordType::new("robot.geometry", "Point2D")
            .field(RecordField::new("x", DataType::Double))
            .field(RecordField::new("y", DataType::Double)),
    )
}

fn pose2d_record() -> RecordType {
    RecordType::new("robot.geometry", "Pose2D")
        .field(RecordField::new("position", point2d()))
        .field(
            RecordField::new("theta", DataType::Double)
                .default_value(jvalue!(0.0))
                .unwrap(),
        )
}

fn pose2d() -> TypeSchema {
    TypeSchema::from_data_type(&DataType::Record(pose2d_record())).unwrap()
}

/// 既定値つきの frame を足した Pose2D (古い Pose2D の値を読める = 互換)
fn pose2d_v2() -> TypeSchema {
    TypeSchema::from_data_type(&DataType::Record(
        pose2d_record().field(
            RecordField::new("frame", DataType::String)
                .default_value(jvalue!("map"))
                .unwrap(),
        ),
    ))
    .unwrap()
}

/// 既定値なしの frame を足した Pose2D (古い Pose2D の値を読めない = 互換でない)
fn pose2d_strict() -> TypeSchema {
    TypeSchema::from_data_type(&DataType::Record(
        RecordType::new("robot.geometry", "Pose2D")
            .field(RecordField::new("position", point2d()))
            .field(RecordField::new("frame", DataType::String)),
    ))
    .unwrap()
}

fn twist() -> TypeSchema {
    TypeSchema::from_data_type(&DataType::Record(
        RecordType::new("robot.geometry", "Twist")
            .field(RecordField::new("linear", DataType::Double))
            .field(RecordField::new("angular", DataType::Double)),
    ))
    .unwrap()
}

fn long_schema() -> TypeSchema {
    TypeSchema::from_data_type(&DataType::Long).unwrap()
}

fn double_schema() -> TypeSchema {
    TypeSchema::from_data_type(&DataType::Double).unwrap()
}

fn pose_value() -> Value {
    jvalue!({"position": {"x": 1.5, "y": 0.0}, "theta": 0.0})
}

fn make_pose(_: CapsuleMap) -> JuizResult<Capsule> {
    Ok(pose_value().into())
}

fn make_twist(_: CapsuleMap) -> JuizResult<Capsule> {
    Ok(jvalue!({"linear": 1.0, "angular": 0.0}).into())
}

fn pass_pose(args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(args.get("pose")?.lock_as_value(|v| v.clone())?.into())
}

/// pose.position.x を返す
fn read_x(args: CapsuleMap) -> JuizResult<Capsule> {
    let x = args
        .get("pose")?
        .lock_as_value(|v| v.pointer("/position/x").cloned().unwrap_or(Value::Null))?;
    Ok(x.into())
}

fn pass_value(args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(args.get("value")?.lock_as_value(|v| v.clone())?.into())
}

struct Factories(HashMap<String, ProcessFactoryPtr>);

impl Factories {
    fn new() -> JuizResult<Self> {
        let mut f = Factories(HashMap::new());
        f.add(
            ProcessManifest::new("make_pose").set_output_schema(Some(pose2d()))?,
            make_pose,
        )?;
        f.add(
            ProcessManifest::new("make_twist").set_output_schema(Some(twist()))?,
            make_twist,
        )?;
        f.add(ProcessManifest::new("make_untyped"), make_pose)?;
        f.add(
            ProcessManifest::new("pass_pose")
                .add_typed_arg("pose", "", pose2d(), pose_value())?
                .set_output_schema(Some(pose2d()))?,
            pass_pose,
        )?;
        f.add(
            ProcessManifest::new("read_pose").add_typed_arg("pose", "", pose2d(), pose_value())?,
            read_x,
        )?;
        f.add(
            ProcessManifest::new("read_pose_v2").add_typed_arg(
                "pose",
                "",
                pose2d_v2(),
                pose_value(),
            )?,
            read_x,
        )?;
        f.add(
            ProcessManifest::new("read_pose_strict").add_typed_arg(
                "pose",
                "",
                pose2d_strict(),
                Value::Null,
            )?,
            read_x,
        )?;
        f.add(
            ProcessManifest::new("read_untyped").add_object_arg("pose", "", pose_value()),
            read_x,
        )?;
        f.add(
            ProcessManifest::new("pass_long")
                .add_typed_arg("value", "", long_schema(), jvalue!(1))?
                .set_output_schema(Some(long_schema()))?,
            pass_value,
        )?;
        f.add(
            ProcessManifest::new("pass_double")
                .add_typed_arg("value", "", double_schema(), jvalue!(1.0))?
                .set_output_schema(Some(double_schema()))?,
            pass_value,
        )?;
        f.add(
            ProcessManifest::new("pass_untyped").add_object_arg("value", "", jvalue!(1)),
            pass_value,
        )?;
        Ok(f)
    }

    fn add(
        &mut self,
        manifest: ProcessManifest,
        function: ProcessBodyFunctionType,
    ) -> JuizResult<()> {
        let type_name = manifest.type_name.clone();
        self.0
            .insert(type_name, process_factory_create(manifest, function)?);
        Ok(())
    }

    fn composite(&mut self, composite: &CompositeManifest) -> JuizResult<ProcessFactoryPtr> {
        let pf = composite_process_factory_create(composite, |name| self.0.get(name).cloned())?;
        self.0.insert(composite.type_name.clone(), pf.clone());
        Ok(pf)
    }
}

fn node(process: &str) -> CompositeNode {
    CompositeNode::new(process)
}

/// reader(pose = writer())
fn chain(type_name: &str, reader: &str, writer: &str) -> CompositeManifest {
    CompositeManifest::new(type_name, node(reader).arg_node("pose", node(writer)))
}

fn call_x(pf: &ProcessFactoryPtr) -> JuizResult<Value> {
    let p = pf
        .lock()?
        .create_process(&ProcessManifest::new(pf.type_name()).name("p0"))?;
    let v = p
        .lock()?
        .call(CapsuleMap::new())?
        .lock_as_value(|v| v.clone())?;
    Ok(v)
}

/// 定義時エラーの位置と、下に残った TypeIncompatibleError の有無
fn invalid_error(r: JuizResult<ProcessFactoryPtr>) -> (String, String, bool) {
    let e = match r {
        Ok(_) => panic!("must fail"),
        Err(e) => e,
    };
    let has_incompatible = e.chain().any(|c| {
        matches!(
            c.downcast_ref::<JuizError>(),
            Some(JuizError::TypeIncompatibleError { .. })
        )
    });
    match e.downcast_ref::<JuizError>() {
        Some(JuizError::CompositeManifestInvalidError { path, message, .. }) => {
            (path.clone(), message.clone(), has_incompatible)
        }
        other => panic!("unexpected error: {other:?} ({e:#})"),
    }
}

#[test]
fn same_type_can_be_composed() -> JuizResult<()> {
    let mut f = Factories::new()?;
    let pf = f.composite(&chain("same", "read_pose", "make_pose"))?;
    assert_eq!(call_x(&pf)?, jvalue!(1.5));
    Ok(())
}

#[test]
fn compatible_type_can_be_composed() -> JuizResult<()> {
    let mut f = Factories::new()?;
    let pf = f.composite(&chain("compatible", "read_pose_v2", "make_pose"))?;
    assert_eq!(call_x(&pf)?, jvalue!(1.5));
    // 数の昇格 long → double も互換
    f.composite(&CompositeManifest::new(
        "promote",
        node("pass_double").arg_node("value", node("pass_long")),
    ))?;
    Ok(())
}

#[test]
fn incompatible_type_is_rejected_at_definition() -> JuizResult<()> {
    let mut f = Factories::new()?;
    let (path, message, incompatible) =
        invalid_error(f.composite(&chain("strict", "read_pose_strict", "make_pose")));
    assert_eq!(path, "body.pose");
    assert!(message.contains("make_pose"), "{message}");
    assert!(message.contains("read_pose_strict"), "{message}");
    assert!(incompatible);

    // 完全名の違うレコード
    let (path, _, incompatible) =
        invalid_error(f.composite(&chain("twist", "read_pose", "make_twist")));
    assert_eq!(path, "body.pose");
    assert!(incompatible);

    // 数の逆向き (double → long) は互換でない
    let (path, _, incompatible) = invalid_error(f.composite(&CompositeManifest::new(
        "narrow",
        node("pass_long").arg_node("value", node("pass_double")),
    )));
    assert_eq!(path, "body.value");
    assert!(incompatible);

    // 失敗した定義は登録されない
    assert!(!f.0.contains_key("strict"));
    Ok(())
}

#[test]
fn deep_node_error_has_its_path() -> JuizResult<()> {
    let mut f = Factories::new()?;
    // read_pose(pose = pass_pose(pose = make_twist()))
    let (path, _, incompatible) = invalid_error(f.composite(&CompositeManifest::new(
        "deep",
        node("read_pose").arg_node(
            "pose",
            node("pass_pose").arg_node("pose", node("make_twist")),
        ),
    )));
    assert_eq!(path, "body.pose.pose");
    assert!(incompatible);
    Ok(())
}

#[test]
fn untyped_child_can_always_be_composed() -> JuizResult<()> {
    let mut f = Factories::new()?;
    // 型なしの戻り値 → 型付きの引数
    let pf = f.composite(&chain("untyped_writer", "read_pose", "make_untyped"))?;
    assert_eq!(call_x(&pf)?, jvalue!(1.5));
    // 型付きの戻り値 → 型なしの引数
    let pf = f.composite(&chain("untyped_reader", "read_untyped", "make_twist"))?;
    assert_eq!(call_x(&pf)?, Value::Null);
    Ok(())
}

#[test]
fn nested_composite_carries_its_types() -> JuizResult<()> {
    let mut f = Factories::new()?;
    // 内側の Composite の戻り値は根 (pass_pose) の型
    let inner = f.composite(&CompositeManifest::new(
        "inner",
        node("pass_pose").arg_node("pose", node("make_pose")),
    ))?;
    let m = inner.lock()?.manifest()?;
    assert_eq!(
        m.output_schema.as_ref().map(|s| s.identifier()),
        Some(pose2d().identifier())
    );
    f.composite(&chain("outer_ok", "read_pose", "inner"))?;
    let (path, _, incompatible) =
        invalid_error(f.composite(&chain("outer_ng", "read_pose_strict", "inner")));
    assert_eq!(path, "body.pose");
    assert!(incompatible);
    Ok(())
}

#[test]
fn manifest_inherits_argument_and_output_schema() -> JuizResult<()> {
    let mut f = Factories::new()?;
    // pass_pose(pose = 公開) をそのまま Composite にする
    let pf = f.composite(&CompositeManifest::new("wrap", node("pass_pose")))?;
    let m = pf.lock()?.manifest()?;
    let pose = m.arguments.iter().find(|a| a.name == "pose").unwrap();
    assert_eq!(
        pose.schema.as_ref().map(|s| s.identifier()),
        Some(pose2d().identifier())
    );
    assert_eq!(
        m.output_schema.as_ref().map(|s| s.identifier()),
        Some(pose2d().identifier())
    );
    Ok(())
}

#[test]
fn const_is_checked_against_schema() -> JuizResult<()> {
    let mut f = Factories::new()?;
    f.composite(&CompositeManifest::new(
        "const_ok",
        node("read_pose").arg_const("pose", pose_value()),
    ))?;
    // position.x が文字列
    let (path, message, _) = invalid_error(f.composite(&CompositeManifest::new(
        "const_ng",
        node("read_pose").arg_const("pose", jvalue!({"position": {"x": "a", "y": 0.0}})),
    )));
    assert_eq!(path, "body");
    assert!(message.contains("robot.geometry.Pose2D"), "{message}");
    // 省略可能でない型に null は渡せない
    invalid_error(f.composite(&CompositeManifest::new(
        "const_null",
        node("pass_long").arg_const("value", Value::Null),
    )));
    Ok(())
}

#[test]
fn expose_default_is_checked_against_schema() -> JuizResult<()> {
    let mut f = Factories::new()?;
    f.composite(&CompositeManifest::new(
        "default_ok",
        node("read_pose").arg_expose("pose", ExposeSpec::new("p").default(pose_value())),
    ))?;
    let (path, _, _) = invalid_error(f.composite(&CompositeManifest::new(
        "default_ng",
        node("read_pose").arg_expose(
            "pose",
            ExposeSpec::new("p").default(jvalue!({"position": 1})),
        ),
    )));
    assert_eq!(path, "body");
    Ok(())
}

#[test]
fn exposed_name_with_different_types_collides() -> JuizResult<()> {
    let mut f = Factories::new()?;
    f.add(
        ProcessManifest::new("pair")
            .add_object_arg("a", "", jvalue!(0))
            .add_object_arg("b", "", jvalue!(0)),
        pair,
    )?;
    // pair(a = read_pose(pose = p), b = <second>(pose = p))。引数の型名はどちらも Object
    let pair_of = |type_name: &str, first: &str, second: &str| {
        CompositeManifest::new(
            type_name,
            node("pair")
                .arg_node("a", node(first).arg_expose("pose", ExposeSpec::new("p")))
                .arg_node("b", node(second).arg_expose("pose", ExposeSpec::new("p"))),
        )
    };
    // 同じ型どうしはまとめられる
    f.composite(&pair_of("pair_same", "read_pose", "read_pose"))?;
    // 互換でも型が違えばまとめられない
    let (path, message, _) =
        invalid_error(f.composite(&pair_of("pair_ng", "read_pose", "read_pose_v2")));
    assert_eq!(path, "body.b");
    assert!(message.contains("collides"), "{message}");
    // 型なしと型付きはまとめられ、公開引数は型付きの方の型になる
    let pf = f.composite(&pair_of("pair_untyped", "read_untyped", "read_pose"))?;
    let m = pf.lock()?.manifest()?;
    let p = m.arguments.iter().find(|a| a.name == "p").unwrap();
    assert_eq!(
        p.schema.as_ref().map(|s| s.identifier()),
        Some(pose2d().identifier())
    );
    Ok(())
}

fn pair(args: CapsuleMap) -> JuizResult<Capsule> {
    let a = args.get("a")?.lock_as_value(|v| v.clone())?;
    let b = args.get("b")?.lock_as_value(|v| v.clone())?;
    Ok(jvalue!([a, b]).into())
}
