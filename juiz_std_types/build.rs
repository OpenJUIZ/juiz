// 型は juiz_idl に組み込んだ JIDL (juiz_idl/std/juiz_std/*.jidl) から作る (docs/spec/datatype.md §24.4)
fn main() {
    juiz_idl::build::configure()
        .compile(&[
            "juiz_std/common.jidl",
            "juiz_std/geometry.jidl",
            "juiz_std/sensor.jidl",
        ])
        .unwrap();
}
