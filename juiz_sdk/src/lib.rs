//! juizの機能要素開発のためのパッケージ
//! 
//! 
//! # 使い方
//! プロセス、コンテナ、コンテナプロセスおよびコンポーネントの実体の関数定義に宣言型マクロとして使います。
//! 
//! ## Cargo.toml
//! ```ignore
//! [package]
//! name = "listener"
//! version = "0.1.0"
//! edition = "2021"
//! 
//! # See more keys and their definitions at https://doc.rust-lang.org/cargo/reference/manifest.html
//! 
//! [lib]
//! crate-type = ["cdylib"]
//! path="src/lib.rs"
//! 
//! [dependencies]
//! juiz_sdk = "*"
//! ```
//! 
//! ## プロセスの例
//! 
//! ### lib.rs
//! ```ignore
//! #[juiz_process(
//!     description = "This is listener process."
//!     arguments = {
//!         default = {
//!             arg1 = "Hello, Juiz!"
//!         }
//!         description = {
//!             arg1 = "This message is printed."
//!         }
//!     }
//! )]
//! fn listener(arg1: String) -> JuizResult<Capsule> {
//!     log::trace!("listener({:}) called", arg1);
//!     println!("listener: {:}", arg1);
//!     return Ok(jvalue!("Hello World").into());
//! }
//! ```
//! ## コンテナの例
//! 
//! ```ignore
//! use juiz_sdk::prelude::*;
//! #[repr(Rust)]
//! pub struct ExampleContainer {
//!     pub value: i64
//! }
//! 
//! #[juiz_container(
//!     description = "This is description for container."
//!     arguments = {
//!         default = {
//!             initial_value = 0
//!         }
//!         description = {
//!             initial_value = "Default value of container included value."
//!         }
//!     }
//! )]
//! fn example_container(initial_value: i64) -> JuizResult<Box<ExampleContainer>> {
//!     Ok(Box::new(ExampleContainer{value:initial_value}))
//! }
//! ```
//! 
//! ## コンテナプロセスの例
//! 
//! コンテナプロセス定義のためのマクロ
//!
//! # Examples
//!
//! ```ignore
//! use example_container::ExampleContainer;
//! use juiz_sdk::prelude::*;
//! 
//! #[juiz_container_process(
//!     container_type = "example_container"
//!     description = "Container Process for example_container. This process will add given value to container."
//!     arguments = {
//!         default = {
//!             arg1 = 1
//!         }
//!     }
//! )]
//! fn increment_function(container: &mut ContainerImpl<ExampleContainer>, arg1: i64) -> JuizResult<Capsule> {
//!     container.value = container.value + arg1;
//!     return Ok(jvalue!(container.value).into());
//! }
//! 
//! ```
//! 
//! ## コンポーネントの例
//! 
//! ```ignore
//! use juiz_sdk::prelude::*;
//! #[juiz_component_process]
//! fn example_component_increment(arg1: i64) -> JuizResult<Capsule> {
//!     log::trace!("increment_process({:?}) called", arg1);
//!     return Ok(jvalue!(arg1+1).into());
//! }
//!
//! #[repr(Rust)]
//! pub struct ExampleComponentContainer {
//!     pub value: i64
//! }
//! 
//! #[juiz_component_container]
//! fn example_component_container(initial_value: i64) -> JuizResult<Box<ExampleComponentContainer>> {
//!     println!("example_component_container({initial_value}) called");
//!     Ok(Box::new(ExampleComponentContainer{value: initial_value}))
//! }
//! 
//! #[juiz_component_container_process( container_type = "example_component_container" )]
//! fn example_component_container_get(container: &mut ContainerImpl<ExampleComponentContainer>) -> JuizResult<Capsule> {
//!     println!("example_component_container_get()");
//!     Ok(jvalue!(container.value).into())
//! }
//! 
//! #[juiz_component_container_process( container_type = "example_component_container" )]
//! fn example_component_container_increment(container: &mut ContainerImpl<ExampleComponentContainer>) -> JuizResult<Capsule> {
//!     println!("example_component_container_increment()");
//!     container.value = container.value + 1;
//!     Ok(jvalue!(container.value).into())
//! }   
//! 
//! #[juiz_component_container_process( container_type = "example_component_container" 
//!    arguments = {
//!       default = {
//!         arg1 = 1
//!       }
//!    }
//! )]
//! fn example_component_container_add(container: &mut ContainerImpl<ExampleComponentContainer>, arg1: i64) -> JuizResult<Capsule> {
//!     println!("example_component_container_add({arg1})");
//!     container.value = container.value + arg1;
//!     Ok(jvalue!(container.value).into())
//! }
//! 
//! juiz_component_manifest!(
//!     component_name = "example_component"
//!     containers = {
//!         example_component_container = [
//!             example_component_container_get,
//!             example_component_container_increment,
//!             example_component_container_add
//!         ]
//!     }
//!     processes = [
//!         example_component_increment
//!     ]
//! );
//! 
//! ```

pub mod identifier;
pub mod connection_identifier;
pub mod process_identifier;
pub mod topic_identifier;
pub mod value;
pub mod result;
pub mod manifests;
pub mod prelude;
pub mod utils;
pub mod factory;
pub mod containers;
pub mod object;
pub mod processes;
pub mod connections;
pub mod geometry;

pub use env_logger;
pub use log;
pub use image;
pub use anyhow;
pub use serde;
pub use serde_json;
