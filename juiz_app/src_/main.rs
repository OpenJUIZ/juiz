use juiz_sdk::prelude::*;
use juiz_sdk::serde_json;

#[juiz_process(
description = "This is process document"
arguments = {
    default = {
        arg1 = 1
        arg2 = "foo"
        arg3 = 3.4
        arg4 = {
            value1 = "here"
            value2 = "foo"
            value3 = "yeah"
            value4 = 3
        }
    },
    description = {
        arg1 = "Comment of arg1"
        arg2 = "Comment of arg2"
    }
}
)]
fn hoge_function(arg1: i64, arg2: String, arg3: f64, arg4: Value) -> JuizResult<Capsule> {
    println!("hoge function {arg1:?}");
    println!("hoge function {arg2:?}");
    println!("hoge function {arg3:?}");
    println!("hoge function {arg4:?}");
    Ok("ok".into())
}

pub fn main() -> Result<(), anyhow::Error> {
    let mut cm = CapsuleMap::new();
    cm.insert("arg1".to_owned(), jvalue!(32).into());
    cm.insert("arg2".to_owned(), jvalue!("Hellow World").into());
    cm.insert("arg3".to_owned(), jvalue!(32.4).into());
    cm.insert("arg4".to_owned(), jvalue!({"hello": "world"}).into());
    let v = hoge_function(cm);
    println!("v = {v:?}");

    let vv = manifest2();
    println!("manifest = {vv:?}");
    Ok(())
}
