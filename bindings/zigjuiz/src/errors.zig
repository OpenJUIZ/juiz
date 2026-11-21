//! zigjuiz が返すエラーコード。juiz_sdk.h の JUIZ_ZIG_* と同じ値 (-11xx。C++ の -10xx の次)。
//!
//! どちらのコードでも、zigjuiz は戻り値の capsule にエラー文 (文字列) を入れる。juiz はそれを読んで
//! エラー (JuizError::ZigPluginError) の message にする。
const c = @import("c");

/// Process の関数が Zig のエラー (error.Foo) を返した。エラー文は "error.Foo"
pub const zig_error_returned: i64 = c.JUIZ_ZIG_ERROR_RETURNED;
/// 引数が無いか、引数の struct のフィールドの型で読めない。エラー文は引数の名前と理由
pub const zig_argument_error: i64 = c.JUIZ_ZIG_ARGUMENT_ERROR;
/// 戻り値を capsule に書けなかった (メモリ不足など)
pub const zig_result_error: i64 = c.JUIZ_ZIG_RESULT_ERROR;
/// ContainerProcess に、期待する状態の型の zigjuiz の Container ではないもの (別の言語の Container、別の型の状態) が渡された (#574)
pub const zig_container_type_error: i64 = c.JUIZ_ZIG_CONTAINER_TYPE_ERROR;
