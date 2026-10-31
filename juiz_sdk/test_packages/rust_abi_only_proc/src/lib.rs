//! C ABI の入口 (`process_factory_cabi`) を持たない、古い形の Rust プラグイン
//! (docs/spec/rust-plugin-abi.md §11、issue #330)。
//!
//! juiz_sdk 0.0.29 までの juiz_macro が生成していたものと同じ入口 (`process_factory` と
//! `process_factory_build_info`) だけを手で書く。ホストはこれを今の Rust ABI と BuildInfo の検査で読む。
use juiz_sdk::prelude::*;

fn manifest() -> ProcessManifest {
    ProcessManifest::new("rust_abi_only_proc")
        .description("a plugin without the C ABI entry")
        .add_int_arg("arg1", "", 1)
}

fn rust_abi_only_proc(args: CapsuleMap) -> JuizResult<Capsule> {
    let arg1 = args.get_int("arg1")?;
    Ok(jvalue!(arg1 + 100).into())
}

#[no_mangle]
pub extern "C" fn process_factory_build_info() -> *const ::std::os::raw::c_char {
    juiz_sdk::build_info::build_info_ptr()
}

#[no_mangle]
pub unsafe extern "Rust" fn process_factory() -> JuizResult<ProcessFactoryStruct> {
    Ok(juiz_sdk::prelude::process_factory(manifest(), rust_abi_only_proc))
}
