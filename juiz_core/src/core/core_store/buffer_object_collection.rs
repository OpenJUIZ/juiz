use std::{
    collections::HashMap,
    sync::{Arc, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use crate::prelude::*;
use juiz_sdk::anyhow;
pub struct BufferObjectCollection<T, TF>
where
    T: JuizObject + ?Sized,
    TF: JuizObject + ?Sized,
{
    name: String,
    factories: HashMap<String, Arc<Mutex<TF>>>,
    /// 登録したオブジェクト。CoreBroker は複数のスレッドから同時に読まれる (HTTP ブローカーの各ハンドラ、
    /// issue #356) ので、`&self` で足すこの表も RwLock で守る (以前の Rc<RefCell> は同時に触ると壊れた。issue #362)
    objects: RwLock<HashMap<Identifier, Arc<RwLock<T>>>>,
}

impl<T, TF> BufferObjectCollection<T, TF>
where
    T: JuizObject + ?Sized,
    TF: JuizObject + ?Sized,
{
    pub fn new(name: &str) -> Box<BufferObjectCollection<T, TF>> {
        Box::new(Self {
            name: name.to_string(),
            factories: HashMap::new(),
            objects: RwLock::new(HashMap::new()),
        })
    }

    pub fn clear(&mut self) -> JuizResult<()> {
        self.write_objects().clear();
        self.factories.clear();
        Ok(())
    }

    pub fn objects(&self) -> RwLockReadGuard<'_, HashMap<String, Arc<RwLock<T>>>> {
        self.read_objects()
    }

    fn read_objects(&self) -> RwLockReadGuard<'_, HashMap<String, Arc<RwLock<T>>>> {
        // 表を書き換える途中で panic しても表そのものは壊れないので、毒は無視する
        self.objects.read().unwrap_or_else(|e| e.into_inner())
    }

    fn write_objects(&self) -> RwLockWriteGuard<'_, HashMap<String, Arc<RwLock<T>>>> {
        self.objects.write().unwrap_or_else(|e| e.into_inner())
    }

    pub fn register_factory(&mut self, pf: Arc<Mutex<TF>>) -> JuizResult<Arc<Mutex<TF>>> {
        let type_name = juiz_lock(&pf)?.type_name().to_string();
        log::trace!(
            "StoreWorker({})::registerfactory(Factory(type_name={:?})) called",
            self.name,
            type_name
        );
        if self.factories.contains_key(&type_name) {
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

    pub fn register(&self, p: Arc<RwLock<T>>) -> JuizResult<Arc<RwLock<T>>> {
        let id = juiz_borrow(&p)?.identifier().clone();
        log::trace!(
            "StoreWorker({})::register(Object(id={:?})) called",
            self.name,
            id
        );
        self.write_objects().insert(id.clone(), p);
        self.get(&id)
    }

    /// id のオブジェクトを外す。無ければ None
    pub fn deregister_by_id(&self, id: &Identifier) -> Option<Arc<RwLock<T>>> {
        log::trace!(
            "StoreWorker({})::deregister_by_id(id={:?}) called",
            self.name,
            id
        );
        self.write_objects().remove(id)
    }

    pub fn get_with_error_ignore(&self, id: &Identifier) -> JuizResult<Arc<RwLock<T>>> {
        // 読み込みロックを match の外で離す (None の枝でもう一度読むので、同じスレッドで二重に取らない)
        let found = self.read_objects().get(id).cloned();
        match found {
            Some(p) => Ok(p),
            None => {
                log::trace!("StoreWorker({})::get_with_error_ignore(id={:?})が失敗しました。オブジェクトが見つかりません。", self.name, id);
                log::trace!(" - CoreStore includes objects[");
                for (k, _v) in self.read_objects().iter() {
                    log::trace!("    - {:?}", k);
                }
                log::trace!("]");
                Err(anyhow::Error::from(JuizError::ObjectCanNotFoundByIdError {
                    id: id.clone(),
                }))
            }
        }
    }

    pub fn get(&self, id: &Identifier) -> JuizResult<Arc<RwLock<T>>> {
        // 読み込みロックを match の外で離す (None の枝でもう一度読むので、同じスレッドで二重に取らない)
        let found = self.read_objects().get(id).cloned();
        match found {
            Some(p) => Ok(p),
            None => {
                log::error!(
                    "StoreWorker({})::get(id={:?})が失敗しました。オブジェクトが見つかりません。",
                    self.name,
                    id
                );
                log::trace!(" - CoreStore includes objects[");
                for (k, _v) in self.read_objects().iter() {
                    log::trace!("    - {:?}", k);
                }
                log::trace!("]");
                Err(anyhow::Error::from(JuizError::ObjectCanNotFoundByIdError {
                    id: id.clone(),
                }))
            }
        }
    }

    pub fn list_ids(&self) -> JuizResult<Value> {
        let mut prof = jvalue!([]);
        let o_array = get_array_mut(&mut prof)?;
        self.read_objects()
            .iter()
            .for_each(|(identifier, _arc_obj)| {
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
}
