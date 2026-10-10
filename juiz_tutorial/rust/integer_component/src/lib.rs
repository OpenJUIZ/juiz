
use juiz_sdk::prelude::*;


#[repr(Rust)]
pub struct RustIntegerContainer {
    pub value: i64
}

#[juiz_component_container]
fn rust_integer_container(initial_value: i64) -> JuizResult<Box<RustIntegerContainer>> {
    println!("rust_integer_container({initial_value}) called");
    Ok(Box::new(RustIntegerContainer{value: initial_value}))
}


#[juiz_component_container_process( 
    container_type = "rust_integer_container" )
]
fn rust_integer_container_get(container: &mut ContainerImpl<RustIntegerContainer>) -> JuizResult<Capsule> {
    println!("rust_integer_container_get()");
    Ok(jvalue!(container.value).into())
}


#[juiz_component_container_process( 
    container_type = "rust_integer_container"
    arguments = {
      default = {
        value = 1
      }
    }
 )]
fn rust_integer_container_add(container: &mut ContainerImpl<RustIntegerContainer>, value: i64) -> JuizResult<Capsule> {
    println!("rust_integer_container_add()");
    container.value = container.value + value;
    Ok(jvalue!(container.value).into())
}   

#[juiz_component_container_process( 
    container_type = "rust_integer_container" 
    arguments = {
      default = {
        value = 0
      }
    }
)]
fn rust_integer_container_set(container: &mut ContainerImpl<RustIntegerContainer>, value: i64) -> JuizResult<Capsule> {
    println!("rust_integer_container_set()");
    container.value = value;
    Ok(jvalue!(container.value).into())
}   

juiz_component_manifest!(
    component_name = "rust_integer_component"
    containers = {
        rust_integer_container = [
            rust_integer_container_get,
            rust_integer_container_set,
            rust_integer_container_add
        ]
    }
);
    

