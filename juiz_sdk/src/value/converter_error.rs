pub const JUIZ_OK: i64 = 0;
pub const JUIZ_VALUE_TYPE_ERROR: i64 = -101;
/// C 文字列が NULL、または UTF-8 として読めないなど、値に変換できなかった
pub const JUIZ_VALUE_CONVERTER_ERROR: i64 = -103;

pub const JUIZ_CAPSULEPTR_LOCK_ERROR: i64 = -201;

pub const JUIZ_CAPSULEMAP_NO_VALUE: i64 = -301;

pub const JUIZ_CAPSULE_TYPE_ERROR: i64 = -401;
pub const JUIZ_CAPSULE_NO_VALUE: i64 = -402;

/// C から渡された NUL 終端文字列を UTF-8 の &str として読む。
/// NULL や不正な UTF-8 のときは None を返す(panic しない)。
pub(crate) unsafe fn c_str_to_str<'a>(v: *const std::os::raw::c_char) -> Option<&'a str> {
    if v.is_null() {
        return None;
    }
    std::ffi::CStr::from_ptr(v).to_str().ok()
}
