use std::cell::{Cell, RefCell};

use crate::prelude::*;
use juiz_sdk::{connection_identifier::ConnectionIdentifier, connections::ConnectionProfile};
/// inlet.rs
///
///
///

pub struct Inlet {
    name: String,
    source_connections: Vec<Box<dyn SourceConnection>>,
    default_value: CapsulePtr,
    buffer: RefCell<Option<CapsulePtr>>,
    /// bind (p_apply) で値が設定されてから、まだ collect_value で読まれていないとき true
    updated: Cell<bool>,
}

impl std::fmt::Debug for Inlet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inlet")
            .field("name", &self.name)
            .field("default_value", &self.default_value)
            .field("buffer", &self.buffer)
            .finish()
    }
}

impl Inlet {
    pub fn new(name: &str, default_value: Value) -> Inlet {
        Inlet {
            name: name.to_owned(),
            default_value: default_value.into(),
            source_connections: Vec::new(),
            buffer: RefCell::new(None),
            updated: Cell::new(false),
        }
    }

    pub fn name(&self) -> &String {
        &self.name
    }

    // pub fn source_connection_by_identifier(&mut self, identifier: &Identifier) -> Option<&Box<dyn SourceConnection>> {
    //     for c in self.source_connections.iter() {
    //         if c.identifier() == identifier {
    //             return Some(c)
    //         }
    //     }
    //     return None
    // }

    pub fn source_connections(&self) -> &Vec<Box<dyn SourceConnection>> {
        return &self.source_connections;
    }

    // pub fn source_connections_mut(&mut self) -> &mut Vec<Box<dyn SourceConnection>> {
    //     return &mut self.source_connections
    // }
    #[allow(unused)]
    pub fn profile_full(&self) -> JuizResult<Value> {
        Ok(jvalue!({
            "name": self.name,
            "source_connections": self.source_connections.iter().map(|sc| -> Value {
                sc.profile().into()
            }).collect::<Vec<Value>>()
        })
        .into())
    }

    /// 次の実行で使われる値 (p_apply で拘束した値か、push で届いた値)。
    /// まだ無いときと、値型でない (画像など) ときは None (issue #112)
    pub fn current_value(&self) -> Option<Value> {
        self.buffer
            .borrow()
            .as_ref()
            .and_then(|c| c.lock_as_value(|v| v.clone()).ok())
    }

    pub fn is_updated(&self) -> JuizResult<bool> {
        if self.updated.get() {
            return Ok(true);
        }
        //self.source_connections.iter().find_map(|sc| { if sc.is_source_updated() })
        for sc in self.source_connections.iter() {
            if sc.is_source_updated()? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// データを収集。
    ///
    /// まず全てのコネクションに対してpullでデータ収集を行う。
    /// 出力はself.bufferに保存されるため、あとからの接続が優先される。
    /// エラーはログは出すが無視する。
    /// 接続が無いか、全ての接続がPull型ではないか、もしくはPull型接続をpullしてもエラーがあった場合、
    /// self.bufferを確認する。bufferがNoneならばデフォルトの値を出力する。
    /// bufferにデータが残っていればそれを返す。
    pub fn collect_value(&self) -> CapsulePtr {
        log::trace!("Inlet({})::collect_value() called", self.name());
        self.updated.set(false);
        for sc in self.source_connections.iter() {
            if sc.connection_type() == ConnectionType::Pull {
                match sc.pull() {
                    Err(e) => {
                        log::error!("Pull data via Connection({}) in Inlet({})::collect_value() failed. Error: {:}", sc.identifier(), self.name(), e);
                    }
                    Ok(output) => {
                        self.buffer.replace(Some(output.clone()));
                        //return self.buffer.borrow().unwrap().clone();
                    }
                }
            }
        }
        if self.buffer.borrow().is_none() {
            self.default_value.clone()
        } else {
            self.buffer.borrow().clone().unwrap()
        }
    }

    pub(crate) fn insert(&mut self, con: Box<dyn SourceConnection + 'static>) {
        self.source_connections.push(con);
    }

    /// コネクションを外し、外したもののプロファイルを返す
    pub(crate) fn remove(&mut self, id: &ConnectionIdentifier) -> Option<ConnectionProfile> {
        let index = self
            .source_connections
            .iter()
            .position(|c| c.identifier().is_same_connection(id))?;
        Some(self.source_connections.remove(index).profile())
    }

    /// push 接続で届いた値をバッファに保存する。
    ///
    /// p_apply と同じバッファに書くので、後から来た値が次の execute で使われる (issue #70)。
    /// 届いた値ですぐ計算するため、updated フラグは立てない。
    pub fn store(&self, value: CapsulePtr) {
        self.buffer.replace(Some(value));
    }

    pub fn bind(&mut self, value: CapsulePtr) -> JuizResult<CapsulePtr> {
        // 後から設定した値で上書きし、次の invoke でメモを使わずに再計算させる (issue #18)
        self.buffer.replace(Some(value.clone()));
        self.updated.set(true);
        Ok(value)
    }
}
