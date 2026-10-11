use juiz_sdk::prelude::*;

#[juiz_component_process]
fn example_component_increment(arg1: i64) -> JuizResult<Capsule> {
    log::trace!("increment_process({:?}) called", arg1);
    return Ok(jvalue!(arg1 + 1).into());
}

#[repr(Rust)]
pub struct ExampleComponentContainer {
    pub value: i64,
}

#[juiz_component_container]
fn example_component_container(initial_value: i64) -> JuizResult<Box<ExampleComponentContainer>> {
    println!("example_component_container({initial_value}) called");
    Ok(Box::new(ExampleComponentContainer {
        value: initial_value,
    }))
}

#[juiz_component_container_process(container_type = "example_component_container")]
fn example_component_container_get(
    container: &mut ContainerImpl<ExampleComponentContainer>,
) -> JuizResult<Capsule> {
    println!("example_component_container_get()");
    Ok(jvalue!(container.value).into())
}

#[juiz_component_container_process(container_type = "example_component_container")]
fn example_component_container_increment(
    container: &mut ContainerImpl<ExampleComponentContainer>,
) -> JuizResult<Capsule> {
    println!("example_component_container_increment()");
    container.value = container.value + 1;
    Ok(jvalue!(container.value).into())
}

#[juiz_component_container_process( container_type = "example_component_container" 
   arguments = {
      default = {
        arg1 = 1
      }
   }
)]
fn example_component_container_add(
    container: &mut ContainerImpl<ExampleComponentContainer>,
    arg1: i64,
) -> JuizResult<Capsule> {
    println!("example_component_container_add({arg1})");
    container.value = container.value + arg1;
    Ok(jvalue!(container.value).into())
}

juiz_component_manifest!(
    component_name = "example_component"
    containers = {
        example_component_container = [
            example_component_container_get,
            example_component_container_increment,
            example_component_container_add
        ]
    }
    processes = [
        example_component_increment
    ]
);
