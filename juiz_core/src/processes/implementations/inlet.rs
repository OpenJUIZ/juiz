use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

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
    /// p_apply で拘束した値か、push で届いた値。
    ///
    /// ProcessPtr は RwLock なので、読み取りロックのまま別々のスレッドから同時に触られる
    /// (EC の周期実行・接続先からの push・profile の取得など)。RefCell だと二重借用で panic するので
    /// Mutex にする (issue #566)
    buffer: Mutex<Option<CapsulePtr>>,
    /// bind (p_apply) で値が設定されてから、まだ collect_value で読まれていないとき true
    updated: AtomicBool,
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
            buffer: Mutex::new(None),
            updated: AtomicBool::new(false),
        }
    }

    /// buffer のロック。中身は Option の差し替えだけなので、毒されていても中身をそのまま使う
    fn lock_buffer(&self) -> MutexGuard<'_, Option<CapsulePtr>> {
        self.buffer.lock().unwrap_or_else(|e| e.into_inner())
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
        // Capsule のロックを buffer のロックの中で取らないよう、先に複製して放す
        let buffered = self.lock_buffer().clone();
        buffered.and_then(|c| c.lock_as_value(|v| v.clone()).ok())
    }

    /// 今バッファにある値 (p_apply で拘束した値か、push で届いた値)。
    ///
    /// current_value と違い画像なども返す。pull せず、updated フラグにも触らない (issue #83 の call_partial 用)
    pub fn buffered_value(&self) -> Option<CapsulePtr> {
        self.lock_buffer().clone()
    }

    pub fn is_updated(&self) -> JuizResult<bool> {
        if self.updated.load(Ordering::Acquire) {
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
        self.updated.store(false, Ordering::Release);
        for sc in self.source_connections.iter() {
            if sc.connection_type() == ConnectionType::Pull {
                match sc.pull() {
                    Err(e) => {
                        log::error!("Pull data via Connection({}) in Inlet({})::collect_value() failed. Error: {:}", sc.identifier(), self.name(), e);
                    }
                    Ok(output) => {
                        // pull の間は buffer のロックを持たない (上流の実行は時間がかかることがある)
                        *self.lock_buffer() = Some(output);
                    }
                }
            }
        }
        // 一度のロックで読む (確かめてから読むまでの間に別スレッドが書いても食い違わない)
        self.lock_buffer()
            .clone()
            .unwrap_or_else(|| self.default_value.clone())
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
        *self.lock_buffer() = Some(value);
    }

    pub fn bind(&mut self, value: CapsulePtr) -> JuizResult<CapsulePtr> {
        // 後から設定した値で上書きし、次の invoke でメモを使わずに再計算させる (issue #18)
        *self.lock_buffer() = Some(value.clone());
        self.updated.store(true, Ordering::Release);
        Ok(value)
    }
}
