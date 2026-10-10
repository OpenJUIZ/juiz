//! juizの機能要素開発のためのヘルパーマクロの定義のためのパッケージ
//! 
//! このパッケージはjuiz_sdkから使われる、機能要素定義用のマクロのパッケージです。
//! 
//! 
//! # 使い方
//! プロセス、コンテナ、コンテナプロセスおよびコンポーネントの実体の関数定義に宣言型マクロとして使います。
//! 
//! ## プロセスの例
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
//! 
//! 



extern crate proc_macro;

mod util;
mod process;
mod container;
mod container_process;
mod component;

use crate::proc_macro::TokenStream;


/// プロセス定義のためのマクロ
/// 
/// マクロの引数は省略可能です。
/// 
/// # Examples
///
/// ```ignore
/// #[juiz_process(
///     description = "This is listener process."
///     arguments = {
///         default = {
///             arg1 = "Hello, Juiz!"
///         }
///         description = {
///             arg1 = "This message is printed."
///         }
///     }
/// )]
/// fn listener(arg1: String) -> JuizResult<Capsule> {
///     log::trace!("listener({:}) called", arg1);
///     println!("listener: {:}", arg1);
///     return Ok(jvalue!("Hello World").into());
/// }
/// ```
#[proc_macro_attribute]
pub fn juiz_process(attr: TokenStream, item: TokenStream) -> TokenStream {
    process::juiz_process_inner(attr, item)
}


/// コンテナ定義のためのマクロ
/// 
/// マクロの引数は省略できます。
///
/// # Examples
///
/// ```ignore
/// use juiz_sdk::prelude::*;
/// #[repr(Rust)]
/// pub struct ExampleContainer {
///     pub value: i64
/// }
/// 
/// #[juiz_container(
///     description = "This is description for container."
///     arguments = {
///         default = {
///             initial_value = 0
///         }
///         description = {
///             initial_value = "Default value of container included value."
///         }
///     }
/// )]
/// fn example_container(initial_value: i64) -> JuizResult<Box<ExampleContainer>> {
///     Ok(Box::new(ExampleContainer{value:initial_value}))
/// }
/// ```
#[proc_macro_attribute]
pub fn juiz_container(attr: TokenStream, item: TokenStream) -> TokenStream {
    container::juiz_container_inner(attr, item)
}

/// コンテナプロセス定義のためのマクロ
///
/// # Examples
///
/// ```ignore
/// use example_container::ExampleContainer;
/// use juiz_sdk::prelude::*;
/// 
/// #[juiz_container_process(
///     container_type = "example_container"
///     description = "Container Process for example_container. This process will add given value to container."
///     arguments = {
///         default = {
///             arg1 = 1
///         }
///     }
/// )]
/// fn increment_function(container: &mut ContainerImpl<ExampleContainer>, arg1: i64) -> JuizResult<Capsule> {
///     container.value = container.value + arg1;
///     return Ok(jvalue!(container.value).into());
/// }
/// 
/// ```
#[proc_macro_attribute]
pub fn juiz_container_process(attr: TokenStream, item: TokenStream) -> TokenStream {
    container_process::juiz_container_process_inner(attr, item)
}

/// コンポーネントのマニフェスト定義のためのマクロ
/// コンポーネントのプロジェクトのlib.rsにこの定義が必要です。
/// コンポーネントに含まれる実体メソッドを以下の例のように列挙します。
///
/// # Examples
///
/// ```ignore
/// juiz_component_manifest!(
///     component_name = "example_component"
///     containers = {
///         example_component_container = [
///             example_component_container_get,
///             example_component_container_increment,
///             example_component_container_add
///         ]
///     }
///     processes = [
///         example_component_increment
///     ]
/// );
/// ```
#[proc_macro]
pub fn juiz_component_manifest(attr: TokenStream) -> TokenStream {
    component::juiz_component_manifest_inner(attr)
}

/// コンポーネント内でのプロセス定義のためのマクロ
///
/// `juiz_process`と同じ使い方ができます。例は引数を省略しています。
/// # Examples
///
/// ```ignore
/// #[juiz_component_process]
/// fn example_component_increment(arg1: i64) -> JuizResult<Capsule> {
///     log::trace!("increment_process({:?}) called", arg1);
///     return Ok(jvalue!(arg1+1).into());
/// }
/// ```
#[proc_macro_attribute]
pub fn juiz_component_process(attr: TokenStream, item: TokenStream) -> TokenStream {
    process::juiz_component_process_inner(attr, item)
}

/// コンポーネント内でのコンテナ定義のためのマクロ
///
/// `juiz_container`と同様の使い方ができます。例は引数を全て省略した使い方。
/// # Examples
///
/// ```ignore
/// #[repr(Rust)]
/// pub struct ExampleComponentContainer {
///     pub value: i64
/// }
/// 
/// #[juiz_component_container]
/// fn example_component_container(initial_value: i64) -> JuizResult<Box<ExampleComponentContainer>> {
///     println!("example_component_container({initial_value}) called");
///     Ok(Box::new(ExampleComponentContainer{value: initial_value}))
/// }
/// ```
#[proc_macro_attribute]
pub fn juiz_component_container(attr: TokenStream, item: TokenStream) -> TokenStream {
    container::juiz_component_container_inner(attr, item)
}

/// コンポーネント内でのコンテナプロセス定義のためのマクロ
///
/// # Examples
///
/// ```ignore
/// #[juiz_component_container_process( container_type = "example_component_container" 
///    arguments = {
///     default = {
///       arg1 = 1
///     }
///  }
/// )]
/// fn example_component_container_add(container: &mut ContainerImpl<ExampleComponentContainer>, arg1: i64) -> JuizResult<Capsule> {
///   println!("example_component_container_add({arg1})");
///   container.value = container.value + arg1;
///   Ok(jvalue!(container.value).into())
/// }
/// ```
#[proc_macro_attribute]
pub fn juiz_component_container_process(attr: TokenStream, item: TokenStream) -> TokenStream {
    container_process::juiz_component_container_process_inner(attr, item)
}