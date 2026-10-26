//! Composition の定義と manifest 生成のテスト (issue #166、仕様 docs/spec/composition.md)

use std::collections::HashMap;

use juiz_sdk::manifests::{
    CompositeArgument, CompositeNode, Composition, ExposeSpec, OutputProfile,
    COMPOSITE_CONTAINER_PROCESS_FACTORY_NAME, COMPOSITE_CONTAINER_TYPE_NAME,
    COMPOSITE_PROCESS_FACTORY_NAME,
};
use juiz_sdk::prelude::*;

/// opencv サンプル相当の子の manifest
fn children() -> HashMap<String, ProcessManifest> {
    let list = vec![
        ProcessManifest::new("video_capture_readandget")
            .container(ContainerManifest::new("video_capture"))
            .set_outputs(None),
        ProcessManifest::new("cv_cvt_color")
            .add_image_arg("img", "input image")
            .add_string_arg("code", "conversion code", "RGB2GRAY"),
        ProcessManifest::new("filesystem_imwrite")
            .container(ContainerManifest::new("filesystem"))
            .add_image_arg("img", "image to save")
            .add_string_arg("filename", "file name", "image.png"),
        ProcessManifest::new("add")
            .add_int_arg("a", "first", 1)
            .add_int_arg("b", "second", 2),
        ProcessManifest::new("scale")
            .add_int_arg("x", "value", 0)
            .add_float_arg("gain", "gain", 1.0),
        ProcessManifest::new("py_increment")
            .language("python")
            .add_int_arg("a", "value", 0),
    ];
    list.into_iter().map(|m| (m.type_name.clone(), m)).collect()
}

fn lookup(map: &HashMap<String, ProcessManifest>) -> impl Fn(&str) -> Option<ProcessManifest> + '_ {
    move |t| map.get(t).cloned()
}

fn capture_gray_save_value() -> Value {
    jvalue!({
        "type_name": "capture_gray_save",
        "description": "1 枚撮ってグレーにして保存する",
        "containers": { "camera": "video_capture", "fs": "filesystem" },
        "body": {
            "process": "filesystem_imwrite",
            "container": "fs",
            "args": {
                "img": {
                    "process": "cv_cvt_color",
                    "args": {
                        "img": { "process": "video_capture_readandget", "container": "camera" },
                        "code": { "const": "BGR2GRAY" }
                    }
                },
                "filename": { "expose": "path" }
            }
        }
    })
}

fn error_of(r: JuizResult<ProcessManifest>) -> (String, String) {
    let e = r.expect_err("must be an error");
    match e.downcast_ref::<JuizError>() {
        Some(JuizError::CompositeManifestInvalidError { path, message, .. }) => {
            (path.clone(), message.clone())
        }
        _ => panic!("unexpected error: {e}"),
    }
}

#[test]
fn container_tree_from_conf_value() -> JuizResult<()> {
    let children = children();
    let c: CompositeManifest = serde_json::from_value(capture_gray_save_value())?;
    assert!(c.has_containers());
    let m = c.build_process_manifest(lookup(&children))?;

    assert_eq!(m.type_name, "capture_gray_save");
    assert_eq!(m.description.as_str(), "1 枚撮ってグレーにして保存する");
    assert_eq!(m.factory, COMPOSITE_CONTAINER_PROCESS_FACTORY_NAME);
    assert_eq!(
        m.container_type.as_deref(),
        Some(COMPOSITE_CONTAINER_TYPE_NAME)
    );
    assert_eq!(m.language, "rust");
    // code は const なので公開しない。filename は path に名前を変えて公開
    let names: Vec<&str> = m.arguments.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, vec!["path"]);
    assert_eq!(m.arguments[0].type_name, ArgumentType::String);
    assert_eq!(m.arguments[0].default, jvalue!("image.png"));
    assert_eq!(m.arguments[0].description.as_str(), "file name");

    // 正規化した木: 書かなかった引数も expose で現れ、すべてのノードに args がある
    let comp = m.composition.as_ref().unwrap();
    let v = serde_json::to_value(comp)?;
    assert_eq!(
        v,
        jvalue!({
            "containers": { "camera": "video_capture", "fs": "filesystem" },
            "body": {
                "process": "filesystem_imwrite",
                "container": "fs",
                "args": {
                    "img": {
                        "process": "cv_cvt_color",
                        "args": {
                            "img": { "process": "video_capture_readandget", "container": "camera", "args": {} },
                            "code": { "const": "BGR2GRAY" }
                        }
                    },
                    "filename": { "expose": "path" }
                }
            }
        })
    );
    Ok(())
}

#[test]
fn pure_composite_exposes_in_evaluation_order() -> JuizResult<()> {
    let children = children();
    // scale(x = add(a, b=expose y), gain) → 公開は a, y, gain の順
    let c = CompositeManifest::new(
        "scaled_sum",
        CompositeNode::new("scale").arg_node(
            "x",
            CompositeNode::new("add").arg_expose("b", ExposeSpec::new("y")),
        ),
    );
    let m = c.build_process_manifest(lookup(&children))?;
    assert_eq!(m.factory, COMPOSITE_PROCESS_FACTORY_NAME);
    assert!(m.container_type.is_none());
    let names: Vec<&str> = m.arguments.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, vec!["a", "y", "gain"]);
    assert_eq!(m.arguments[1].default, jvalue!(2));
    assert!(m.composition.as_ref().unwrap().containers.is_empty());
    Ok(())
}

#[test]
fn outputs_come_from_root() -> JuizResult<()> {
    let mut children = children();
    let out = OutputProfile::default();
    children.get_mut("scale").unwrap().outputs = Some(out.clone());
    let c = CompositeManifest::new(
        "s",
        CompositeNode::new("scale").arg_node("x", CompositeNode::new("add")),
    );
    let m = c.build_process_manifest(lookup(&children))?;
    assert_eq!(
        serde_json::to_value(&m.outputs)?,
        serde_json::to_value(Some(out))?
    );
    Ok(())
}

#[test]
fn shared_name_with_same_type_is_one_argument() -> JuizResult<()> {
    let children = children();
    // add(a, b=add(a, b)) → a と b はどちらも Int なので共有
    let c = CompositeManifest::new(
        "add3",
        CompositeNode::new("add").arg_node("b", CompositeNode::new("add")),
    );
    let m = c.build_process_manifest(lookup(&children))?;
    let names: Vec<&str> = m.arguments.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, vec!["a", "b"]);
    // 既定値・説明は最初に出てきた子から
    assert_eq!(m.arguments[0].default, jvalue!(1));
    Ok(())
}

#[test]
fn collision_with_different_type_is_error() {
    let children = children();
    // scale(x = add(a=expose gain), gain) → gain が Int と Float で衝突
    let c = CompositeManifest::new(
        "bad",
        CompositeNode::new("scale").arg_node(
            "x",
            CompositeNode::new("add").arg_expose("a", ExposeSpec::new("gain")),
        ),
    );
    let (path, message) = error_of(c.build_process_manifest(lookup(&children)));
    assert_eq!(path, "body");
    assert!(message.contains("body.x (add)"), "{message}");
    assert!(message.contains("body (scale)"), "{message}");
    assert!(message.contains("expose"), "{message}");
}

#[test]
fn expose_object_form_overrides_default_and_description() -> JuizResult<()> {
    let children = children();
    let v = jvalue!({
        "type_name": "add_with_default",
        "body": {
            "process": "add",
            "args": {
                "a": { "expose": { "name": "first", "default": 10, "description": "最初" } },
                "b": { "const": 5 }
            }
        }
    });
    let c: CompositeManifest = serde_json::from_value(v)?;
    let m = c.build_process_manifest(lookup(&children))?;
    assert_eq!(m.arguments.len(), 1);
    assert_eq!(m.arguments[0].name, "first");
    assert_eq!(m.arguments[0].default, jvalue!(10));
    assert_eq!(m.arguments[0].description.as_str(), "最初");
    // オブジェクト形は往復できる
    let comp = serde_json::to_value(m.composition.as_ref().unwrap())?;
    assert_eq!(
        comp["body"]["args"]["a"],
        jvalue!({ "expose": { "name": "first", "default": 10, "description": "最初" } })
    );
    let back: Composition = serde_json::from_value(comp)?;
    assert_eq!(&back, m.composition.as_ref().unwrap());
    Ok(())
}

#[test]
fn const_and_default_are_type_checked() {
    let children = children();
    let c = CompositeManifest::new(
        "bad",
        CompositeNode::new("add").arg_const("a", jvalue!("x")),
    );
    let (path, message) = error_of(c.build_process_manifest(lookup(&children)));
    assert_eq!(path, "body");
    assert!(message.contains("const"), "{message}");

    let c = CompositeManifest::new(
        "bad",
        CompositeNode::new("add").arg_expose("a", ExposeSpec::new("a").default(jvalue!(1.5))),
    );
    let (_, message) = error_of(c.build_process_manifest(lookup(&children)));
    assert!(message.contains("default"), "{message}");
}

#[test]
fn nested_pure_composite_and_mixed_language() -> JuizResult<()> {
    let mut children = children();
    let inner = CompositeManifest::new(
        "add_then_inc",
        CompositeNode::new("py_increment").arg_node("a", CompositeNode::new("add")),
    )
    .build_process_manifest(lookup(&children))?;
    assert_eq!(inner.language, "mixed");
    children.insert(inner.type_name.clone(), inner);

    let outer = CompositeManifest::new(
        "outer",
        CompositeNode::new("scale").arg_node("x", CompositeNode::new("add_then_inc")),
    );
    let m = outer.build_process_manifest(lookup(&children))?;
    assert_eq!(m.language, "mixed");
    let names: Vec<&str> = m.arguments.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, vec!["a", "b", "gain"]);

    // 同じ言語だけならその言語
    let c = CompositeManifest::new(
        "rust_only",
        CompositeNode::new("scale").arg_node("x", CompositeNode::new("add")),
    );
    assert_eq!(
        c.build_process_manifest(lookup(&children))?.language,
        "rust"
    );
    Ok(())
}

#[test]
fn composite_with_slots_cannot_be_nested() -> JuizResult<()> {
    let mut children = children();
    let c: CompositeManifest = serde_json::from_value(capture_gray_save_value())?;
    let m = c.build_process_manifest(lookup(&children))?;
    children.insert(m.type_name.clone(), m);
    let outer = CompositeManifest::new("outer", CompositeNode::new("capture_gray_save"));
    let (path, message) = error_of(outer.build_process_manifest(lookup(&children)));
    assert_eq!(path, "body");
    assert!(message.contains("cannot be nested"), "{message}");
    Ok(())
}

#[test]
fn container_slot_rules() {
    let children = children();
    // スロットの書き忘れ
    let c = CompositeManifest::new("c", CompositeNode::new("video_capture_readandget"))
        .add_container("camera", "video_capture");
    let (_, message) = error_of(c.build_process_manifest(lookup(&children)));
    assert!(message.contains("'container'"), "{message}");

    // 宣言していないスロット
    let c = CompositeManifest::new(
        "c",
        CompositeNode::new("video_capture_readandget").container("cam"),
    )
    .add_container("camera", "video_capture");
    let (_, message) = error_of(c.build_process_manifest(lookup(&children)));
    assert!(message.contains("not declared"), "{message}");

    // スロットの型違い
    let c = CompositeManifest::new(
        "c",
        CompositeNode::new("video_capture_readandget").container("camera"),
    )
    .add_container("camera", "filesystem");
    let (_, message) = error_of(c.build_process_manifest(lookup(&children)));
    assert!(message.contains("needs 'video_capture'"), "{message}");

    // 普通の Process に container を書いた
    let c = CompositeManifest::new("c", CompositeNode::new("add").container("camera"))
        .add_container("camera", "video_capture");
    let (_, message) = error_of(c.build_process_manifest(lookup(&children)));
    assert!(message.contains("not a container process"), "{message}");

    // 使っていないスロット
    let c = CompositeManifest::new(
        "c",
        CompositeNode::new("video_capture_readandget").container("camera"),
    )
    .add_container("camera", "video_capture")
    .add_container("fs", "filesystem");
    let (path, message) = error_of(c.build_process_manifest(lookup(&children)));
    assert_eq!(path, "");
    assert!(message.contains("'fs'"), "{message}");

    // スロット名の検査
    let c = CompositeManifest::new(
        "c",
        CompositeNode::new("video_capture_readandget").container("a/b"),
    )
    .add_container("a/b", "video_capture");
    let (_, message) = error_of(c.build_process_manifest(lookup(&children)));
    assert!(message.contains("slot name"), "{message}");
}

#[test]
fn unknown_child_argument_and_self_reference() {
    let children = children();
    let c = CompositeManifest::new(
        "c",
        CompositeNode::new("scale").arg_node("x", CompositeNode::new("missing")),
    );
    let (path, message) = error_of(c.build_process_manifest(lookup(&children)));
    assert_eq!(path, "body.x");
    assert!(message.contains("'missing' is not found"), "{message}");

    let c = CompositeManifest::new("c", CompositeNode::new("add").arg_const("z", jvalue!(1)));
    let (_, message) = error_of(c.build_process_manifest(lookup(&children)));
    assert!(message.contains("argument 'z'"), "{message}");

    let c = CompositeManifest::new(
        "c",
        CompositeNode::new("scale").arg_node("x", CompositeNode::new("c")),
    );
    let (path, message) = error_of(c.build_process_manifest(lookup(&children)));
    assert_eq!(path, "body.x");
    assert!(message.contains("itself"), "{message}");

    let c = CompositeManifest::new(
        "c",
        CompositeNode::new("add").arg_expose("a", ExposeSpec::new("x:y")),
    );
    let (_, message) = error_of(c.build_process_manifest(lookup(&children)));
    assert!(message.contains("exposed name"), "{message}");
}

#[test]
fn argument_must_have_exactly_one_kind() {
    let bad = [
        jvalue!({ "process": "add", "args": { "a": {} } }),
        jvalue!({ "process": "add", "args": { "a": { "const": 1, "expose": "x" } } }),
        jvalue!({ "process": "add", "args": { "a": { "const": 1, "default": 2 } } }),
        jvalue!({ "process": "add", "args": { "a": { "process": "add", "typo": 1 } } }),
        jvalue!({ "process": "add", "args": { "a": 1 } }),
        jvalue!({ "process": "add", "args": { "a": { "expose": 1 } } }),
    ];
    for v in bad.into_iter() {
        assert!(
            serde_json::from_value::<CompositeNode>(v.clone()).is_err(),
            "{v} must be rejected"
        );
    }
    let ok: CompositeNode = serde_json::from_value(jvalue!({
        "process": "add",
        "args": { "a": { "process": "add" }, "b": { "const": 3 } }
    }))
    .unwrap();
    assert!(matches!(ok.args.get("a"), Some(CompositeArgument::Node(_))));
    assert!(matches!(
        ok.args.get("b"),
        Some(CompositeArgument::Const(_))
    ));
}

#[test]
fn composition_survives_manifest_and_profile_conversion() -> JuizResult<()> {
    let children = children();
    let c: CompositeManifest = serde_json::from_value(capture_gray_save_value())?;
    let m = c.build_process_manifest(lookup(&children))?;

    // serde で往復
    let v = serde_json::to_value(&m)?;
    let back: ProcessManifest = serde_json::from_value(v.clone())?;
    assert_eq!(back.composition, m.composition);
    // TryFrom<Value> でも読む
    let back2: ProcessManifest = v.try_into()?;
    assert_eq!(back2.composition, m.composition);

    // インスタンスの manifest と profile に引き継ぐ
    let inst = m.build_instance_manifest(ProcessManifest::new("x").name("cgs0"))?;
    assert_eq!(inst.composition, m.composition);
    let profile: ProcessProfile = inst.try_into()?;
    assert_eq!(profile.composition, m.composition);

    // Composite でない manifest には composition キーを出さない
    let plain = serde_json::to_value(children.get("add").unwrap())?;
    assert!(plain.get("composition").is_none());
    Ok(())
}
