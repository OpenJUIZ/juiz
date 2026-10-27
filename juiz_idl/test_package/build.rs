fn main() {
    juiz_idl::build::configure()
        .avsc(true)
        .processes(true)
        .rust_skeleton(true)
        .compile(&["idl/geometry.jidl"])
        .unwrap();
}
