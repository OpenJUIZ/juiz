use super::*;

/// テスト用の一時ディレクトリ (落ちたら消す)
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> TempDir {
        let p = std::env::temp_dir().join(format!("juiz_model_test_{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.0.join(name);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&p, bytes).unwrap();
        p
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 最小の .glb (ヘッダだけ。core は中身を読まない)
pub(crate) fn glb_bytes(payload_len: usize) -> Vec<u8> {
    let mut v = b"glTF".to_vec();
    v.extend_from_slice(&2u32.to_le_bytes());
    v.extend_from_slice(&((12 + payload_len) as u32).to_le_bytes());
    v.extend(std::iter::repeat(0u8).take(payload_len));
    v
}

fn binary_stl(triangles: u32) -> Vec<u8> {
    let mut v = vec![0u8; 80];
    v.extend_from_slice(&triangles.to_le_bytes());
    v.extend(std::iter::repeat(0u8).take(50 * triangles as usize));
    v
}

const SDF: &str = r#"<?xml version="1.0"?>
<sdf version="1.9">
  <model name="pantilt">
    <link name="body">
      <visual name="base">
        <geometry><box><size>0.06 0.04 0.04</size></box></geometry>
      </visual>
    </link>
  </model>
</sdf>"#;

fn load(dir: &TempDir, m: ModelManifest) -> JuizResult<ContainerModel> {
    ContainerModel::load(&m, Some(&dir.0), DEFAULT_MODEL_MAX_BYTES)
}

fn error_of(r: JuizResult<ContainerModel>) -> &'static str {
    let e = r.unwrap_err();
    match e.chain().find_map(|c| c.downcast_ref::<JuizError>()) {
        Some(JuizError::ContainerModelInvalidError { .. }) => "invalid",
        Some(JuizError::ContainerModelFileNotFoundError { .. }) => "not_found",
        _ => panic!("unexpected error: {e:#}"),
    }
}

fn is_invalid(r: JuizResult<ContainerModel>) -> bool {
    error_of(r) == "invalid"
}

#[test]
fn glb_is_checked_and_described() {
    let dir = TempDir::new();
    let bytes = glb_bytes(100);
    dir.write("models/pantilt.glb", &bytes);
    let m = load(&dir, ModelManifest::new("models/pantilt.glb")).unwrap();
    let info = m.info();
    assert_eq!(info["format"], "glb");
    assert_eq!(info["media_type"], "model/gltf-binary");
    assert_eq!(info["size"], bytes.len() as u64);
    assert_eq!(info["sha256"], sha256_hex(&bytes));
    assert_eq!(info["file_name"], "pantilt.glb");
    assert_eq!(info["scale"], 1.0);
    assert_eq!(info["up_axis"], "y");
    assert_eq!(info["color"], "");
    assert_eq!(
        info["origin"],
        jvalue!({"translation": {"x": 0.0, "y": 0.0, "z": 0.0}, "rotation": {"x": 0.0, "y": 0.0, "z": 0.0, "w": 1.0}})
    );
    // ContainerModelInfo の型に合う
    container_model_type_schemas().unwrap()[0]
        .check_value(&info)
        .unwrap();
    // ディレクトリは外に出さない
    assert!(!info.to_string().contains("models/"));

    // glTF でないもの・版が 2 でないもの
    dir.write("bad.glb", b"PK\x03\x04aaaaaaaa");
    assert!(is_invalid(load(&dir, ModelManifest::new("bad.glb"))));
    let mut v1 = glb_bytes(0);
    v1[4] = 1;
    dir.write("v1.glb", &v1);
    assert!(is_invalid(load(&dir, ModelManifest::new("v1.glb"))));
    dir.write("short.glb", b"glTF");
    assert!(is_invalid(load(&dir, ModelManifest::new("short.glb"))));
}

#[test]
fn conf_values_go_to_info() {
    let dir = TempDir::new();
    dir.write("camera.STL", &binary_stl(2));
    let m = load(
        &dir,
        ModelManifest::from_value(&jvalue!({
            "path": "camera.STL",
            "scale": 0.001,
            "origin": {"translation": {"z": -0.02}},
            "color": "#404040",
        }))
        .unwrap(),
    )
    .unwrap();
    let info = m.info();
    assert_eq!(info["format"], "stl");
    assert_eq!(info["media_type"], "model/stl");
    assert_eq!(info["scale"], 0.001);
    assert_eq!(info["origin"]["translation"]["z"], -0.02);
    assert_eq!(info["up_axis"], "z");
    assert_eq!(info["color"], "#404040");

    // up_axis を書けばそれを使う
    dir.write("zup.glb", &glb_bytes(0));
    let m = load(&dir, ModelManifest::new("zup.glb").up_axis("z")).unwrap();
    assert_eq!(m.info()["up_axis"], "z");
}

#[test]
fn stl_binary_and_ascii() {
    let dir = TempDir::new();
    dir.write("b.stl", &binary_stl(3));
    assert!(load(&dir, ModelManifest::new("b.stl")).is_ok());
    dir.write("a.stl", b"solid cube\nendsolid cube\n");
    assert!(load(&dir, ModelManifest::new("a.stl")).is_ok());
    // 大きさが 84 + 50n でなく、solid でも始まらない
    let mut broken = binary_stl(3);
    broken.push(0);
    dir.write("broken.stl", &broken);
    assert!(is_invalid(load(&dir, ModelManifest::new("broken.stl"))));
}

#[test]
fn gltf_needs_asset_version_2() {
    let dir = TempDir::new();
    dir.write(
        "m.gltf",
        br#"{"asset": {"version": "2.0"}, "buffers": [{"uri": "data:application/octet-stream;base64,AAAA"}]}"#,
    );
    assert!(load(&dir, ModelManifest::new("m.gltf")).is_ok());
    // 外部参照は警告だけ
    dir.write("ext.gltf", br#"{"asset": {"version": "2.0"}, "buffers": [{"uri": "m.bin"}]}"#);
    assert!(load(&dir, ModelManifest::new("ext.gltf")).is_ok());
    dir.write("v1.gltf", br#"{"asset": {"version": "1.0"}}"#);
    assert!(is_invalid(load(&dir, ModelManifest::new("v1.gltf"))));
    dir.write("no.gltf", b"not json");
    assert!(is_invalid(load(&dir, ModelManifest::new("no.gltf"))));
}

#[test]
fn sdf_needs_one_model_under_sdf() {
    let dir = TempDir::new();
    dir.write("p.sdf", SDF.as_bytes());
    let m = load(&dir, ModelManifest::new("p.sdf")).unwrap();
    assert_eq!(m.info()["media_type"], "application/xml");
    assert_eq!(m.info()["up_axis"], "z");

    dir.write(
        "world.sdf",
        br#"<sdf version="1.9"><world name="w"><model name="a"/></world></sdf>"#,
    );
    assert!(is_invalid(load(&dir, ModelManifest::new("world.sdf"))));
    dir.write(
        "two.sdf",
        br#"<sdf version="1.9"><model name="a"/><model name="b"/></sdf>"#,
    );
    assert!(is_invalid(load(&dir, ModelManifest::new("two.sdf"))));
    dir.write("robot.sdf", br#"<robot name="r"><model name="a"/></robot>"#);
    assert!(is_invalid(load(&dir, ModelManifest::new("robot.sdf"))));
    dir.write("broken.sdf", b"<sdf><model name=\"a\"></sdf>");
    assert!(is_invalid(load(&dir, ModelManifest::new("broken.sdf"))));
}

#[test]
fn explicit_format_and_other() {
    let dir = TempDir::new();
    // 拡張子の無いファイルも format を書けば確かめる
    dir.write("pantilt_model", &glb_bytes(0));
    let m = load(&dir, ModelManifest::new("pantilt_model").format(ModelFormat::Glb)).unwrap();
    assert_eq!(m.info()["format"], "glb");
    assert!(is_invalid(load(
        &dir,
        ModelManifest::new("pantilt_model").format(ModelFormat::Stl)
    )));
    // .obj は other で何も確かめない
    dir.write("a.obj", b"v 0 0 0\n");
    let m = load(&dir, ModelManifest::new("a.obj")).unwrap();
    assert_eq!(m.info()["format"], "other");
    assert_eq!(m.info()["media_type"], "application/octet-stream");
    // .urdf は警告して other
    dir.write("r.urdf", b"<robot/>");
    assert_eq!(load(&dir, ModelManifest::new("r.urdf")).unwrap().info()["format"], "other");
}

#[test]
fn missing_file_and_size_limit() {
    let dir = TempDir::new();
    assert_eq!(error_of(load(&dir, ModelManifest::new("none.glb"))), "not_found");
    std::fs::create_dir_all(dir.0.join("d.glb")).unwrap();
    assert_eq!(error_of(load(&dir, ModelManifest::new("d.glb"))), "not_found");
    dir.write("big.glb", &glb_bytes(100));
    let r = ContainerModel::load(&ModelManifest::new("big.glb"), Some(&dir.0), 50);
    assert!(is_invalid(r));
    // 絶対パスはそのまま
    let abs = dir.write("abs.glb", &glb_bytes(0));
    assert!(ContainerModel::load(
        &ModelManifest::new(abs.to_str().unwrap()),
        Some(Path::new("/nonexistent")),
        DEFAULT_MODEL_MAX_BYTES
    )
    .is_ok());
}

#[test]
fn registry_follows_file_changes() {
    let dir = TempDir::new();
    let path = dir.write("m.glb", &glb_bytes(10));
    let models = ContainerModels::new();
    models
        .insert("c0", load(&dir, ModelManifest::new("m.glb")).unwrap())
        .unwrap();
    assert!(models.info("none").unwrap().is_none());
    assert!(models.read("none").unwrap().is_none());

    let d = models.read("c0").unwrap().unwrap();
    assert_eq!(d.data, glb_bytes(10));
    assert_eq!(d.sha256(), sha256_hex(&glb_bytes(10)));

    // 書き換えると大きさが変わり、SHA-256 も変わる
    std::fs::write(&path, glb_bytes(20)).unwrap();
    let info = models.info("c0").unwrap().unwrap();
    assert_eq!(info["size"], glb_bytes(20).len() as u64);
    assert_eq!(info["sha256"], sha256_hex(&glb_bytes(20)));
    assert_eq!(models.read("c0").unwrap().unwrap().data, glb_bytes(20));

    // 形式が合わなくなったら取り出しはエラー、戻せば元どおり
    std::fs::write(&path, b"broken!!").unwrap();
    assert!(models.info("c0").is_err());
    assert!(models.read("c0").is_err());
    std::fs::remove_file(&path).unwrap();
    let e = models.read("c0").unwrap_err();
    assert!(matches!(
        e.downcast_ref::<JuizError>(),
        Some(JuizError::ContainerModelFileNotFoundError { .. })
    ));
    std::fs::write(&path, glb_bytes(5)).unwrap();
    assert_eq!(models.read("c0").unwrap().unwrap().data, glb_bytes(5));

    // 上限を下げると取り出しもエラー
    models.set_max_bytes(10).unwrap();
    assert!(models.info("c0").is_err() || {
        // 更新時刻と大きさが同じなら確かめ直さない。書き直して確かめさせる
        std::fs::write(&path, glb_bytes(6)).unwrap();
        models.info("c0").is_err()
    });

    assert!(models.remove("c0").unwrap().is_some());
    assert!(!models.contains("c0").unwrap());
}

#[test]
fn json_form_round_trip() {
    let d = ContainerModelData {
        info: jvalue!({"sha256": "ab", "media_type": "model/stl", "file_name": "a.stl"}),
        data: vec![0, 1, 2, 250, 251, 252, 253],
    };
    let v = d.to_value();
    assert_eq!(v["data"], "AAEC+vv8/Q==");
    let back = ContainerModelData::from_value(&v).unwrap();
    assert_eq!(back.data, d.data);
    assert_eq!(back.sha256(), "ab");
    assert_eq!(back.media_type(), "model/stl");
    assert_eq!(back.file_name(), "a.stl");
    assert!(ContainerModelData::from_value(&jvalue!({"info": {}})).is_err());
    assert!(ContainerModelData::from_value(&jvalue!({"info": {}, "data": "!!"})).is_err());
}

#[test]
fn max_bytes_option() {
    assert_eq!(max_bytes_from_options(None).unwrap(), DEFAULT_MODEL_MAX_BYTES);
    assert_eq!(
        max_bytes_from_options(Some(&jvalue!({"model": {"max_bytes": 1024}}))).unwrap(),
        1024
    );
    assert!(max_bytes_from_options(Some(&jvalue!({"model": {"max_bytes": 0}}))).is_err());
    assert!(max_bytes_from_options(Some(&jvalue!({"model": {"max_bytes": "1"}}))).is_err());
}
