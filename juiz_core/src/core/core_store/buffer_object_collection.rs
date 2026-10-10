use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::{Arc, Mutex}};

use crate::prelude::*;
use juiz_sdk::anyhow;
pub struct BufferObjectCollection<T, TF> where T: JuizObject + ?Sized, TF: JuizObject + ?Sized {
    name: String,
    factories: HashMap<String, Arc<Mutex<TF>>>,
    objects: Rc<RefCell<HashMap<Identifier, Arc<Mutex<T>>>>>,
}

impl<T, TF> BufferObjectCollection<T, TF> where T: JuizObject + ?Sized, TF: JuizObject + ?Sized {

    pub fn new(name: &str) -> Box<BufferObjectCollection<T, TF>> {
        Box::new(Self { name: name.to_string(), 
            factories: HashMap::new(), 
            objects: Rc::new(RefCell::new(HashMap::new())) })
    }

    pub fn clear(&mut self) -> JuizResult<()> {
        self.objects.borrow_mut().clear();
        self.factories.clear();
        Ok(())
    }

    pub fn objects(&self) -> std::cell::Ref<HashMap<String, Arc<Mutex<T>>>> {
        self.objects.borrow()
    }

    pub fn register_factory(&mut self, pf: Arc<Mutex<TF>>) -> JuizResult<Arc<Mutex<TF>>> {
        let type_name = juiz_lock(&pf)?.type_name().to_string();
        log::trace!("StoreWorker({})::registerfactory(Factory(type_name={:?})) called",self.name,type_name);
        if self.factories.contains_key(&type_name) {
            return Err(anyhow::Error::from(JuizError::FactoryOfSameTypeNameAlreadyExistsError{type_name: type_name}));
        }
        self.factories.insert(type_name, Arc::clone(&pf));
        Ok(pf)
    }

    pub fn factory(&self, type_name: &str) -> JuizResult<&Arc<Mutex<TF>>> {
        match self.factories.get(type_name) {
            None => return Err(anyhow::Error::from(JuizError::FactoryCanNotFoundError{type_name: type_name.to_string()})),
            Some(pf) => return Ok(pf)
        }
    }

    pub fn register(&self, p: Arc<Mutex<T>>) -> JuizResult<Arc<Mutex<T>>> {
        let id = juiz_lock(&p)?.identifier().clone();
        log::trace!("StoreWorker({})::register(Object(id={:?})) called", self.name, id);
        self.objects.borrow_mut().insert(id.clone(), p);
        self.get(&id)
    }

    pub fn deregister(&mut self, p: Arc<Mutex<T>>) -> JuizResult<Arc<Mutex<T>>> {
        let id = p.lock().unwrap().identifier().clone();
        self.deregister_by_id(&id)
    }

    pub fn deregister_by_id(&mut self, id: &Identifier) -> JuizResult<Arc<Mutex<T>>> {
        log::trace!("StoreWorker({})::deregister(Object(id={:?})) called", self.name, id);
        match self.objects.borrow_mut().remove(id) {
            Some(p) => Ok(p),
            None =>{
                log::trace!("StoreWorker({})::deregister(id={:}) failed. Not found.", self.name, id);
                Err(anyhow::Error::from(JuizError::ObjectCanNotFoundByIdError { id: id.clone() }))
            }
        }
    }

    pub fn get_with_error_ignore(&self, id: &Identifier) -> JuizResult<Arc<Mutex<T>>> {
        match self.objects.borrow().get(id) {
            Some(p) => Ok(Arc::clone(p)),
            None => {
                log::trace!("StoreWorker({})::get_with_error_ignore(id={:?})が失敗しました。オブジェクトが見つかりません。", self.name, id);
                log::trace!(" - CoreStore includes objects[");
                for (k, _v) in self.objects.borrow().iter() {
                    log::trace!("    - {:?}", k);
                }
                log::trace!("]");
                Err(anyhow::Error::from(JuizError::ObjectCanNotFoundByIdError{id: id.clone()}))
            }
            
        }
    }

    pub fn get(&self, id: &Identifier) -> JuizResult<Arc<Mutex<T>>> {
        match self.objects.borrow().get(id) {
            Some(p) => Ok(Arc::clone(p)),
            None => {
                log::error!("StoreWorker({})::get(id={:?})が失敗しました。オブジェクトが見つかりません。", self.name, id);
                log::trace!(" - CoreStore includes objects[");
                for (k, _v) in self.objects.borrow().iter() {
                    log::trace!("    - {:?}", k);
                }
                log::trace!("]");
                Err(anyhow::Error::from(JuizError::ObjectCanNotFoundByIdError{id: id.clone()}))
            }
        }
    }

    pub fn factories_profile_full(&self) -> JuizResult<Value> {
        let mut prof = jvalue!({});
        let o_hashmap = get_hashmap_mut(&mut prof)?;
        self.factories.iter().for_each(|(identifier, arc_obj)| {
            match juiz_lock(&arc_obj) {
                Err(e) => {
                    o_hashmap.insert(identifier.clone(), jvalue!(format!("Err({})", e)));
                },
                Ok(p) => {
                    o_hashmap.insert(identifier.clone(), p.profile_full().unwrap().try_into().unwrap());
                }
            }
        });
        Ok(prof)
    }

    pub fn objects_profile_full(&self) -> JuizResult<Value> {
        let name = &self.name;
        log::trace!("StoreWorker({name})::objects_profile_full() called");
        let mut prof = jvalue!({});
        let o_hashmap = get_hashmap_mut(&mut prof)?;
        self.objects.borrow().iter().for_each(|(identifier, arc_obj)| {
            log::trace!(" - {identifier}");
            match juiz_try_lock(&arc_obj) {
                Err(e) => {
                    o_hashmap.insert(identifier.clone(), jvalue!(format!("Err({})", e)));
                },
                Ok(p) => {
                    o_hashmap.insert(identifier.clone(), p.profile_full().unwrap().try_into().unwrap());
                }
            }
        });
        Ok(prof)
    }

    pub fn list_ids(&self) -> JuizResult<Value> {
        let mut prof = jvalue!([]);
        let o_array = get_array_mut(&mut prof)?;
        self.objects.borrow().iter().for_each(|(identifier, _arc_obj)| {
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
        let mut o_array: Vec<Value>  = Vec::new();
        self.objects.borrow().iter().for_each(|(_identifier, arc_obj)| {
            match juiz_try_lock(&arc_obj) {
                Err(e) => {
                    o_array.push(jvalue!(format!("Err({})", e)));
                },
                Ok(p) => {
                    o_array.push(p.profile_full().unwrap().try_into().unwrap());
                }
            }
        });
        Ok(o_array)
    }

    pub fn cleanup_objects(&mut self) -> JuizResult<()> {
        self.objects.borrow_mut().clear();
        Ok(())
    }
}

