// use std::collections::HashMap;
// use anyhow::anyhow;
// use serde_json::Map;
// use crate::{identifier::identifier_new, prelude::*, value::obj_get_str};
// use super::{argument_manifest::{ArgumentManifest, ArgumentType}, manifest_description::Description, TopicManifest};

// #[derive(Clone, Debug)]
// pub struct ContainerProcessManifest {
//     pub name: Option<String>,
//     pub container_manifest: ContainerManifest,
//     pub type_name: String,
//     pub description: Description,
//     pub arguments: Vec<ArgumentManifest>,
//     pub use_memo: bool,
//     pub broker_type_name: String,
//     pub broker_name: String,
//     pub publishes: Vec<TopicManifest>,
//     pub subscribes: HashMap<String, TopicManifest>,
// }

// impl ContainerProcessManifest {

//     /// ```
//     /// use juiz_core::prelude::*;
//     /// let manifest = ContainerProcessManifest::new("hoge_type")
//     ///   .description("hoge manifest")
//     ///   .add_int_arg("arg0", "int_arg", 1.into())
//     ///   .add_float_arg("arg1", "float_arg", 1.0.into())
//     ///   .add_string_arg("arg2", "string_arg", "default_string".into());
//     /// assert_eq!(manifest.arguments[0].name, "arg0");
//     /// assert_eq!(manifest.arguments[0].type_name.as_str(), "int");
//     /// assert_eq!(manifest.arguments[1].name, "arg1");
//     /// assert_eq!(manifest.arguments[1].type_name.as_str(), "float");
//     /// assert_eq!(manifest.arguments[2].name, "arg2");
//     /// assert_eq!(manifest.arguments[2].type_name.as_str(), "string");
//     /// ```
//     pub fn new(container_manifest: ContainerManifest, type_name: &str) -> Self {
//         Self {
//             name: None,
//             container_manifest,
//             type_name: type_name.to_owned(),
//             description: "".into(),
//             arguments: Vec::new(),
//             use_memo: false,
//             broker_name: "core".to_owned(),
//             broker_type_name: "core".to_owned(),
//             publishes: Vec::new(),
//             subscribes: HashMap::new(),
//         }
//     }

//     pub fn name(mut self, name: &str) -> Self {
//         self.name = Some(name.to_owned());
//         self
//     }

//     pub fn use_memo(mut self, use_memo: bool) -> Self {
//         self.use_memo = use_memo;
//         self
//     }

//     pub fn description(mut self, description: &str) -> Self {
//         self.description = description.into();
//         self
//     }

//     pub fn add_arg(mut self, arg: ArgumentManifest) -> Self {
//         self.arguments.push(arg);
//         self
//     }

//     /// ```
//     /// use juiz_core::prelude::*;
//     /// let manifest = ProcessManifest::new("hoge_type")
//     ///   .description("hoge manifest")
//     ///   .publishes("topic1")
//     /// assert_eq!(manifest.publishes[0].name, "topic1");
//     /// ```
//     pub fn publishes(mut self, topic_name: &str) -> Self {
//         self.publishes.push(TopicManifest::new(topic_name));
//         self
//     }

//         /// ```
//     /// use juiz_core::prelude::*;
//     /// let manifest = ProcessManifest::new("hoge_type")
//     ///   .description("hoge manifest")
//     ///   .add_int_arg("arg0", "int_arg", 1.into());
//     ///   .subscribes("arg0", "topic1")
//     /// assert_eq!(manifest.subscribes.get("arg0").unwrap().name, "topic1");
//     /// ```
//     pub fn subscribes(mut self, arg_name:&str, topic_name: &str) -> Self {
//         self.subscribes.insert(arg_name.to_owned(), TopicManifest::new(topic_name));
//         self
//     }

//     /// ```
//     /// use juiz_core::prelude::*;
//     /// let manifest = ProcessManifest::new("hoge_type")
//     ///   .description("hoge manifest")
//     ///   .add_int_arg("arg0", "int_arg", 1.into());
//     /// assert_eq!(manifest.arguments[0].name, "arg0");
//     /// assert_eq!(manifest.arguments[0].type_name.as_str(), "int");
//     /// ```
//     pub fn add_int_arg(self, name: &str, description: &str, default: i64) -> Self {
//         self.add_arg(ArgumentManifest::new_int(name, default).description(description))
//     }

//         /// ```
//     /// use juiz_core::prelude::*;
//     /// let manifest = ProcessManifest::new("hoge_type")
//     ///   .description("hoge manifest")
//     ///   .add_float_arg("arg1", "float_arg", 1.0.into());
//     /// assert_eq!(manifest.arguments[0].name, "arg1");
//     /// assert_eq!(manifest.arguments[0].type_name.as_str(), "float");
//     /// ```
//     pub fn add_float_arg(self, name: &str, description: &str, default: f64) -> Self {
//         self.add_arg(ArgumentManifest::new_float(name, default).description(description))
//     }

//     pub fn add_object_arg(self, name: &str, description: &str, default: Value) -> Self {
//         self.add_arg(ArgumentManifest::new_object(name, default).description(description))
//     }

//     pub fn add_image_arg(self, name: &str, description: &str) -> Self {
//         self.add_arg(ArgumentManifest::new_image(name).description(description))
//     }

//         /// ```
//     /// use juiz_core::prelude::*;
//     /// let manifest = ProcessManifest::new("hoge_type")
//     ///   .description("hoge manifest")
//     ///   .add_string_arg("arg2", "string_arg", "default_string".into());
//     /// assert_eq!(manifest.arguments[0].name, "arg2");
//     /// assert_eq!(manifest.arguments[0].type_name.as_str(), "string");
//     /// ```
//     pub fn add_string_arg(self, name: &str, description: &str, default: &str) -> Self {
//         self.add_arg(ArgumentManifest::new_string(name, default).description(description))
//     }

//     pub fn identifier(&self) -> JuizResult<Identifier> {
//         if let Some(name) = self.name.as_ref() {
//             Ok(identifier_new(
//                 self.broker_type_name.as_str(),
//                 self.broker_name.as_str(),
//                 "ContainerProcess",
//                 self.type_name.as_str(),
//                 name.as_str()))
//         } else {
//             Err(anyhow!(JuizError::ProcessManifestInvalidError{message:"ContainerProcessManifest::identifier() failed.".to_owned()}))
//         }

//     }
// }

// #[allow(dead_code)]
// fn arguments_to_object(args: Vec<ArgumentManifest>) -> Value {
//     let mut v : serde_json::Map<String, Value> = serde_json::Map::new();
//     for a in args.into_iter() {
//         v.insert(a.name.clone(), a.into());
//     }
//     v.into()
// }

// fn arguments_to_array(args: Vec<ArgumentManifest>) -> Value {
//     args.into_iter().map(|arg| -> Value {
//         arg.into()
//     }).collect()
// }

// /// ```
// /// use juiz_core::prelude::*;
// /// fn main() -> JuizResult<()> {
// ///     let manifest = ProcessManifest::new("hoge_type")
// ///         .description("hoge manifest")
// ///         .add_int_arg("arg0", "int_arg", 1.into());
// ///     let value: Value = manifest.into();
// ///     assert_eq!(obj_get_str(&value, "type_name")?, "hoge_type");
// ///     assert_eq!(obj_get_str(&value, "description")?, "hoge manifest");
// ///     let arr: &Vec<Value> = obj_get_array(&value, "arguments")?;
// ///     assert_eq!(obj_get_str(&arr[0] , "type")?, "int");
// ///     assert_eq!(obj_get_str(&arr[0] , "name")?, "arg0");
// ///     assert_eq!(obj_get_str(&arr[0] , "description")?, "int_arg");
// ///     Ok(())
// /// }
// /// ```
// impl Into<Value> for ContainerProcessManifest {
//     fn into(self) -> Value {
//         jvalue!({
//             "type_name": self.type_name,
//             "container": Into::<Value>::into(self.container_manifest),
//             "description": self.description.to_str(),
//             "arguments": arguments_to_array(self.arguments),
//             "publishes": self.publishes.into_iter().map(|v|{ v.into() }).collect::<Vec<Value>>(),
//             "subscribes": self.subscribes.into_iter().map(|(k,v)|{ (k, v.into() ) }).collect::<Map<String, Value>>(),
//         })
//     }
// }

// /// ```
// /// use juiz_core::prelude::*;
// /// fn main() -> JuizResult<()> {
// ///
// /// let manifest_value: Value = jvalue!({
// ///   "type_name": "hoge_type",
// ///   "description": "hoge manifest",
// ///   "arguments": [
// ///     {
// ///        "name": "arg0",
// ///        "type": "int",
// ///        "default": 1,
// ///        "description": "int_arg"
// ///     },
// ///   ]
// /// });
// /// let manifest: ProcessManifest = manifest_value.try_into()?;
// ///
// /// Ok(())}
// /// ```
// impl TryFrom<Value> for ContainerProcessManifest {
//     fn try_from(value: Value) -> Result<Self, Self::Error> {
//         let container_manifest: ContainerManifest = obj_get(&value, "container")?.clone().try_into()?;
//         let mut p = ContainerProcessManifest::new(container_manifest, obj_get_str(&value, "type_name")?)
//             .description(obj_get_str(&value, "description").or::<Self::Error>(Ok("")).unwrap());
//         match obj_get(&value, "arguments") {
//             Ok(obj) => {
//                 match obj {
//                     serde_json::Value::Array(vec) => {
//                         for arg_obj in vec.into_iter() {
//                             p = p.add_arg(arg_obj.clone().try_into()?);
//                         }
//                     },
//                     _ => {

//                     }
//                 }
//             }
//             Err(_) => {},
//         }
//         match obj_get_array(&value, "publishes") {
//             Ok(value_array) => {
//                 for arg_obj in value_array.into_iter() {
//                     p = p.publishes(arg_obj.as_str().unwrap());
//                 }
//             }
//             Err(_) => {},
//         }
//         match obj_get_obj(&value, "subscribes") {
//             Ok(value_map) => {
//                 for (arg_name, arg_obj) in value_map.into_iter() {
//                     p = p.subscribes(arg_name.as_str(), arg_obj.as_str().unwrap());
//                 }
//             }
//             Err(_) => {},
//         }
//         Ok(p)
//     }
//     type Error = anyhow::Error;
// }
