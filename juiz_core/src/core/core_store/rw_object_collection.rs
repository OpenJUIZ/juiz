use std::{
    collections::{hash_map::Values, HashMap},
    sync::{Arc, Mutex, RwLock},
};

use crate::{
    prelude::*,
    utils::manifest_util::{get_array_mut, get_hashmap_mut},
};

pub struct RwObjectCollection<T, TF>
where
    T: JuizObject + ?Sized,
    TF: JuizObject + ?Sized,
{
    name: String,
    factories: HashMap<String, Arc<Mutex<TF>>>,
    objects: HashMap<Identifier, Arc<RwLock<T>>>,
}

impl<T, TF> RwObjectCollection<T, TF>
where
    T: JuizObject + ?Sized,
    TF: JuizObject + ?Sized,
{
    pub fn new(name: &str) -> Box<RwObjectCollection<T, TF>> {
        Box::new(RwObjectCollection {
            name: name.to_string(),
            factories: HashMap::new(),
            objects: HashMap::new(),
        })
    }

    pub fn clear(&mut self) -> JuizResult<()> {
        self.objects.clear();
        self.factories.clear();
        Ok(())
    }

    pub fn objects(&self) -> Values<String, Arc<RwLock<T>>> {
        self.objects.values()
    }

    pub fn register_factory(&mut self, pf: Arc<Mutex<TF>>) -> JuizResult<Arc<Mutex<TF>>> {
        let type_name = juiz_lock(&pf)?.type_name().to_string();
        log::trace!(
            "RWStoreWorker({})::registerfactory(Factory(type_name={:?})) called",
            self.name,
            type_name
        );
        if self.factories.contains_key(&type_name) {
            log::error!("RWStoreWorker({})::registerfactory(Factory(type_name={:?})) failed. Already Exists.", self.name, type_name );
            return Err(anyhow::Error::from(
                JuizError::FactoryOfSameTypeNameAlreadyExistsError {
                    type_name: type_name,
                },
            ));
        }
        self.factories.insert(type_name, Arc::clone(&pf));
        Ok(pf)
    }

    pub fn factory(&self, type_name: &str) -> JuizResult<&Arc<Mutex<TF>>> {
        match self.factories.get(type_name) {
            None => {
                return Err(anyhow::Error::from(JuizError::FactoryCanNotFoundError {
                    type_name: type_name.to_string(),
                }))
            }
            Some(pf) => return Ok(pf),
        }
    }

    pub fn register(&mut self, p: Arc<RwLock<T>>) -> JuizResult<Arc<RwLock<T>>> {
        // let id = proc_lock(&p)?.identifier().clone();
        let id = p.read().unwrap().identifier().clone();
        log::trace!(
            "RWStoreWorker({})::register(Object(id={:?})) called",
            self.name,
            id
        );
        self.objects.insert(id.clone(), p);
        self.get(&id)
    }

    pub fn deregister(&mut self, p: Arc<RwLock<T>>) -> JuizResult<Arc<RwLock<T>>> {
        let id = p.read().unwrap().identifier().clone();
        self.deregister_by_id(&id)
    }

    pub fn deregister_by_id(&mut self, id: &Identifier) -> JuizResult<Arc<RwLock<T>>> {
        log::trace!(
            "RWStoreWorker({})::deregister(Object(id={:?})) called",
            self.name,
            id
        );
        match self.objects.remove(id) {
            Some(p) => Ok(p),
            None => {
                log::trace!(
                    "RwStoreWorker({})::deregister(id={:}) failed. Not found.",
                    self.name,
                    id
                );
                Err(anyhow::Error::from(JuizError::ObjectCanNotFoundByIdError {
                    id: id.clone(),
                }))
            }
        }
    }

    pub fn get(&self, id: &Identifier) -> JuizResult<Arc<RwLock<T>>> {
        match self.objects.get(id) {
            Some(p) => Ok(Arc::clone(p)),
            None => {
                //log::error!("RWStoreWorker({})::get(id={:?}) failed.", self.name, id);
                //log::trace!(" Error Additional Info - CoreStore includes processes[");
                //for (k, _v) in self.objects.iter() {
                //    log::trace!("    - {:?}", k);
                //}
                //log::trace!("]");
                Err(anyhow::Error::from(JuizError::ObjectCanNotFoundByIdError {
                    id: id.clone(),
                }))
            }
        }
    }

    pub fn factories_profile_full(&self) -> JuizResult<Value> {
        let mut prof = jvalue!({});
        let o_hashmap = get_hashmap_mut(&mut prof)?;
        self.factories
            .iter()
            .for_each(|(identifier, arc_obj)| match juiz_lock(&arc_obj) {
                Err(e) => {
                    o_hashmap.insert(identifier.clone(), jvalue!(format!("Err({})", e)));
                }
                Ok(p) => {
                    o_hashmap.insert(
                        identifier.clone(),
                        p.profile_full().unwrap().try_into().unwrap(),
                    );
                }
            });
        Ok(prof)
    }

    pub fn objects_profile_full(&self) -> JuizResult<Value> {
        let name = &self.name;
        log::trace!("RWStoreWorker({name})::objects_profile_full() called");
        let mut prof = jvalue!({});
        let o_hashmap = get_hashmap_mut(&mut prof)?;
        self.objects.iter().for_each(|(identifier, arc_obj)| {
            log::trace!(" - {identifier}");
            /*
            match juiz_try_lock(&arc_obj) {
                Err(e) => {
                    o_hashmap.insert(identifier.clone(), jvalue!(format!("Err({})", e)));
                },
                Ok(p) => {
                    o_hashmap.insert(identifier.clone(), p.profile_full().unwrap().try_into().unwrap());
                }
            }
            */
            o_hashmap.insert(
                identifier.clone(),
                arc_obj
                    .read()
                    .unwrap()
                    .profile_full()
                    .unwrap()
                    .try_into()
                    .unwrap(),
            );
        });
        Ok(prof)
    }

    pub fn list_ids(&self) -> JuizResult<Value> {
        let mut prof = jvalue!([]);
        let o_array = get_array_mut(&mut prof)?;
        self.objects.iter().for_each(|(identifier, _arc_obj)| {
            /*match juiz_lock(&arc_obj) {
                Err(e) => {
                    o_array.push(jvalue!(format!("Err({})", e)));
                },
                Ok(_p) => {
                    o_array.push(jvalue!(identifier));
                }
            }*/
            o_array.push(jvalue!(identifier));
        });
        Ok(prof)
    }

    pub fn list_manifests(&self) -> JuizResult<Vec<Value>> {
        let mut o_array: Vec<Value> = Vec::new();
        self.objects
            .iter()
            .for_each(|(_identifier, arc_obj)| match arc_obj.read() {
                Err(e) => {
                    o_array.push(jvalue!(format!("Err({})", e)));
                }
                Ok(p) => {
                    o_array.push(p.profile_full().unwrap().try_into().unwrap());
                }
            });
        Ok(o_array)
    }

    pub fn cleanup_objects(&mut self) -> JuizResult<()> {
        self.objects.clear();
        Ok(())
    }
}
