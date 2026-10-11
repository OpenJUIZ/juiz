use anyhow::anyhow;
use example_container_stack::{ExampleContainer, ExampleContainerStack};
use juiz_sdk::{env_logger, prelude::*};

fn get_function(
    container: &mut ContainerImpl<ExampleContainerStack>,
    _v: CapsuleMap,
) -> JuizResult<Capsule> {
    let v = match container
        .container
        .lock_mut()?
        .downcast_mut::<ContainerImpl<ExampleContainer>>()
    {
        Some(cn) => Ok(cn.value),
        None => Err(anyhow!(JuizError::ContainerDowncastingError {
            identifier: "ContainerPtr".to_owned()
        })),
    }?;
    return Ok(jvalue!({
        "name": container.name,
        "value": v
    })
    .into());
}

#[no_mangle]
pub unsafe extern "Rust" fn container_process_factory() -> JuizResult<ContainerProcessFactoryStruct>
{
    env_logger::init();
    let manifest = ProcessManifest::new("example_container_stack_get")
        .description("Example(get)")
        .container(ExampleContainerStack::manifest());
    Ok(juiz_sdk::factory::container_process_factory(
        manifest,
        get_function,
    ))
}
