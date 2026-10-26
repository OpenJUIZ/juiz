//! Process の引数・戻り値のデータ型 (issue #163、仕様 docs/spec/datatype.md)
//!
//! 型の正本は Avro 1.11 のスキーマ (.avsc)。通信は今の辞書のままで、型は契約として manifest に載せる。
//!
//! ```
//! use juiz_sdk::prelude::*;
//! use juiz_sdk::types::*;
//!
//! let point = DataType::Record(
//!     RecordType::new("robot.geometry", "Point2D")
//!         .field(RecordField::new("x", DataType::Double))
//!         .field(RecordField::new("y", DataType::Double).default_value(jvalue!(0.0)).unwrap()),
//! );
//! let schema = TypeSchema::from_data_type(&point).unwrap();
//! assert_eq!(schema.name, "robot.geometry.Point2D");
//! assert!(schema.fingerprint.starts_with("sha256:"));
//! schema.check_value(&jvalue!({"x": 1.0})).unwrap();
//! assert!(schema.check_value(&jvalue!({"y": 1.0})).is_err());
//! ```

mod compatibility;
mod data_type;
mod juiz_type;
mod type_schema;
mod value_check;

pub use compatibility::{compare_data_types, compare_types, ensure_compatible, Compatibility};
pub use data_type::{
    fingerprint_of_canonical_form, is_valid_type_name, validate_type_name, DataType, EnumType,
    RecordField, RecordType, ANY_FULL_NAME, IMAGE_FULL_NAME, RESERVED_NAMESPACE,
};
pub use juiz_type::{JuizType, TypeTag};
pub use type_schema::TypeSchema;
pub use value_check::{check_value, read_value};
