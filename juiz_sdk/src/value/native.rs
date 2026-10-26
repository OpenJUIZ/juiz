//! 遅延評価のためのネイティブ値 (docs/spec/lazy-evaluation.md §3)
//!
//! 言語のプラグインが Process の戻り値を、juiz の共通形式 (Value / Image) に変換しないまま
//! Capsule に包むための型。共通形式が要るときだけ encode し、結果は覚えておく。

use std::{
    any::Any,
    sync::{Arc, OnceLock},
};

use crate::prelude::*;
use anyhow::anyhow;

/// encode したときの種類。encode せずに答えられなければならない
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeKind {
    Value,
    Image,
}

/// encode した結果。Capsule の中身の Value / Image と同じもの
#[derive(Clone, Debug)]
pub enum EncodedValue {
    Value(Value),
    Image(DynamicImage),
}

/// 型タグ (docs/spec/lazy-evaluation.md §9.1、datatype.md §12)。L1 の Python では使わない
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeTypeTag {
    /// 完全名。名前のない型は表示名
    pub name: String,
    /// "sha256:<64 桁>"
    pub fingerprint: String,
    /// ABI タグ。不透明な文字列で、完全一致だけで比べる
    pub abi: String,
}

/// 言語ごとのネイティブ値が実装する。juiz_sdk は中身を知らない
pub trait NativeValue: Send + Sync + std::fmt::Debug {
    /// 作った言語の名前 (python / rust / cpp)
    fn language(&self) -> &str;
    /// encode したときの種類。encode せずに答えられなければならない
    fn kind(&self) -> NativeKind;
    /// 表示用の型名 (trace・ログ・profile 用)。例: "numpy.ndarray"、"dict"
    fn type_name(&self) -> String;
    /// 共通形式を作る。失敗してよい
    fn encode(&self) -> JuizResult<EncodedValue>;
    /// 同じ言語の受け取り側が具体的な型へダウンキャストするため
    fn as_any(&self) -> &dyn Any;
    /// 型タグ。型なしの値と L1 の Python は None
    fn type_tag(&self) -> Option<&NativeTypeTag> {
        None
    }
}

struct NativeInner {
    value: Box<dyn NativeValue>,
    encoded: OnceLock<JuizResult<EncodedValue>>,
}

/// ネイティブ値と、encode 済みのキャッシュ。Clone は Arc の複製 (値は共有)
#[derive(Clone)]
pub struct NativeCell(Arc<NativeInner>);

impl std::fmt::Debug for NativeCell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.describe())
    }
}

impl NativeCell {
    pub fn new(value: impl NativeValue + 'static) -> Self {
        Self(Arc::new(NativeInner {
            value: Box::new(value),
            encoded: OnceLock::new(),
        }))
    }

    pub fn value(&self) -> &dyn NativeValue {
        self.0.value.as_ref()
    }

    pub fn kind(&self) -> NativeKind {
        self.0.value.kind()
    }

    /// `<language>:<type_name>`。encode しない
    pub fn describe(&self) -> String {
        format!("{}:{}", self.0.value.language(), self.0.value.type_name())
    }

    /// encode 済み (失敗も含む) か
    pub fn is_encoded(&self) -> bool {
        self.0.encoded.get().is_some()
    }

    /// encode して (初回だけ) 結果を返す。
    ///
    /// OnceLock::get_or_init は使わない。初期化中の別スレッドを待つ間に GIL を持っていると、
    /// encode 側が GIL を待って相互待ちになるため。同時に呼ばれたときは encode が 2 回走りうるが、
    /// 覚えるのは先に終わった方の結果だけ (docs/spec/lazy-evaluation.md §6)。
    pub fn encoded(&self) -> &JuizResult<EncodedValue> {
        if let Some(r) = self.0.encoded.get() {
            return r;
        }
        let result = self.0.value.encode();
        if let Err(e) = result.as_ref() {
            log::error!(
                "encode of native value ({}) failed. Error({e:#})",
                self.describe()
            );
        }
        let _ = self.0.encoded.set(result);
        self.0.encoded.get().unwrap()
    }

    /// encode した結果のエラーを、呼び出し元に返すためのエラーに作り直す
    pub(crate) fn encode_error(&self, e: &anyhow::Error) -> anyhow::Error {
        anyhow!(JuizError::NativeEncodeError {
            native: self.describe(),
            message: format!("{e:#}"),
        })
    }

    pub fn try_as_value(&self) -> JuizResult<Option<&Value>> {
        match self.encoded() {
            Ok(EncodedValue::Value(v)) => Ok(Some(v)),
            Ok(EncodedValue::Image(_)) => Ok(None),
            Err(e) => Err(self.encode_error(e)),
        }
    }

    pub fn try_as_image(&self) -> JuizResult<Option<&DynamicImage>> {
        match self.encoded() {
            Ok(EncodedValue::Image(v)) => Ok(Some(v)),
            Ok(EncodedValue::Value(_)) => Ok(None),
            Err(e) => Err(self.encode_error(e)),
        }
    }

    /// encode した結果を取り出す。ほかに参照が無ければキャッシュを複製せずに移す
    pub fn into_encoded(self) -> JuizResult<EncodedValue> {
        let _ = self.encoded();
        match Arc::try_unwrap(self.0) {
            Ok(inner) => inner.encoded.into_inner().unwrap(),
            Err(arc) => {
                let cell = NativeCell(arc);
                match cell.encoded() {
                    Ok(v) => Ok(v.clone()),
                    Err(e) => Err(cell.encode_error(e)),
                }
            }
        }
    }
}
