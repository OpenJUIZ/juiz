use std::collections::HashMap;

use crate::prelude::*;



pub struct Outlet {
    name: String, 
    destination_connections: HashMap<String, Box<dyn DestinationConnection>>,
    output_memo: CapsulePtr,
    use_memo: bool,
}

impl std::fmt::Debug for Outlet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Outlet").field("name", &self.name).field("output_memo", &self.output_memo).field("use_memo", &self.use_memo).finish()
    }
}

impl Outlet {

    pub fn new(name: &str, use_memo: bool) -> Outlet {
        log::trace!("Outlet()::new(use_memo='{}') called", use_memo);
        Outlet{
            name: name.to_owned(),
            destination_connections: HashMap::new(),
            output_memo: Capsule::empty().into(),
            use_memo: use_memo,
        }
    }

    // pub fn use_memo(&self) -> bool {
    //     self.use_memo
    // }
    #[allow(unused)]
    pub fn profile_full(&self) -> JuizResult<Value> {
        Ok(jvalue!({
            "destination_connections": self.destination_connections.iter().map(| (_name, dc) | -> Value { dc.profile().into() }).collect::<Vec<Value>>()
        }).into())
    }

    pub(crate) fn insert(&mut self, name: String, con: Box<dyn DestinationConnection + 'static>) -> () {
        self.destination_connections.insert(name, con);
    }

    pub(crate) fn destination_connections(&self) -> JuizResult<Vec<&Box<dyn DestinationConnection>>> {
        let mut v: Vec<&Box<dyn DestinationConnection>> = Vec::new();
        for c in self.destination_connections.values() {
            v.push(c);
        }
        Ok(v)
    }

    pub(crate) fn memo(&self) -> CapsulePtr {
        self.output_memo.clone()//.borrow()
    }

    #[allow(dead_code)]
    pub(crate) fn memo_mut(&self) -> CapsulePtr {
        self.output_memo.clone()//borrow_mut()
    }
    /*
    pub(crate) fn is_buffer_empty(&self) -> JuizResult<bool> {
        Ok(self.output_memo.borrow().get_value()?.is_null())
    }

    pub(crate) fn get_value(&self) -> JuizResult<&Value> {
        self.output_memo.borrow().get_value()
    }

    pub(crate) fn get_value_mut(&mut self) -> JuizResult<&mut Value> {
        self.output_memo.borrow().get_value_mut()
    }
    */

    /// 出力バッファー (memo) にデータを書き込む
    pub(crate) fn set_value(&self, capsule: CapsulePtr) -> CapsulePtr {
        log::trace!("Outlet({})::set_value()が呼ばれました。", self.name);
        if self.use_memo {
            log::trace!("【set_value】use_memoがtrueなのでmemoにします。");
            self.output_memo.replace(capsule);
        } else {
            log::debug!("【set_value】use_memoがfalseです。値は消費されます。");
            return capsule;
        }
        self.memo()
    }

    /// 出力を出力接続 (DestinationConnection) に投げる
    /// 
    pub fn push(&self, output: CapsulePtr) -> JuizResult<CapsulePtr> {
        log::trace!("Outlet({})::push()が呼ばれました。", self.name);
        for (_name, dc) in self.destination_connections.iter() {
            let cp = dc.profile();
            log::debug!("【push】DestinationConnection ({cp}) があります。pushします。");
            let _ = dc.push(output.clone())?;
        }
        log::trace!("【push】が成功しました。");
        return Ok(output);
    }

}