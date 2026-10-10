
use std::fmt::Display;

use serde::{Deserialize, Serialize};


use super::{ContainerManifest, Description, ProcessManifest};


#[derive(Debug, Serialize, Deserialize)]
pub struct ComponentManifest {
    pub type_name: String,
    pub description: Description,
    pub language: String,
    pub containers: Vec<ContainerManifest>,
    pub processes: Vec<ProcessManifest>,
}


impl Display for ComponentManifest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {

        f.write_fmt(format_args!("ComponentManifest(\"{}\", \"{}\", {}, containers=[", self.type_name, self.language, self.description))?;
        for c in self.containers.iter() {
            f.write_fmt(format_args!("{},", c))?;
        }
        f.write_str("], processes=[")?;
        for p in self.processes.iter() {
            f.write_fmt(format_args!("{}, ", p))?;
        }

        f.write_str("])")?;
        Ok(())
    }
}

impl ComponentManifest {



    pub fn new(type_name: &str) -> Self {
        Self {
            type_name: type_name.to_owned(),
            description: Description::new(""),
            language: "rust".to_owned(),
            containers: Vec::new(),
            processes: Vec::new(),
        }
    }

    pub fn language(mut self, language: &str) -> Self {
        self.language = language.to_owned();
        self
    }

    pub fn description(mut self, description: &str) -> Self {
        self.description = description.into();
        self
    }


    pub fn add_container(mut self, container: ContainerManifest) -> Self {
        // println!("add_container({})", container);
        self.containers.push(container);
        self
    }

    pub fn add_process(mut self, process: ProcessManifest) -> Self {
        if let Some(container_type) = process.container_type.as_ref() {
            let m = self.containers.into_iter().map(|c| {
                if c.type_name == container_type.as_str() {
                    c.add_process(process.clone())
                } else {
                    c
                }
            }).collect::<Vec<ContainerManifest>>();
            self.containers = m;
            self
        } else {
            self.processes.push(process);
            self
        }
        
    }
}

// impl TryFrom<Value> for ComponentManifest {
//     type Error = anyhow::Error;

//     fn try_from(value: Value) -> Result<Self, Self::Error> {
//         let desc = match obj_get_str(&value, "description") {
//             Ok(v) => v,
//             Err(_) => ""
//         };
//         let mut p = ComponentManifest::new(obj_get_str(&value, "type_name")?).description(desc);
//         match obj_get_str(&value, "language") {
//             Ok(lang) => {
//                 p = p.language(lang);
//             },
//             Err(_) => {}
//         }
//         match obj_get_array(&value, "containers") {
//             Ok(value_array) => {
//                 for v in value_array.iter() {
//                     p = p.add_container(v.clone().try_into()?);
//                 }
//             }
//             Err(_) => {}
//         }
//         match obj_get_array(&value, "processes") {
//             Ok(value_array) => {
//                 for v in value_array.iter() {
//                     p = p.add_process(v.clone().try_into()?);
//                 }
//             }
//             Err(_) => {}
//         }
//         Ok(p)
//     }
// }

// impl Into<Value> for ComponentManifest {
//     fn into(self) -> Value {
//         let v = jvalue!({
//             "type_name": self.type_name,
//             "language": self.language,
//             "description": self.description.to_str(),
//             "processes": self.processes.iter().map(|c| { c.clone().into() }).collect::<Vec<Value>>(),
//             "containers": self.containers.iter().map(|c| { c.clone().into() }).collect::<Vec<Value>>(),
//         });
//         v
//     }
// }