use super::*;

/// テストごとの一時ディレクトリ
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let p =
            std::env::temp_dir().join(format!("juiz_model_check_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Dir(p)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.0.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn check(p: &Path) -> CheckReport {
    check_model_file(p, None, None, None).unwrap()
}

fn has(r: &CheckReport, text: &str) -> bool {
    r.warnings.iter().any(|w| w.contains(text))
}

fn close3(a: [f64; 3], b: [f64; 3]) -> bool {
    a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < 1e-9)
}

/// JSON チャンクだけの .glb
fn glb(json: &Value) -> Vec<u8> {
    let mut j = serde_json::to_vec(json).unwrap();
    while j.len() % 4 != 0 {
        j.push(b' ');
    }
    let mut v = b"glTF".to_vec();
    v.extend_from_slice(&2u32.to_le_bytes());
    v.extend_from_slice(&((12 + 8 + j.len()) as u32).to_le_bytes());
    v.extend_from_slice(&(j.len() as u32).to_le_bytes());
    v.extend_from_slice(b"JSON");
    v.extend(j);
    v
}

/// 三角形 1 つ (頂点は与えたもの) のバイナリ STL
fn binary_stl(vertices: [[f32; 3]; 3]) -> Vec<u8> {
    let mut v = vec![0u8; 80];
    v.extend_from_slice(&1u32.to_le_bytes());
    v.extend_from_slice(&[0u8; 12]);
    for p in vertices {
        for x in p {
            v.extend_from_slice(&x.to_le_bytes());
        }
    }
    v.extend_from_slice(&[0u8; 2]);
    v
}

fn gltf_with_box(min: [f64; 3], max: [f64; 3], node: Value) -> Value {
    jvalue!({
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [node],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
        "accessors": [{"min": min, "max": max}],
    })
}

#[test]
fn glb_bounds_follow_nodes_and_up_axis() {
    let d = Dir::new("glb");
    // glTF の「上」(+Y) 1 m は juiz の +Z 1 m (§4.2)
    let p = d.write(
        "up.glb",
        &glb(&gltf_with_box(
            [0.0, 0.0, 0.0],
            [0.2, 1.0, 0.5],
            jvalue!({"mesh": 0}),
        )),
    );
    let r = check(&p);
    assert_eq!(r.info["format"], "glb");
    assert_eq!(r.info["up_axis"], "y");
    assert!(close3(r.extent.unwrap(), [0.2, 0.5, 1.0]), "{r:?}");
    assert!(r.warnings.is_empty(), "{r:?}");
    // up_axis: z なら回さない
    let r = check_model_file(&p, None, Some("z"), None).unwrap();
    assert!(close3(r.extent.unwrap(), [0.2, 1.0, 0.5]), "{r:?}");

    // ノードの scale・回転・子を掛ける (mm の CAD から書き出したもの)
    let mut j = gltf_with_box(
        [0.0; 3],
        [1000.0, 1000.0, 1000.0],
        jvalue!({"children": [1], "scale": [0.001, 0.001, 0.001]}),
    );
    j["nodes"].as_array_mut().unwrap().push(jvalue!({"mesh": 0, "rotation": [0.0, 0.0, 0.7071067811865476, 0.7071067811865476], "translation": [5.0, 0.0, 0.0]}));
    j["scenes"] = jvalue!([{"nodes": [0]}]);
    let r = check(&d.write("nodes.glb", &glb(&j)));
    assert!(close3(r.extent.unwrap(), [1.0, 1.0, 1.0]), "{r:?}");
}

#[test]
fn gltf_external_references_and_size() {
    let d = Dir::new("gltf");
    let mut j = gltf_with_box([0.0; 3], [200.0, 1.0, 1.0], jvalue!({"mesh": 0}));
    j["buffers"] =
        jvalue!([{"uri": "data:application/octet-stream;base64,AAAA"}, {"uri": "mesh.bin"}]);
    j["images"] = jvalue!([{"uri": "tex.png"}]);
    let p = d.write("a.gltf", &serde_json::to_vec(&j).unwrap());
    let r = check(&p);
    assert!(
        has(&r, "buffers[1].uri refers to an external file 'mesh.bin'"),
        "{r:?}"
    );
    assert!(has(&r, "images[0].uri"), "{r:?}");
    assert!(!has(&r, "buffers[0]"), "{r:?}");
    assert!(has(&r, "very large"), "{r:?}");
    // glb の中の外部参照も (core は読まない、§3.3)
    let r = check(&d.write("a.glb", &glb(&j)));
    assert!(has(&r, "mesh.bin"), "{r:?}");
}

#[test]
fn stl_in_millimeters_needs_scale() {
    let d = Dir::new("stl");
    let p = d.write(
        "camera.stl",
        &binary_stl([[0.0, 0.0, 0.0], [120.0, 0.0, 0.0], [0.0, 40.0, 30.0]]),
    );
    let r = check(&p);
    assert_eq!(r.info["format"], "stl");
    assert!(close3(r.extent.unwrap(), [120.0, 40.0, 30.0]), "{r:?}");
    assert!(has(&r, "scale: 0.001"), "{r:?}");
    let r = check_model_file(&p, None, None, Some(0.001)).unwrap();
    assert!(close3(r.extent.unwrap(), [0.12, 0.04, 0.03]), "{r:?}");
    assert!(r.warnings.is_empty(), "{r:?}");
    // 小さすぎる
    let r = check_model_file(&p, None, None, Some(0.000001)).unwrap();
    assert!(has(&r, "very small"), "{r:?}");

    let ascii = "solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 0.5 0 0\nvertex 0 0.25 0.1\nendloop\nendfacet\nendsolid t\n";
    let r = check(&d.write("ascii.stl", ascii.as_bytes()));
    assert!(close3(r.extent.unwrap(), [0.5, 0.25, 0.1]), "{r:?}");
}

const SDF_OK: &str = r#"<sdf version="1.9">
  <model name="pantilt">
    <link name="body">
      <visual name="base">
        <pose>0 0 0.02 0 0 0</pose>
        <geometry><cylinder><radius>0.04</radius><length>0.04</length></cylinder></geometry>
        <material><diffuse>0.2 0.2 0.2 1</diffuse></material>
      </visual>
      <visual name="head">
        <pose>0 0 0.07 0 0 90</pose>
        <geometry><box><size>0.06 0.04 0.04</size></box></geometry>
      </visual>
    </link>
  </model>
</sdf>"#;

#[test]
fn sdf_subset_bounds() {
    let d = Dir::new("sdf_ok");
    let r = check(&d.write("pantilt.sdf", SDF_OK.as_bytes()));
    assert!(r.warnings.is_empty(), "{r:?}");
    // 円柱 z 0〜0.04、箱 z 0.05〜0.09。箱は yaw 90 rad で回るので x・y は円柱の 0.08 の内側
    let e = r.extent.unwrap();
    assert!((e[2] - 0.09).abs() < 1e-9, "{e:?}");
    assert!(
        (e[0] - 0.08).abs() < 1e-9 && (e[1] - 0.08).abs() < 1e-9,
        "{e:?}"
    );
    // degrees="true" で 90 度回すと箱の 0.06 が y 向きに
    let one = r#"<sdf version="1.9"><model name="m"><link name="l"><visual name="v">
        <pose degrees="true">0 0 0 0 0 90</pose>
        <geometry><box><size>0.6 0.2 0.1</size></box></geometry></visual></link></model></sdf>"#;
    let r = check(&d.write("deg.sdf", one.as_bytes()));
    assert!(close3(r.extent.unwrap(), [0.2, 0.6, 0.1]), "{r:?}");
}

#[test]
fn sdf_mistakes_are_warned() {
    let d = Dir::new("sdf_ng");
    d.write(
        "wheel.stl",
        &binary_stl([[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]),
    );
    let sdf = r#"<sdf version="1.9">
  <model name="robot">
    <pose>0 0 0 0 0</pose>
    <link name="base">
      <visual name="a">
        <pose relative_to="world">0 0 0 0 0 0</pose>
        <geometry><box><size>1 1</size></box></geometry>
        <material><diffuse>255 0 0 1</diffuse></material>
      </visual>
      <visual name="b"><geometry><cone><radius>1</radius></cone></geometry></visual>
      <visual name="c"><geometry><mesh><uri>wheel.stl</uri></mesh></geometry></visual>
      <visual name="d"><geometry><mesh><uri>model://wheel/meshes/w.dae</uri></mesh></geometry></visual>
      <visual name="e"><geometry><mesh><uri>missing.glb</uri></mesh></geometry></visual>
      <visual name="f"><geometry><sphere><radius>-1</radius></sphere></geometry></visual>
      <visual name="g"><pose>1 0 0 0 0 0</pose><geometry><plane><size>2 2</size></plane></geometry></visual>
    </link>
    <link name="arm"><pose relative_to="robot">0 0 1 0 0 0</pose></link>
    <joint name="j" type="revolute"><parent>base</parent><child>arm</child></joint>
    <model name="inner"><link name="x"/></model>
  </model>
</sdf>"#;
    let r = check(&d.write("robot.sdf", sdf.as_bytes()));
    for text in [
        "<pose> of <model>: needs 6 numbers",
        "relative_to=\"world\"",
        "<size> of <box> of <visual name=\"a\">",
        "<diffuse> of <visual name=\"a\">",
        "unknown geometry <cone> in <visual name=\"b\">",
        "<mesh> of <visual name=\"c\"> in <link name=\"base\"> is not shown",
        "'model://wheel/meshes/w.dae' is not supported",
        "'missing.glb' is not found",
        "<radius> of <sphere> of <visual name=\"f\"> in <link name=\"base\"> must be positive",
        "1 <joint> found",
        "nested <model name=\"inner\">",
    ] {
        assert!(has(&r, text), "missing '{text}' in {:#?}", r.warnings);
    }
    // relative_to に親の名前は書ける
    assert!(!has(&r, "relative_to=\"robot\""), "{:#?}", r.warnings);
    // mesh の wheel.stl はあるので「見つからない」とは言わない
    assert!(!has(&r, "'wheel.stl' is not found"), "{:#?}", r.warnings);
    // 外接箱は表示できる平面だけから (x 0〜2、y −1〜1、厚さ 0)
    assert!(close3(r.extent.unwrap(), [2.0, 2.0, 0.0]), "{r:?}");
}

#[test]
fn errors_of_the_core_check() {
    let d = Dir::new("errors");
    // 先頭の確かめに合わない (§3.1)
    let p = d.write("bad.glb", b"not a glb file");
    let e = check_model_file(&p, None, None, None).unwrap_err();
    assert!(format!("{e:#}").contains("not glb"), "{e:#}");
    // <world> は部分集合に入らない
    let p = d.write(
        "world.sdf",
        b"<sdf version=\"1.9\"><world name=\"w\"/></sdf>",
    );
    assert!(check_model_file(&p, None, None, None).is_err());
    // ファイルが無い
    assert!(check_model_file(&d.0.join("none.stl"), None, None, None).is_err());
    // 引数の誤り
    let p = d.write("ok.sdf", SDF_OK.as_bytes());
    assert!(check_model_file(&p, Some("obj"), None, None).is_err());
    assert!(check_model_file(&p, None, Some("x"), None).is_err());
    assert!(check_model_file(&p, None, None, Some(0.0)).is_err());
    // 形式の明示 (拡張子を見ない)
    let p = d.write("model.bin", SDF_OK.as_bytes());
    assert_eq!(
        check_model_file(&p, Some("sdf"), None, None).unwrap().info["format"],
        "sdf"
    );
    assert_eq!(check(&p).info["format"], "other");
    assert!(check(&p).extent.is_none());
}

#[test]
fn report_and_list_lines() {
    let d = Dir::new("report");
    let p = d.write("pantilt.sdf", SDF_OK.as_bytes());
    let s = format_report("pantilt.sdf", &check(&p));
    let lines: Vec<&str> = s.lines().collect();
    assert!(
        lines[0].starts_with("pantilt.sdf: sdf (application/xml), "),
        "{s}"
    );
    assert_eq!(lines[1], "  up_axis: z, scale: 1.0");
    assert_eq!(
        lines[2],
        "  size: 0.08 x 0.08 x 0.09 m (x, y, z after scale and up_axis)"
    );
    assert_eq!(lines[3], "OK");

    assert_eq!(human_size(1000), "1000 B");
    assert_eq!(human_size(1536), "1.5 KiB");
    assert_eq!(human_size(3 * 1024 * 1024 + 512 * 1024), "3.5 MiB");

    let mut profile = ContainerProfile::new("static_anchor", "a");
    assert!(model_lines(&profile).is_empty());
    profile.model = Some(Value::Null);
    assert!(model_lines(&profile).is_empty());
    profile.model = Some(jvalue!({"format": "glb", "size": 2048, "file_name": "pantilt.glb"}));
    assert_eq!(
        model_lines(&profile),
        vec!["model: glb 2.0 KiB (pantilt.glb)"]
    );
}

#[test]
fn write_model_to_file_or_directory() {
    let d = Dir::new("write");
    let data = b"solid x\nendsolid x\n".to_vec();
    let info = jvalue!({"sha256": juiz_core::models::sha256_hex(&data), "file_name": "camera.stl"});
    let m = ContainerModelData {
        info,
        data: data.clone(),
    };
    let p = write_model(&m, &d.0).unwrap();
    assert_eq!(p, d.0.join("camera.stl"));
    assert_eq!(std::fs::read(&p).unwrap(), data);
    let p = write_model(&m, &d.0.join("other.stl")).unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), data);
    // sha256 が合わなければ書かない
    let bad = ContainerModelData {
        info: jvalue!({"sha256": "00", "file_name": "x.stl"}),
        data,
    };
    assert!(write_model(&bad, &d.0).is_err());
    assert!(!d.0.join("x.stl").exists());
}
