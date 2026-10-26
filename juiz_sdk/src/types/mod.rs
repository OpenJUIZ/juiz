//! Process の引数・戻り値のデータ型 (issue #163、仕様 docs/spec/datatype.md)
//!
//! 型のモデル・指紋・値の検査・互換性の判定はクレート `juiz_types` にあり、ここで再公開する。
//! `juiz_types` の関数は `juiz_types::TypeError` を返す。juiz_sdk の manifest や `JuizType` を通したときは
//! 同じ名前の [`JuizError`] の変種 (`TypeSchemaInvalidError` など) に変換する ([`type_error`])。

mod juiz_type;

pub use juiz_type::{JuizType, TypeTag};
pub use juiz_types::*;

use crate::prelude::*;
use anyhow::anyhow;

impl From<TypeError> for JuizError {
    fn from(e: TypeError) -> Self {
        match e {
            TypeError::SchemaInvalid { path, message } => {
                JuizError::TypeSchemaInvalidError { path, message }
            }
            TypeError::Incompatible {
                writer,
                reader,
                path,
                reason,
            } => JuizError::TypeIncompatibleError {
                writer,
                reader,
                path,
                reason,
            },
            TypeError::ValueDoesNotMatch {
                type_name,
                path,
                reason,
            } => JuizError::ValueDoesNotMatchTypeError {
                type_name,
                path,
                reason,
            },
        }
    }
}

/// `TypeError` を `JuizError` にして anyhow のエラーにする (`map_err(type_error)?` で使う)
pub fn type_error(e: TypeError) -> anyhow::Error {
    anyhow!(JuizError::from(e))
}

/// 型の `type_name` (仕様 §6 の表) を `ArgumentType` で返す
pub trait ArgumentTypeOf {
    fn argument_type(&self) -> JuizResult<ArgumentType>;
}

impl ArgumentTypeOf for DataType {
    fn argument_type(&self) -> JuizResult<ArgumentType> {
        self.argument_type_name().try_into()
    }
}

impl ArgumentTypeOf for TypeSchema {
    fn argument_type(&self) -> JuizResult<ArgumentType> {
        self.data_type().map_err(type_error)?.argument_type()
    }
}
