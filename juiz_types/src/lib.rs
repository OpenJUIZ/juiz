//! juiz の Process の引数・戻り値のデータ型 (issue #163、仕様 docs/spec/datatype.md)
//!
//! 型の正本は Avro 1.11 のスキーマ (.avsc)。このクレートは .avsc の読み書き、指紋、辞書の値の検査、
//! 互換性の判定を持ち、juiz_sdk に依存しない (生成器 juiz-idl などからも使えるように)。
//! `juiz_sdk::types` がこのクレートを再公開し、`JuizType` トレイトを足している。
//!
//! ```
//! use juiz_types::*;
//! use serde_json::json;
//!
//! let point = DataType::Record(
//!     RecordType::new("robot.geometry", "Point2D")
//!         .field(RecordField::new("x", DataType::Double))
//!         .field(RecordField::new("y", DataType::Double).default_value(json!(0.0)).unwrap()),
//! );
//! let schema = TypeSchema::from_data_type(&point).unwrap();
//! assert_eq!(schema.name, "robot.geometry.Point2D");
//! assert!(schema.fingerprint.starts_with("sha256:"));
//! schema.check_value(&json!({"x": 1.0})).unwrap();
//! assert!(schema.check_value(&json!({"y": 1.0})).is_err());
//! ```

mod compatibility;
mod data_type;
mod type_schema;
mod value_check;

pub use compatibility::{compare_data_types, compare_types, ensure_compatible, Compatibility};
pub use data_type::{
    fingerprint_of_canonical_form, is_valid_type_name, validate_type_name, DataType, EnumType,
    RecordField, RecordType, ANY_FULL_NAME, IMAGE_FULL_NAME, RESERVED_NAMESPACE,
};
pub use type_schema::TypeSchema;
pub use value_check::{
    check_value, read_value, ATTR_MAXIMUM, ATTR_MAX_ITEMS, ATTR_MAX_LENGTH, ATTR_MINIMUM,
    ATTR_MIN_ITEMS,
};

/// このクレートのエラー。juiz_sdk は同じ名前の `JuizError` の変種に変換する (仕様 §8)
#[derive(thiserror::Error, Debug, Clone, PartialEq)]
pub enum TypeError {
    /// 型のスキーマ (や JIDL) が仕様 §3 の規則に合わない。`path` は型の中の位置
    #[error("Type schema is invalid at '{path}': {message}")]
    SchemaInvalid { path: String, message: String },
    /// writer の型の値を reader の型として読めない (仕様 §7)。`writer` / `reader` は型の識別子
    #[error("Type {writer} cannot be read as {reader} at '{path}': {reason}")]
    Incompatible {
        writer: String,
        reader: String,
        path: String,
        reason: String,
    },
    /// 辞書の値が型に合わない (仕様 §4)
    #[error("Value does not match type {type_name} at '{path}': {reason}")]
    ValueDoesNotMatch {
        type_name: String,
        path: String,
        reason: String,
    },
}

pub type Result<T> = std::result::Result<T, TypeError>;
