use example_container::ExampleContainer;
use juiz_sdk::prelude::*;

#[juiz_container_process(
    container_type = "example_container"
    description = "Container Process for example_container. This process will add given value to container."
    arguments = {
        default = {
            arg1 = 1
        }
    }
)]
fn example_container_increment(container: &mut ContainerImpl<ExampleContainer>, arg1: i64) -> JuizResult<Capsule> {
    container.value = container.value + arg1;
    return Ok(jvalue!(container.value).into());
}
