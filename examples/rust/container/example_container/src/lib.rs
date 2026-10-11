use juiz_sdk::prelude::*;

#[repr(Rust)]
pub struct ExampleContainer {
    pub value: i64,
}

#[juiz_container]
fn example_container(initial_value: i64) -> JuizResult<Box<ExampleContainer>> {
    Ok(Box::new(ExampleContainer {
        value: initial_value,
    }))
}
