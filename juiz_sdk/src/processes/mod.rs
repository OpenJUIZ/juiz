//! プロセスに関する機能パッケージ
//!

pub mod process;
pub mod process_ptr;

pub use process::{Process, ProcessBodyFunctionTrait, ProcessBodyFunctionType};
pub use process_ptr::ProcessPtr;
