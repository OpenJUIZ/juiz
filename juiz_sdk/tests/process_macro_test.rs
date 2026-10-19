//! #[juiz_process] / #[juiz_container_process] が生成する引数の取り出しのテスト (issue #91)
//!
//! マクロが生成するファクトリ関数は #[no_mangle] なので、一つのテストバイナリに置けるのは
//! juiz_process と juiz_container_process をそれぞれ一つまで。

use juiz_sdk::prelude::*;

mod sink {
    use juiz_sdk::prelude::*;

    #[juiz_process(arguments = { default = { msg = {}, n = 0 } })]
    pub fn sink(msg: Value, n: i64) -> JuizResult<Capsule> {
        Ok(jvalue!({"t": msg.get("t"), "msg": msg, "n": n}).into())
    }
}

pub struct Store {
    last: Value,
}

// juiz_container_process は container_type のモジュールの manifest2() を参照する。
mod store {
    use juiz_sdk::prelude::*;

    pub fn manifest2() -> ContainerManifest {
        ContainerManifest::new("store")
    }
}

mod store_set {
    use super::{store, Store};
    use juiz_sdk::prelude::*;

    #[juiz_container_process(container_type = "store", arguments = { default = { msg = {} } })]
    pub fn store_set(container: &mut ContainerImpl<Store>, msg: Value) -> JuizResult<Capsule> {
        container.last = msg.clone();
        Ok(msg.into())
    }
}

fn capsule_value(c: Capsule) -> Value {
    c.as_value().unwrap().clone()
}

#[test]
fn value_argument_of_process_is_not_wrapped() {
    let args: CapsuleMap = vec![("msg", jvalue!({"t": 5})), ("n", jvalue!(3))].into();
    assert_eq!(
        capsule_value(sink::sink(args).unwrap()),
        jvalue!({"t": 5, "msg": {"t": 5}, "n": 3})
    );
}

#[test]
fn value_argument_of_container_process_is_not_wrapped() {
    let mut c = ContainerImpl::new(
        ContainerManifest::new("store").name("store0"),
        Box::new(Store {
            last: jvalue!(null),
        }),
    )
    .unwrap();
    let args: CapsuleMap = vec![("msg", jvalue!({"t": 7}))].into();
    assert_eq!(
        capsule_value(store_set::store_set(&mut c, args).unwrap()),
        jvalue!({"t": 7})
    );
    assert_eq!(c.last, jvalue!({"t": 7}));
}
