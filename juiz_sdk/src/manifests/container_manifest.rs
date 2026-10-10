

use std::fmt::Display;

use serde::{Deserialize, Serialize};

use crate::{prelude::*};
use super::manifest_description::Description;

#[allow(unused)]
fn default_description() -> Description { Description::new("") }
fn default_broker_type_name() -> String { "core".to_owned() }
fn default_broker_name() -> String { "core".to_owned() }
fn default_factory() -> String { "container_factory".to_owned() }

#[allow(unused)]
fn default_use_memo() -> bool { false }
fn default_language() -> String { "rust".to_owned() }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerManifest {
    pub name: Option<String>,
    pub type_name: String,
    #[serde(default="default_language")]
    pub language: String,
    #[serde(default="default_factory")]
    pub factory: String, 
    #[serde(default)]
    pub arguments: Vec<ArgumentManifest>,
    #[serde(default)]
    pub description: Description,
    pub parent_type_name: Option<String>,
    pub parent_name: Option<String>,
    #[serde(default)]
    pub processes: Vec<ProcessManifest>,

    #[serde(default)]
    pub args: Value,
    #[serde(default="default_broker_type_name")]
    pub broker_type_name: String,
    #[serde(default="default_broker_name")]
    pub broker_name: String,
}

impl Display for ContainerManifest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("ContainerManifest(\"{}\", \"{}\", processes=[", self.type_name, self.language, ))?;
        for p in self.processes.iter() {
            f.write_fmt(format_args!("{}, ", p))?;
        }
        f.write_str("])")?;

        Ok(())
    }
}


impl ContainerManifest {

    pub fn build_instance_manifest(&self, mut partial_instance_manifest: ContainerManifest) -> JuizResult<Self> {
        partial_instance_manifest.type_name = self.type_name.clone();
        Ok(partial_instance_manifest
            .description(self.description.as_str())
        )
    }

    pub fn new(type_name: &str) -> Self {
        Self {
            name: None,
            language: "rust".to_owned(),
            type_name: type_name.to_owned(),
            factory: "container_factory".to_owned(),
            description: "".into(), 
            parent_type_name: None,
            parent_name: None,
            processes: Vec::new(),
            args: jvalue!({}),
            arguments: Vec::new(),
            broker_name: "core".to_owned(),
            broker_type_name: "core".to_owned(),
        }
    }

    pub fn parent_container_manifest(&self) -> Self {
        Self {
            name: self.parent_name.clone(),
            language: "rust".to_owned(),
            type_name: self.parent_type_name.as_ref().unwrap().clone(),
            factory: "component_factory".to_owned(),
            description: "".into(),
            parent_type_name: None,
            parent_name: None,
            processes: Vec::new(),
            args: jvalue!({}),
            arguments: Vec::new(),
            broker_name: self.broker_name.clone(),
            broker_type_name: self.broker_type_name.clone(),
        }
    }

    pub fn name(mut self, name: &str) -> Self {
        self.name = Some(name.to_owned());
        self
    }

    pub fn language(mut self, language: &str) -> Self {
        self.language = language.to_owned();
        self
    }

    pub fn description(mut self, description: &str) -> Self {
        self.description = description.into();
        self
    }

    pub fn parent_type_name(mut self, parent_type_name: &str) -> Self {
        self.parent_type_name = Some(parent_type_name.to_owned());
        self
    }

    pub fn parent_name(mut self, parent_name: &str) -> Self {
        self.parent_name = Some(parent_name.to_owned());
        self
    }

    pub fn add_process(mut self, process_manifest: ProcessManifest) -> Self {
        // println!("add_process({})", process_manifest);
        self.processes.push( 
            process_manifest
                .container_name(self.name.as_ref().map(|s|{s.clone()}))
                .container_type(Some(self.type_name.clone()))
        );
        // println!("cm: {}", self);
        self
    }

    pub fn factory(mut self, factory: &str) -> Self {
        self.factory = factory.to_owned();
        self
    }
    
    pub fn add_arg(mut self, arg: ArgumentManifest) -> Self {
        self.arguments.push(arg);
        self
    }

    /// ```
    /// use juiz_sdk::prelude::*;
    /// let manifest = ProcessManifest::new("hoge_type")
    ///   .description("hoge manifest")
    ///   .add_int_arg("arg0", "int_arg", 1.into());
    /// assert_eq!(manifest.arguments[0].name, "arg0");
    /// assert_eq!(manifest.arguments[0].type_name.as_str(), "Int");
    /// ```
    pub fn add_int_arg(self, name: &str, description: &str, default: i64) -> Self {
        self.add_arg(ArgumentManifest::new_int(name, default).description(description))
    }

        /// ```
    /// use juiz_sdk::prelude::*;
    /// let manifest = ProcessManifest::new("hoge_type")
    ///   .description("hoge manifest")
    ///   .add_float_arg("arg1", "float_arg", 1.0.into());
    /// assert_eq!(manifest.arguments[0].name, "arg1");
    /// assert_eq!(manifest.arguments[0].type_name.as_str(), "Float");
    /// ```
    pub fn add_float_arg(self, name: &str, description: &str, default: f64) -> Self {
        self.add_arg(ArgumentManifest::new_float(name, default).description(description))
    }

    pub fn add_bool_arg(self, name: &str, description: &str, default: bool) -> ContainerManifest {
        self.add_arg(ArgumentManifest::new_bool(name, default).description(description))
    }

    pub fn add_object_arg(self, name: &str, description: &str, default: Value) -> Self {
        self.add_arg(ArgumentManifest::new_object(name, default).description(description))
    }

    pub fn add_string_arg(self, name: &str, description: &str, default: &str) -> Self {
        self.add_arg(ArgumentManifest::new_string(name, default).description(description))
    }

    pub fn add_image_arg(self, name: &str, description: &str) -> Self {
        self.add_arg(ArgumentManifest::new_image(name).description(description))
    }

    pub fn identifier(&self) -> JuizResult<ContainerIdentifier> { 
        Ok(ContainerIdentifier::new(self.broker_name.clone(),
            self.broker_type_name.clone(), 
            self.type_name.clone(), 
            self.name.as_ref().unwrap().clone()))
    }

    pub fn add_array_arg(self, name: &str, description: &str, default: Vec<serde_json::Value>) -> ContainerManifest {
        self.add_arg(ArgumentManifest::new_array(name, default).description(description))
    }


}

// impl TryFrom<Value> for ContainerManifest {
//     type Error = anyhow::Error;

//     fn try_from(value: Value) -> Result<Self, Self::Error> {
//         let desc = match obj_get_str(&value, "description") {
//             Ok(v) => v,
//             Err(_) => ""
//         };
//         let mut p = ContainerManifest::new(obj_get_str(&value, "type_name")?)
//             .description(desc);
//         match obj_get_str(&value, "name") {
//             Ok(name) => {
//                 p = p.name(name);
//             },
//             Err(_) => {}
//         }
//         match obj_get_str(&value, "language") {
//             Ok(lang) => {
//                 p = p.language(lang);
//             },
//             Err(_) => {}
//         }
//         match obj_get_str(&value, "factory") {
//             Ok(fact) => {
//                 p = p.factory(fact);
//             },
//             Err(_) => {}
//         }
//         match obj_get_str(&value, "parent_name") {
//             Ok(name) => {
//                 p = p.parent_name(name);
//             },
//             Err(_) => {}
//         }
//         match obj_get_str(&value, "parent_type_name") {
//             Ok(name) => {
//                 p = p.parent_type_name(name);
//             },
//             Err(_) => {}
//         }
//         match obj_get_array(&value, "processes") {
//             Ok(process_manifest_values) => {
//                 for process_manifest_value in process_manifest_values.iter() {
//                     let pp :ProcessManifest = process_manifest_value.clone().try_into().context("in loading ContainerManifest from Value")?;
//                     p = p.add_process(pp);
//                 }
//             },
//             Err(_) => {}
//         }
//         match obj_get_array(&value, "arguments") {
//             Ok(arg_manifest_values) => {
//                 // println!("try_into: {arg_manifest_values:?}");
//                 for arg_manifest_value in arg_manifest_values.iter() {
//                     p = p.add_arg(arg_manifest_value.clone().try_into()?);
//                 }
//             }
//             Err(_) => {},
//         }

//         Ok(p)
//     }
// }


// fn arguments_to_array(args: Vec<ArgumentManifest>) -> Value {
//     args.into_iter().map(|arg| -> Value {
//         arg.into()
//     }).collect()
// }

// impl Into<Value> for ContainerManifest {
//     fn into(self) -> Value {
//         let mut v = jvalue!({
//             "type_name": self.type_name,
//             "language": self.language,
//             "description": self.description.to_str(),
//             "arguments": arguments_to_array(self.arguments),
//             "processes": self.processes.iter().map(|p|{ p.clone().into() }).collect::<Vec<Value>>()
//         });
//         let obj = v.as_object_mut().unwrap();
//         if let Some(name) = self.name {
//             obj.insert("name".to_owned(), name.into());
//         }
//         if let Some(parent_type_name) = self.parent_type_name {
//             obj.insert("parent_type_name".to_owned(), parent_type_name.into());
//         }
//         if let Some(parent_name) = self.parent_name {
//             obj.insert("parent_name".to_owned(), parent_name.into());
//         }


//         v   
//     }
// }
