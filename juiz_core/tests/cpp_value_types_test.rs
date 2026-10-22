//! C++ のプロセスが配列とオブジェクトの引数 (中の bool / int / 文字列 / 入れ子) を
//! 正しく受け取れることを確かめるテスト (issue #130)
//!
//! C++ 側は libjuiz_sdk の value_array_foreach / value_object_foreach の戻り値と value_get_bool を使う。
//! 以前は C++ の宣言が Rust の定義と合っておらず、戻り値が無い関数の戻り値を比べたり、
//! bool (1 バイト) を int で受けて上位バイトが不定になったりしていた。
//! また、入れ子の配列を受け取ると要素がすべて true になっていた (juiz::Value::emplace_back の不具合)。
//!
//! C++ の例は scripts/test.sh (CI も同じ) で ../target にビルドされている前提。

use juiz_core::prelude::*;

fn system_manifest() -> Value {
    jvalue!({
        "name": "cpp_value_types_test",
        "plugins": {
            "process_factories": {
                "value_types_process_cpp": { "language": "c++", "path": "../target" }
            }
        },
        "processes": [
            { "type_name": "value_types_process_cpp", "name": "types0" }
        ]
    })
}

#[test]
fn cpp_process_receives_array_and_object_args() -> JuizResult<()> {
    let _ = env_logger::try_init();
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|system| {
            let cb = system.core_broker().lock()?;
            let id = cb
                .process_list(false, None)?
                .into_iter()
                .find(|id| id.name == "types0")
                .expect("process types0 not found");
            let cases = [
                (jvalue!([]), jvalue!({}), "[] {}"),
                (
                    jvalue!([false, true, false, 3, "s"]),
                    jvalue!({"f": false, "t": true, "i": -2}),
                    "[false,true,false,3,'s'] {f:false,i:-2,t:true}",
                ),
                // 入れ子の配列の要素がすべて true になっていた (juiz::Value::emplace_back の不具合)
                (
                    jvalue!([[false, 2, "s"], {"b": false}]),
                    jvalue!({"l": [false, true], "o": {"b": false}}),
                    "[[false,2,'s'],{b:false}] {l:[false,true],o:{b:false}}",
                ),
            ];
            for (list, obj, expected) in cases {
                let args = vec![("list", list.clone()), ("obj", obj.clone())];
                let r = cb
                    .process_call(&id, args.into())?
                    .lock_as_value(|v| v.clone())?;
                assert_eq!(r, jvalue!(expected), "list={list} obj={obj}");
            }
            Ok(())
        })
}
