use example_container::ExampleContainer;
use juiz_sdk::prelude::*;

#[juiz_container_process(container_type = "example_container")]
fn example_container_get(container: &mut ContainerImpl<ExampleContainer>) -> JuizResult<Capsule> {
    println!("example_container_get() called");
    return Ok(jvalue!(container.value).into());
}
