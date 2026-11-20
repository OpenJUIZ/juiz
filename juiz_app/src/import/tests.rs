//! juiz import の単体テスト (container-model.md §13.1、issue #388)

use std::path::{Path, PathBuf};

use juiz_core::prelude::serde_json;

use juiz_sdk::geometry::{Quaternion, Transform, Vector3};

use super::write::relative;
use super::*;

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let p = std::env::temp_dir().join(format!(
            "juiz_import_{name}_{}",
            std::process::id() as u128 * 1000 + rand_suffix()
        ));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
}

fn rand_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos() as u128
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn args(file: &str) -> ImportArgs {
    ImportArgs {
        file: file.to_owned(),
        output: None,
        anchor: None,
        namespace: None,
        package_paths: Vec::new(),
        model_paths: Vec::new(),
        models_dir: None,
        copy_meshes: false,
        force: false,
    }
}

fn binary_stl() -> Vec<u8> {
    let mut v = vec![0u8; 80];
    v.extend_from_slice(&1u32.to_le_bytes());
    v.extend(std::iter::repeat(0u8).take(50));
    v
}

const URDF: &str = r#"<?xml version="1.0"?>
<robot name="arm">
  <material name="blue"><color rgba="0 0 1 1"/></material>
  <link name="base_link">
    <visual><origin xyz="0 0 0.05"/><geometry><box size="0.2 0.2 0.1"/></geometry><material name="blue"/></visual>
    <visual name="pole"><origin xyz="0 0 0.2"/><geometry><cylinder radius="0.02" length="0.3"/></geometry></visual>
  </link>
  <link name="upper_arm">
    <visual><geometry><sphere radius="0.05"/></geometry></visual>
  </link>
  <link name="head">
    <visual><origin xyz="0 0 0.01"/><geometry><mesh filename="package://my_robot/meshes/head.stl" scale="0.001 0.001 0.001"/></geometry>
      <material name="red"><color rgba="1 0 0 1"/></material></visual>
  </link>
  <link name="tool/0"/>
  <joint name="shoulder" type="revolute">
    <parent link="base_link"/><child link="upper_arm"/>
    <origin xyz="0 0 0.35" rpy="0 0 1.5707963267948966"/><axis xyz="0 1 0"/><limit lower="-1.57" upper="1.57"/>
  </joint>
  <joint name="neck" type="continuous">
    <parent link="upper_arm"/><child link="head"/><origin xyz="0.2 0 0"/>
  </joint>
  <joint name="tool" type="fixed"><parent link="head"/><child link="tool/0"/><origin xyz="0.05 0 0"/></joint>
</robot>
"#;

fn setup_package(dir: &Path) -> PathBuf {
    let pkg = dir.join("pkg/my_robot");
    std::fs::create_dir_all(pkg.join("meshes")).unwrap();
    std::fs::write(pkg.join("meshes/head.stl"), binary_stl()).unwrap();
    pkg
}

fn resolver(dir: &Path, packages: &[(&str, PathBuf)], models: &[PathBuf]) -> Resolver {
    Resolver {
        base_dir: dir.to_path_buf(),
        packages: packages
            .iter()
            .map(|(n, p)| (n.to_string(), p.clone()))
            .collect(),
        package_roots: Vec::new(),
        ament_prefixes: Vec::new(),
        model_roots: models.to_vec(),
    }
}

fn conf_value(conf: &str) -> serde_json::Value {
    let docs = yaml_rust2::YamlLoader::load_from_str(conf).expect("valid YAML");
    yaml_to_json(&docs[0])
}

fn yaml_to_json(y: &yaml_rust2::Yaml) -> serde_json::Value {
    use yaml_rust2::Yaml;
    match y {
        Yaml::Real(s) => serde_json::json!(s.parse::<f64>().unwrap()),
        Yaml::Integer(i) => serde_json::json!(*i as f64),
        Yaml::String(s) => serde_json::json!(s),
        Yaml::Boolean(b) => serde_json::json!(b),
        Yaml::Array(a) => serde_json::Value::Array(a.iter().map(yaml_to_json).collect()),
        Yaml::Hash(h) => serde_json::Value::Object(
            h.iter()
                .map(|(k, v)| (k.as_str().unwrap().to_owned(), yaml_to_json(v)))
                .collect(),
        ),
        _ => serde_json::Value::Null,
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn urdf_links_joints_and_models() {
    let dir = TempDir::new("urdf");
    let pkg = setup_package(&dir.0);
    let conf_dir = dir.0.join("out");
    let r = resolver(&dir.0, &[("my_robot", pkg)], &[]);
    let out = import(Kind::Urdf, URDF, &r, &conf_dir, &args("robot.urdf")).unwrap();
    let v = conf_value(&out.conf);
    let cs = v["containers"].as_array().unwrap();
    assert_eq!(cs.len(), 4, "{}", out.conf);
    // 根は static_anchor、ほかは frame
    assert_eq!(cs[0]["type_name"], "static_anchor");
    assert_eq!(cs[0]["name"], "base_link");
    assert_eq!(cs[1]["type_name"], "frame");
    // 基本形・複数の visual は link ごとの SDF
    assert_eq!(cs[0]["model"]["path"], "models/arm/base_link.sdf");
    assert_eq!(cs[1]["model"]["path"], "models/arm/upper_arm.sdf");
    // visual 1 つの STL はそのファイルを指す。scale・origin・色
    assert_eq!(cs[2]["model"]["path"], "../pkg/my_robot/meshes/head.stl");
    assert!(close(cs[2]["model"]["scale"].as_f64().unwrap(), 0.001));
    assert!(close(
        cs[2]["model"]["origin"]["translation"]["z"]
            .as_f64()
            .unwrap(),
        0.01
    ));
    assert_eq!(cs[2]["model"]["color"], "#ff0000");
    // 使えない文字は _ に
    assert_eq!(cs[3]["name"], "tool_0");
    assert!(cs[3].get("model").is_none());
    assert!(
        out.warnings.iter().any(|w| w.contains("renamed")),
        "{:?}",
        out.warnings
    );

    let ts = v["transforms"].as_array().unwrap();
    assert_eq!(ts.len(), 3);
    assert_eq!(ts[0]["source"], "base_link");
    assert_eq!(ts[0]["destination"], "upper_arm");
    assert!(close(ts[0]["translation"]["z"].as_f64().unwrap(), 0.35));
    assert!(close(
        ts[0]["rotation"]["z"].as_f64().unwrap(),
        std::f64::consts::FRAC_1_SQRT_2
    ));
    assert_eq!(ts[2]["destination"], "tool_0");
    // 関節の種類・軸・範囲はコメントに残す
    assert!(
        out.conf
            .contains("# joint \"shoulder\": revolute, axis [0, 1, 0], limit [-1.57, 1.57]"),
        "{}",
        out.conf
    );
    assert!(
        out.conf.contains("# joint \"tool\": fixed\n"),
        "{}",
        out.conf
    );

    // SDF のファイル: 色と pose
    assert_eq!(out.files.len(), 2);
    let (p, bytes) = &out.files[0];
    assert_eq!(p, &conf_dir.join("models/arm/base_link.sdf"));
    let sdf = String::from_utf8(bytes.clone()).unwrap();
    assert!(sdf.contains("<pose>0 0 0.05 0 0 0</pose>"), "{sdf}");
    assert!(sdf.contains("<box><size>0.2 0.2 0.1</size></box>"), "{sdf}");
    assert!(sdf.contains("<diffuse>0 0 1 1</diffuse>"), "{sdf}");
    // 書いた SDF は core の作成時の検査 (§3.1) を通る
    for (p, bytes) in &out.files {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
        juiz_core::models::ContainerModel::load(
            &juiz_sdk::manifests::ModelManifest::new(&p.to_string_lossy()),
            None,
            juiz_core::models::DEFAULT_MODEL_MAX_BYTES,
        )
        .unwrap();
    }
}

#[test]
fn urdf_anchor_namespace_and_copy() {
    let dir = TempDir::new("urdf_anchor");
    let pkg = setup_package(&dir.0);
    let conf_dir = dir.0.join("app");
    let r = resolver(&dir.0, &[("my_robot", pkg.clone())], &[]);
    let mut a = args("robot.urdf");
    a.anchor = Some("world".to_owned());
    a.namespace = Some("robot1".to_owned());
    a.copy_meshes = true;
    a.models_dir = Some("models".to_owned());
    let out = import(Kind::Urdf, URDF, &r, &conf_dir, &a).unwrap();
    let v = conf_value(&out.conf);
    let cs = v["containers"].as_array().unwrap();
    assert_eq!(cs[0]["type_name"], "static_anchor");
    assert_eq!(cs[0]["name"], "world");
    assert_eq!(cs[0]["namespace"], "robot1");
    assert!(cs[1..].iter().all(|c| c["type_name"] == "frame"));
    // 写したメッシュはパッケージの中を指す
    assert_eq!(cs[3]["model"]["path"], "models/meshes/head.stl");
    assert_eq!(
        out.copies,
        vec![(
            pkg.join("meshes/head.stl"),
            conf_dir.join("models/meshes/head.stl")
        )]
    );
    let ts = v["transforms"].as_array().unwrap();
    assert_eq!(ts[0]["source"], "robot1/world");
    assert_eq!(ts[0]["destination"], "robot1/base_link");
    assert!(ts[0].get("translation").is_none(), "identity is omitted");
    assert_eq!(ts[1]["source"], "robot1/base_link");
}

#[test]
fn urdf_errors() {
    let dir = TempDir::new("urdf_err");
    let r = resolver(&dir.0, &[], &[]);
    let a = args("x.urdf");
    let err = |t: &str| {
        import(Kind::Urdf, t, &r, &dir.0, &a)
            .err()
            .unwrap_or_default()
    };
    assert!(err("<sdf/>").contains("<robot>"));
    assert!(err("<robot name='r'/>").contains("no link"));
    let cycle = r#"<robot name="r"><link name="a"/><link name="b"/>
      <joint name="j1" type="fixed"><parent link="a"/><child link="b"/></joint>
      <joint name="j2" type="fixed"><parent link="b"/><child link="a"/></joint></robot>"#;
    assert!(err(cycle).contains("cycle"), "{}", err(cycle));
    let unknown = r#"<robot name="r"><link name="a"/>
      <joint name="j1" type="fixed"><parent link="a"/><child link="z"/></joint></robot>"#;
    assert!(err(unknown).contains("unknown link 'z'"));
    // 見つからないメッシュは警告にしてモデルを付けない
    let missing = r#"<robot name="r"><link name="a"><visual><geometry>
      <mesh filename="package://nowhere/m.stl"/></geometry></visual></link></robot>"#;
    let out = import(Kind::Urdf, missing, &r, &dir.0, &a).unwrap();
    assert!(
        out.warnings.iter().any(|w| w.contains("package 'nowhere'")),
        "{:?}",
        out.warnings
    );
    assert!(!out.conf.contains("model:"), "{}", out.conf);
    // --anchor が link と同じ名前
    let mut a2 = args("x.urdf");
    a2.anchor = Some("a".to_owned());
    assert!(import(
        Kind::Urdf,
        "<robot name='r'><link name='a'/></robot>",
        &r,
        &dir.0,
        &a2
    )
    .err()
    .unwrap()
    .contains("same as a link"));
}

const SDF: &str = r#"<sdf version="1.9">
  <model name="cart">
    <pose>1 0 0 0 0 0</pose>
    <link name="chassis">
      <pose>0 0 0.1 0 0 0</pose>
      <visual name="body"><geometry><box><size>0.4 0.3 0.1</size></box></geometry>
        <material><diffuse>0.8 0.8 0.8</diffuse></material></visual>
    </link>
    <link name="wheel_left">
      <pose relative_to="chassis">0 0.17 -0.05 -90 0 0</pose>
      <visual name="v"><geometry><mesh><uri>model://cart/meshes/wheel.stl</uri></mesh></geometry></visual>
    </link>
    <link name="caster">
      <pose>-0.15 0 0.03 0 0 0</pose>
      <visual name="v"><geometry><sphere><radius>0.03</radius></sphere></geometry></visual>
    </link>
    <link name="free"/>
    <joint name="left_wheel_joint" type="revolute">
      <parent>chassis</parent><child>wheel_left</child>
      <axis><xyz>0 0 1</xyz><limit><lower>-1</lower><upper>1</upper></limit></axis>
    </joint>
    <joint name="caster_joint" type="ball"><parent>chassis</parent><child>caster</child></joint>
  </model>
</sdf>
"#;

#[test]
fn sdf_links_poses_and_roots() {
    let dir = TempDir::new("sdf");
    let gz = dir.0.join("gz");
    std::fs::create_dir_all(gz.join("cart/meshes")).unwrap();
    std::fs::write(gz.join("cart/meshes/wheel.stl"), binary_stl()).unwrap();
    // degrees="true" を試す
    let text = SDF.replace(
        "<pose relative_to=\"chassis\">",
        "<pose relative_to=\"chassis\" degrees=\"true\">",
    );
    let r = resolver(&dir.0, &[], &[gz.clone()]);
    let out = import(Kind::Sdf, &text, &r, &dir.0, &args("cart.sdf")).unwrap();
    let v = conf_value(&out.conf);
    let cs = v["containers"].as_array().unwrap();
    // 根が 2 つ (chassis と free) なので、model の名前の Anchor を足す
    assert_eq!(cs[0]["type_name"], "static_anchor");
    assert_eq!(cs[0]["name"], "cart");
    assert!(
        out.warnings.iter().any(|w| w.contains("2 root links")),
        "{:?}",
        out.warnings
    );
    assert_eq!(cs[2]["model"]["path"], "gz/cart/meshes/wheel.stl");
    let ts = v["transforms"].as_array().unwrap();
    // Anchor → 根は model の座標から見た link の位置 (自動の Anchor には model の <pose> を掛けない)
    assert_eq!(ts[0]["destination"], "chassis");
    assert!(close(ts[0]["translation"]["z"].as_f64().unwrap(), 0.1));
    assert!(ts[0]["translation"]["x"].as_f64().unwrap().abs() < 1e-12);
    assert_eq!(ts[1]["destination"], "free");
    // joint: parent → child = pose(parent)⁻¹ ∘ pose(child)。relative_to と度
    let wheel = &ts[2];
    assert_eq!(wheel["destination"], "wheel_left");
    assert!(close(wheel["translation"]["y"].as_f64().unwrap(), 0.17));
    assert!(close(wheel["translation"]["z"].as_f64().unwrap(), -0.05));
    let rot = Quaternion::new(
        wheel["rotation"]["x"].as_f64().unwrap(),
        wheel["rotation"]["y"].as_f64().unwrap(),
        wheel["rotation"]["z"].as_f64().unwrap(),
        wheel["rotation"]["w"].as_f64().unwrap(),
    );
    assert!(
        rot.angle_to(&Quaternion::from_rpy(
            -std::f64::consts::FRAC_PI_2,
            0.0,
            0.0
        )) < 1e-9
    );
    let caster = &ts[3];
    assert!(close(caster["translation"]["x"].as_f64().unwrap(), -0.15));
    assert!(close(caster["translation"]["z"].as_f64().unwrap(), -0.07));
    assert!(out
        .conf
        .contains("# joint \"left_wheel_joint\": revolute, axis [0, 0, 1], limit [-1, 1]"));
    assert!(
        out.conf.contains("# joint \"caster_joint\": ball\n"),
        "{}",
        out.conf
    );

    // --anchor なら model の <pose> も掛ける
    let mut a = args("cart.sdf");
    a.anchor = Some("map".to_owned());
    let out = import(Kind::Sdf, &text, &r, &dir.0, &a).unwrap();
    let v = conf_value(&out.conf);
    let t0 = &v["transforms"][0];
    assert_eq!(t0["source"], "map");
    assert!(close(t0["translation"]["x"].as_f64().unwrap(), 1.0));
    assert!(close(t0["translation"]["z"].as_f64().unwrap(), 0.1));
}

#[test]
fn sdf_world_and_errors() {
    let dir = TempDir::new("sdf_err");
    let r = resolver(&dir.0, &[], &[]);
    let a = args("w.sdf");
    // <world> の中のただ 1 つの model と、world に付いた joint
    let world = r#"<sdf version="1.9"><world name="w"><model name="m">
      <link name="a"/><link name="b"><pose>0 0 1 0 0 0</pose></link>
      <joint name="fix" type="fixed"><parent>world</parent><child>a</child></joint>
      <joint name="j" type="prismatic"><parent>a</parent><child>b</child></joint>
    </model></world></sdf>"#;
    let out = import(Kind::Sdf, world, &r, &dir.0, &a).unwrap();
    let v = conf_value(&out.conf);
    assert_eq!(v["containers"][0]["type_name"], "static_anchor");
    assert_eq!(v["containers"][0]["name"], "a");
    assert_eq!(v["transforms"].as_array().unwrap().len(), 1);
    assert!(close(
        v["transforms"][0]["translation"]["z"].as_f64().unwrap(),
        1.0
    ));
    let err = |t: &str| {
        import(Kind::Sdf, t, &r, &dir.0, &a)
            .err()
            .unwrap_or_default()
    };
    assert!(err("<robot/>").contains("<sdf>"));
    assert!(err("<sdf><world/></sdf>").contains("no <model>"));
    assert!(err("<sdf><model name='a'><link name='l'/></model><model name='b'><link name='l'/></model></sdf>")
        .contains("2 models"));
}

#[test]
fn relative_paths() {
    let base = std::env::temp_dir();
    assert_eq!(
        relative(&base.join("a"), &base.join("a/b/c.stl")),
        "b/c.stl"
    );
    assert_eq!(
        relative(&base.join("a/x"), &base.join("a/b/c.stl")),
        "../b/c.stl"
    );
    assert_eq!(
        relative(&base.join("a/./x/.."), &base.join("a/b.stl")),
        "b.stl"
    );
}

#[test]
fn resolver_finds_files() {
    let dir = TempDir::new("resolve");
    std::fs::create_dir_all(dir.0.join("ws/pkg_a/meshes")).unwrap();
    std::fs::write(dir.0.join("ws/pkg_a/meshes/m.stl"), b"solid").unwrap();
    std::fs::create_dir_all(dir.0.join("prefix/share/pkg_b")).unwrap();
    std::fs::write(dir.0.join("prefix/share/pkg_b/m.stl"), b"solid").unwrap();
    std::fs::write(dir.0.join("rel.stl"), b"solid").unwrap();
    let r = Resolver {
        base_dir: dir.0.clone(),
        packages: Vec::new(),
        package_roots: vec![dir.0.join("ws")],
        ament_prefixes: vec![dir.0.join("prefix")],
        model_roots: Vec::new(),
    };
    assert_eq!(
        r.resolve("package://pkg_a/meshes/m.stl").unwrap(),
        dir.0.join("ws/pkg_a/meshes/m.stl")
    );
    assert_eq!(
        r.resolve("package://pkg_b/m.stl").unwrap(),
        dir.0.join("prefix/share/pkg_b/m.stl")
    );
    assert_eq!(r.resolve("rel.stl").unwrap(), dir.0.join("rel.stl"));
    let abs = dir.0.join("rel.stl");
    assert_eq!(
        r.resolve(&format!("file://{}", abs.display())).unwrap(),
        abs
    );
    assert!(r
        .resolve("package://pkg_a/meshes/none.stl")
        .unwrap_err()
        .contains("does not exist"));
    assert!(r
        .resolve("https://example.com/m.stl")
        .unwrap_err()
        .contains("not supported"));
    assert!(r
        .resolve("model://m/x.stl")
        .unwrap_err()
        .contains("--model-path"));
}

#[test]
fn rpy_matches_transform_rule() {
    // URDF の rpy は固定軸 roll → pitch → yaw (Rz·Ry·Rx)、transformations.md §3.2 と同じ
    let t = super::robot::rpy([1.0, 2.0, 3.0], [0.0, 0.0, std::f64::consts::FRAC_PI_2]);
    let p = t.transform_point(&Vector3::new(1.0, 0.0, 0.0));
    assert!(
        close(p.x, 1.0) && close(p.y, 3.0) && close(p.z, 3.0),
        "{p:?}"
    );
    let _ = Transform::identity();
}
