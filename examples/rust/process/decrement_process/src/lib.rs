use juiz_sdk::{env_logger, prelude::*};

pub unsafe extern "Rust" fn manifest() -> ProcessManifest {
    ProcessManifest::new("decrement_process")
        .description("Example(decremnet_process)")
        .add_int_arg("arg1", "The output will be 'arg1 - 1'.", 1)
}

fn decrement_process(args: CapsuleMap) -> JuizResult<Capsule> {
    log::trace!("increment_process({:?}) called", args);
    let i = args.get_int("arg1")?;
    return Ok(jvalue!(i - 1).into());
}

#[no_mangle]
pub unsafe extern "Rust" fn process_factory(
) -> JuizResult<(ProcessManifest, fn(CapsuleMap) -> JuizResult<Capsule>)> {
    env_logger::init();
    Ok((manifest(), decrement_process))
    // process_factory_create(manifest(), decrement_process)
}
