
use juiz_core::{env_logger, prelude::*};
pub use example_container::ExampleContainer;


#[repr(Rust)]
pub struct ExampleContainerStack {
    pub name: String,
    pub container: ContainerPtr,
}

impl ExampleContainerStack {

    pub fn manifest() -> ContainerManifest {
        ContainerManifest::new("example_container_stack")
          .parent_type_name("example_container")
    }

}

fn create_example_container_ex(manifest: ContainerManifest, container_ptr: ContainerPtr) -> JuizResult<Box<ExampleContainerStack>> {
    // let my_name = match manifest.name {
    //     None => Err(anyhow!(JuizError::InvalidValueError{ message: "Argument profile does not include 'name'.".to_owned()})),
    //     Some(v) => {
    //         Ok(v.as_str())
    //     },
    // }?;
    Ok(Box::new(ExampleContainerStack{name: manifest.name.clone().unwrap(), container: container_ptr}))
}

#[no_mangle]
pub unsafe extern "Rust" fn container_factory() -> JuizResult<ContainerStackFactoryStruct> {
    let _ = env_logger::try_init();
    let manifest = ExampleContainerStack::manifest();
    Ok(container_stack_factory(manifest, create_example_container_ex))
}


