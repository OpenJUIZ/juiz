

use std::fmt::Display;

use anyhow::anyhow;
use serde::{Serialize, Deserialize};

use crate::{prelude::*};
use super::{manifest_description::Description, ArgumentProfile, ProcessProfile};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerProfile {
    pub name: String,
    pub language: String,
    pub type_name: String,
    pub factory: String, 
    pub arguments: Vec<ArgumentProfile>,
    pub description: Description,
    pub parent_type_name: Option<String>,
    pub parent_name: Option<String>,
    pub processes: Vec<ProcessProfile>,
    pub args: Value,
}

impl Display for ContainerProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("ContainerProfile(\"{}\", \"{}\", processes=[", self.type_name, self.language, ))?;
        for p in self.processes.iter() {
            f.write_fmt(format_args!("{}, ", p))?;
        }
        f.write_str("])")?;

        Ok(())
    }
}

impl TryFrom<ContainerManifest> for ContainerProfile {
    
    fn try_from(value: ContainerManifest) -> Result<Self, Self::Error> {
        Ok(Self {
            name: value.name.ok_or(anyhow!(JuizError::ContainerManifestInvalidError{message: format!("ContainerManifest does not include 'name' value.") }))?,
            language: value.language,
            type_name: value.type_name,
            factory: value.factory,
            arguments: value.arguments.into_iter().map(|a| { a.into() }).collect(),
            description: value.description,
            parent_type_name: value.parent_type_name,
            parent_name: value.parent_name,
            processes: value.processes.into_iter().map(|p| -> JuizResult<ProcessProfile> { p.try_into() }).collect::<JuizResult<Vec<ProcessProfile>>>()?,
            args: value.args,
        })
    }

    type Error = anyhow::Error;
}

impl ContainerProfile {

    pub fn build_instance_manifest(&self, mut partial_instance_manifest: ContainerProfile) -> JuizResult<Self> {
        partial_instance_manifest.type_name = self.type_name.clone();
        Ok(partial_instance_manifest
            .description(self.description.as_str())
        )
    }

    pub fn new(type_name: &str, name: &str) -> Self {
        Self {
            name: name.to_owned(),
            language: "rust".to_owned(),
            type_name: type_name.to_owned(),
            factory: "container_factory".to_owned(),
            description: "".into(), 
            parent_type_name: None,
            parent_name: None,
            processes: Vec::new(),
            args: jvalue!({}),
            arguments: Vec::new(),
        }
    }

    pub fn parent_container_profile(&self) -> Option<Self> {
        self.parent_name.as_ref().and_then(|pn| {
            Some(Self {
                name: pn.clone(),
                language: "rust".to_owned(),
                type_name: self.parent_type_name.as_ref().unwrap().clone(),
                factory: "component_factory".to_owned(),
                description: "".into(),
                parent_type_name: None,
                parent_name: None,
                processes: Vec::new(),
                args: jvalue!({}),
                arguments: Vec::new(),
            })
        })
    }

    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_owned();
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

    pub fn add_process(mut self, process_profile: ProcessProfile) -> Self {
        // println!("add_process({})", process_manifest);
        self.processes.push( 
            process_profile
                .container_name(Some(self.name.clone()))
                .container_type(Some(self.type_name.clone()))
        );
        // println!("cm: {}", self);
        self
    }

    pub fn factory(mut self, factory: &str) -> Self {
        self.factory = factory.to_owned();
        self
    }
    
    pub fn add_arg(mut self, arg: ArgumentProfile) -> Self {
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
        self.add_arg(ArgumentProfile::new_int(name, default).description(description))
    }

        /// ```
    /// use juiz_sdk::prelude::*;
    /// let manifest = ProcessProfile::new("hoge01", "hoge_type")
    ///   .description("hoge manifest")
    ///   .add_float_arg("arg1", "float_arg", 1.0.into());
    /// assert_eq!(manifest.arguments[0].name, "arg1");
    /// assert_eq!(manifest.arguments[0].type_name.as_str(), "Float");
    /// ```
    pub fn add_float_arg(self, name: &str, description: &str, default: f64) -> Self {
        self.add_arg(ArgumentProfile::new_float(name, default).description(description))
    }

    pub fn add_object_arg(self, name: &str, description: &str, default: Value) -> Self {
        self.add_arg(ArgumentProfile::new_object(name, default).description(description))
    }

    pub fn add_string_arg(self, name: &str, description: &str, default: &str) -> Self {
        self.add_arg(ArgumentProfile::new_string(name, default).description(description))
    }

    pub fn add_image_arg(self, name: &str, description: &str) -> Self {
        self.add_arg(ArgumentProfile::new_image(name).description(description))
    }

    pub fn identifier(&self) -> ContainerIdentifier {
        ContainerIdentifier::new( format!("core"), format!("core"), self.type_name.clone(), self.name.clone())
    }

}
