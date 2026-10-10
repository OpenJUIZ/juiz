use std::fmt::{Debug, Display};

use mopa::mopafy;
use crate::{manifests::ContainerProfile, prelude::*, processes::ProcessPtr};

pub trait Container : Display + Debug + mopa::Any {

    fn identifier(&self) -> ContainerIdentifier;

    fn profile(&self) -> JuizResult<ContainerProfile>;
    
    // fn manifest(&self) -> &ContainerManifest;

    fn process(&self, name_or_id: &String) -> Option<ProcessPtr>;

    fn processes(&self) -> Vec<ProcessPtr>;

    fn register_process(&mut self, p: ProcessPtr) -> JuizResult<ProcessPtr>;

    fn purge_process(&mut self, name_or_id: &String) -> JuizResult<()>;

    fn clear(&mut self) -> JuizResult<()>;

}

mopafy!(Container);

