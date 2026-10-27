fn main() {
    juiz_idl::build::configure()
        .avsc(true)
        .processes(true)
        .compile(&["idl/geometry.jidl"])
        .unwrap();
}
