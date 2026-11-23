//! 設計文書から作る雛形 (rust_design_items・python_design_items、docs/spec/design.md §6.2・§7.2)

use juiz_idl::{
    load_str, python_design_items, python_skeleton_items, rust_design_items, rust_skeleton_items,
    ArgumentSignature, DesignSkeleton, GenerateOptions, Signatures, StateInit, StateSignature,
};
use juiz_types::{DataType, TypeSchema};
use serde_json::json;

/// .jidl の宣言から作ったシグネチャに、設計文書だけが持つもの (Container の引数・状態・readonly) を足す
fn signatures() -> Signatures {
    let model = load_str(
        r#"namespace robot;
record Vec2 { double x = 0.0; double y = 0.0; }
/** Scales v. */
process robot.Vec2 scale(robot.Vec2 v, double k = 2.0);
container counter {
  container_process long add(long step = 1);
  container_process long get();
}
"#,
        "robot.jidl",
    )
    .unwrap();
    let mut s = model.signatures[0].clone();
    let c = &mut s.containers[0];
    let schema = |t: DataType| TypeSchema::from_data_type(&t).unwrap();
    let vec2 = model.get_type("robot.Vec2").unwrap().clone();
    c.arguments = vec![
        ArgumentSignature {
            name: "start".into(),
            type_name: "Int".into(),
            description: Some("the first count".into()),
            schema: schema(DataType::Long),
            default: Some(json!(10)),
        },
        ArgumentSignature {
            name: "label".into(),
            type_name: "String".into(),
            description: None,
            schema: schema(DataType::String),
            default: Some(json!("c")),
        },
    ];
    c.state = vec![
        StateSignature {
            name: "count".into(),
            schema: schema(DataType::Long),
            init: StateInit::Argument("start".into()),
            description: Some("the count".into()),
        },
        StateSignature {
            name: "label".into(),
            schema: schema(DataType::String),
            init: StateInit::Argument("label".into()),
            description: None,
        },
        StateSignature {
            name: "first_label".into(),
            schema: schema(DataType::optional(DataType::String)),
            init: StateInit::Argument("label".into()),
            description: None,
        },
        StateSignature {
            name: "history".into(),
            schema: schema(DataType::array(DataType::Long)),
            init: StateInit::Value(json!([])),
            description: None,
        },
        StateSignature {
            name: "last".into(),
            schema: schema(DataType::optional(vec2.clone())),
            init: StateInit::Value(json!(null)),
            description: None,
        },
        StateSignature {
            name: "origin".into(),
            schema: schema(vec2),
            init: StateInit::Value(json!({"x": 1.0, "y": 0.0})),
            description: None,
        },
        StateSignature {
            name: "scale".into(),
            schema: schema(DataType::Double),
            init: StateInit::Value(json!(0)),
            description: None,
        },
    ];
    c.container_processes[1].readonly = true;
    s
}

#[test]
fn rust_design_items_write_the_container_state_and_readonly() {
    let s = signatures();
    let mut d = DesignSkeleton::default();
    d.bodies.insert("add".into(), "container.count += step;\nOk(container.count)".into());
    d.tail = Some("#[cfg(test)]\nmod tests {}\n".into());
    let code = rust_design_items(&s, "my_robot", &GenerateOptions::default(), &d).unwrap();
    // 説明は属性にだけ、既定値は属性の書き方 (書けないものは JSON の文字列)
    assert!(code.starts_with("use juiz_sdk::prelude::*;\n\n#[juiz_component_process(\n    typed = \"all\"\n    description = \"Scales v.\"\n    arguments = { default = { k = 2.0 } }\n)]\nfn scale(v: crate::robot::Vec2, k: f64) -> JuizResult<crate::robot::Vec2> {\n    let _ = (v, k);\n    todo!(\"write the body of scale\")\n}\n"), "{code}");
    assert!(code.contains("/// State of the container `counter`.\n#[repr(Rust)]\npub struct Counter {\n    /// the count\n    pub count: i64,\n    pub label: ::std::string::String,\n    pub first_label: ::std::option::Option<::std::string::String>,\n"), "{code}");
    assert!(code.contains("    arguments = {\n        default = { start = 10 label = \"c\" }\n        description = {\n            start = \"the first count\"\n        }\n    }\n"), "{code}");
    // 状態の初期値: 引数 (2 回使う label は先の方を clone、T? には Some)、0 値、record は辞書の表し方から
    assert!(code.contains("fn counter(start: i64, label: ::std::string::String) -> JuizResult<Box<Counter>> {\n    Ok(Box::new(Counter {\n        count: start,\n        label: label.clone(),\n        first_label: Some(label),\n        history: ::std::vec::Vec::new(),\n        last: None,\n        origin: <crate::robot::Vec2 as ::juiz_sdk::types::JuizType>::from_value(&::juiz_sdk::prelude::serde_json::from_str::<::juiz_sdk::prelude::Value>(\"{\\\"x\\\":1.0,\\\"y\\\":0.0}\").expect(\"the initial value is JSON\"))?,\n        scale: 0.0,\n    }))\n}\n"), "{code}");
    // 本体の断片と readonly
    assert!(code.contains("fn add(container: &mut ContainerImpl<Counter>, step: i64) -> JuizResult<i64> {\n    container.count += step;\n    Ok(container.count)\n}\n"), "{code}");
    assert!(code.contains("/// Receiving the container as `&ContainerImpl` (not `&mut`) makes this ContainerProcess readonly.\n#[juiz_component_container_process(\n    container_type = \"counter\"\n    typed = \"all\"\n)]\nfn get(container: &ContainerImpl<Counter>) -> JuizResult<i64> {\n    let _ = container;\n    todo!(\"write the body of get\")\n}\n"), "{code}");
    assert!(code.ends_with("    containers = {\n        counter = [ add, get ]\n    }\n    processes = [ scale ]\n);\n\n#[cfg(test)]\nmod tests {}\n"), "{code}");
    // .jidl からの雛形は今までどおり (状態を型付けしない、pub fn、JSON の文字列の既定値)
    let jidl = rust_skeleton_items(&signatures(), "my_robot", &GenerateOptions::default()).unwrap();
    assert!(jidl.contains("pub struct Counter {}\n"), "{jidl}");
    assert!(jidl.contains("    arguments = {\n        default = \"{\\\"k\\\":2.0}\"\n    }\n"), "{jidl}");
}

#[test]
fn python_design_items_write_the_container_state_and_readonly() {
    let s = signatures();
    let mut d = DesignSkeleton::default();
    d.bodies.insert("add".into(), "container.count += step\nreturn container.count".into());
    let code = python_design_items(&s, &d).unwrap();
    assert!(code.starts_with("from juiz import juiz_container, juiz_container_process, juiz_process\n\nfrom robot import Vec2\n"), "{code}");
    // 状態のクラスは <Name>、引数から作る
    assert!(code.contains("class Counter:\n    \"\"\"State of the container counter.\"\"\"\n\n    def __init__(self, count: int, label: str, first_label: str | None, history: list[int], last: Vec2 | None, origin: Vec2, scale: float):\n        self.count = count  # the count\n        self.label = label\n"), "{code}");
    assert!(code.contains("@juiz_container\ndef counter(start: int = 10, label: str = \"c\") -> Counter:\n    return Counter(start, label, label, [], None, Vec2.from_value({\"x\": 1.0, \"y\": 0.0}), 0.0)\n"), "{code}");
    assert!(code.contains("@juiz_container_process(container_type=\"counter\")\ndef add(container: Counter, step: int = 1) -> int:\n    container.count += step\n    return container.count\n"), "{code}");
    assert!(code.contains("@juiz_container_process(container_type=\"counter\", readonly=True)\ndef get(container: Counter) -> int:\n    raise NotImplementedError(\"write the body of get\")\n"), "{code}");
    // .jidl からの雛形は今までどおり (<Name>State、readonly は書かない)
    let mut jidl_sigs = signatures();
    jidl_sigs.containers[0].container_processes[1].readonly = false;
    let jidl = python_skeleton_items(&jidl_sigs).unwrap();
    assert!(jidl.contains("class CounterState:"), "{jidl}");
}

#[test]
fn python_design_items_errors() {
    // Container の引数も pyjuiz の受ける型だけ (bytes は不可)
    let mut s = signatures();
    s.containers[0].arguments[0].schema = TypeSchema::from_data_type(&DataType::Bytes).unwrap();
    let e = python_design_items(&s, &DesignSkeleton::default()).unwrap_err().to_string();
    assert!(e.contains("counter argument start"), "{e}");
    // 既定値のある引数の後ろの、既定値の無い record の引数
    let mut s = signatures();
    s.containers[0].arguments[1].default = None;
    s.containers[0].arguments[1].schema = s.processes[0].arguments[0].schema.clone();
    let e = python_design_items(&s, &DesignSkeleton::default()).unwrap_err().to_string();
    assert!(e.contains("argument label has no default but comes after start"), "{e}");
    // bytes の状態は base64 で初期化する
    let mut s = signatures();
    s.containers[0].state = vec![StateSignature {
        name: "raw".into(),
        schema: TypeSchema::from_data_type(&DataType::Bytes).unwrap(),
        init: StateInit::Value(json!("AAE=")),
        description: None,
    }];
    let code = python_design_items(&s, &DesignSkeleton::default()).unwrap();
    assert!(code.starts_with("import base64\n\nfrom juiz import"), "{code}");
    assert!(code.contains("    return Counter(base64.b64decode(\"AAE=\"))\n"), "{code}");
}
