fn main() {
    // juiz_std の型は作り直さず juiz_std_types のものを使う (docs/spec/datatype.md §24.2)
    juiz_idl::build::configure()
        .extern_path("juiz_std", "::juiz_std_types::juiz_std")
        .compile(&["idl/robot.jidl"])
        .unwrap();
}
