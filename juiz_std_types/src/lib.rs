//! juiz の標準の型 `juiz_std` (issue #199、仕様 docs/spec/datatype.md §24.4)
//!
//! juiz_idl に組み込んだ JIDL (`juiz_std/common.jidl`・`geometry.jidl`・`sensor.jidl`) から build.rs で作った型。
//! 名前空間 `juiz_std.geometry` の型は [`geometry`] (`juiz_std::geometry` と同じもの) に置く。
//!
//! ```
//! use juiz_sdk::types::JuizType;
//! use juiz_std_types::geometry::{Point2D, Pose2D};
//!
//! let pose = Pose2D { position: Point2D { x: 1.0, y: 2.0 }, theta: 0.5 };
//! assert_eq!(Pose2D::from_value(&pose.to_value().unwrap()).unwrap(), pose);
//! assert_eq!(Pose2D::type_schema().unwrap().name, "juiz_std.geometry.Pose2D");
//! ```
//!
//! 自分の JIDL から使うときは `import idl "juiz_std/geometry.jidl";` と書き、build.rs で
//! `extern_path("juiz_std", "::juiz_std_types::juiz_std")` を指定すると、型を作り直さずにこのクレートの型を参照する。

include!(concat!(env!("OUT_DIR"), "/juiz_idl.rs"));

pub use juiz_std::{common, geometry, sensor};

/// build.rs の `extern_path` に渡す、名前空間 `juiz_std` に当たるモジュールのパス
pub const RUST_PATH: &str = "::juiz_std_types::juiz_std";
