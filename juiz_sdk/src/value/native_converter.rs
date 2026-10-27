//! C ABI のプラグイン (C++) 向けの、データ型とネイティブ値の関数
//! (issue #195 データ型 Step 6、#177 遅延評価 L3、仕様 docs/spec/datatype.md §21・lazy-evaluation.md §8・§9)
//!
//! - `juiz_type_*`: Avro のスキーマ (.avsc の JSON) から型を作り、型参照 (仕様 §6) と辞書の読み方 (仕様 §4) を C から使う。
//!   型のモデル・指紋・値の読み方は Rust (juiz_types) のものをそのまま使うので、指紋と規則は Rust・Python と一致する。
//! - `capsule_set_native` / `capsule_ptr_borrow_native`: C++ の値を encode せずに Capsule に包み、
//!   型タグ (完全名・指紋・ABI タグ) が同じ C++ の受け取り側には、その値をそのまま貸す。
//!
//! 宣言は bindings/cjuiz/include/juiz_sdk.h と合わせること。

use std::any::Any;
use std::ffi::c_void;
use std::os::raw::c_char;

use super::converter_error::*;
use crate::prelude::*;
use crate::types::{read_value, DataType, TypeSchema, TypeTag};

/// Avro のスキーマが仕様 §3 に合わない
pub const JUIZ_TYPE_SCHEMA_INVALID_ERROR: i64 = -701;
/// 辞書の値が型に合わない (仕様 §4)
pub const JUIZ_VALUE_DOES_NOT_MATCH_TYPE_ERROR: i64 = -702;
/// 値が型タグの同じ C++ のネイティブ値でない (辞書を通して読む)
pub const JUIZ_NATIVE_NOT_MATCHED: i64 = -703;

/// C から使う型。`juiz_type_new` で作り、`juiz_type_free` で解放する
pub struct JuizTypeHandle {
    data_type: DataType,
    schema: TypeSchema,
}

/// Avro のスキーマ (JSON の文字列) から型を作る。合わなければ JUIZ_TYPE_SCHEMA_INVALID_ERROR (理由はログに出す)
#[no_mangle]
pub unsafe extern "C" fn juiz_type_new(
    avro_json: *const c_char,
    out: *mut *mut JuizTypeHandle,
) -> i64 {
    let Some(avro) = c_str_to_str(avro_json) else {
        return JUIZ_VALUE_CONVERTER_ERROR;
    };
    let handle = DataType::from_avro_str(avro).and_then(|data_type| {
        TypeSchema::from_data_type(&data_type).map(|schema| JuizTypeHandle { data_type, schema })
    });
    match handle {
        Ok(h) => {
            *out = Box::into_raw(Box::new(h));
            JUIZ_OK
        }
        Err(e) => {
            log::error!("juiz_type_new() failed. {e}");
            JUIZ_TYPE_SCHEMA_INVALID_ERROR
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn juiz_type_free(t: *mut JuizTypeHandle) {
    if !t.is_null() {
        drop(Box::from_raw(t));
    }
}

/// `{"schema": 型参照 {name, fingerprint, avro}, "type_name": manifest の type_name}` を callback に渡す
#[no_mangle]
pub unsafe extern "C" fn juiz_type_describe(
    t: *mut JuizTypeHandle,
    callback: extern "C" fn(*mut c_void, *mut Value) -> i64,
    arg: *mut c_void,
) -> i64 {
    let Some(t) = t.as_ref() else {
        return JUIZ_VALUE_CONVERTER_ERROR;
    };
    let mut v = jvalue!({
        "schema": t.schema,
        "type_name": t.data_type.argument_type_name(),
    });
    callback(arg, &mut v)
}

/// capsule_ptr の値を型で読み (仕様 §4。足りないフィールドを既定値で埋め、型にないキーを落とし、
/// 列挙の知らない記号を既定値にする)、読んだ値を callback に渡す。
/// 合わなければ JUIZ_VALUE_DOES_NOT_MATCH_TYPE_ERROR (理由はログに出す)。
/// 値が Native なら encode する (ロックの外で、lazy-evaluation.md §6)。
#[no_mangle]
pub unsafe extern "C" fn capsule_ptr_read_typed_value(
    capsule_ptr: *mut CapsulePtr,
    t: *mut JuizTypeHandle,
    path: *const c_char,
    callback: extern "C" fn(*mut c_void, *mut Value) -> i64,
    arg: *mut c_void,
) -> i64 {
    let (Some(cp), Some(t)) = (capsule_ptr.as_ref(), t.as_ref()) else {
        return JUIZ_VALUE_CONVERTER_ERROR;
    };
    let path = c_str_to_str(path).unwrap_or("");
    let read = match cp.lock_as_value(|v| read_value(&t.data_type, v, path)) {
        Ok(r) => r,
        Err(_) => return JUIZ_VALUE_TYPE_ERROR,
    };
    match read {
        Ok(mut v) => callback(arg, &mut v),
        Err(e) => {
            log::error!("capsule_ptr_read_typed_value() failed. {e}");
            JUIZ_VALUE_DOES_NOT_MATCH_TYPE_ERROR
        }
    }
}

/// C++ の値の codec の表 (juiz_sdk.h の juiz_native_codec と同じ並び)。プラグインの中の static に置く
#[repr(C)]
pub struct NativeCodec {
    pub name: *const c_char,
    pub fingerprint: *const c_char,
    pub abi: *const c_char,
    /// obj を辞書 (仕様 §4) にして out に書く。失敗なら負の値
    pub encode: extern "C" fn(*const c_void, *mut Capsule) -> i64,
    /// obj を解放する
    pub destroy: extern "C" fn(*mut c_void),
}

/// C++ のプラグインが返した値。encode・解放はプラグインの codec を呼ぶ
pub struct CppNative {
    obj: *mut c_void,
    codec: *const NativeCodec,
    tag: TypeTag,
}

// C++ の値は const のまま読むだけ (encode と貸し出し)。解放は最後の参照が消えたときの 1 回だけ
unsafe impl Send for CppNative {}
unsafe impl Sync for CppNative {}

impl std::fmt::Debug for CppNative {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("CppNative({})", self.tag.name))
    }
}

impl Drop for CppNative {
    fn drop(&mut self) {
        if !self.obj.is_null() {
            unsafe { ((*self.codec).destroy)(self.obj) };
            self.obj = std::ptr::null_mut();
        }
    }
}

impl NativeValue for CppNative {
    fn language(&self) -> &str {
        "cpp"
    }
    fn kind(&self) -> NativeKind {
        NativeKind::Value
    }
    fn type_name(&self) -> String {
        self.tag.name.clone()
    }
    fn encode(&self) -> JuizResult<EncodedValue> {
        let mut out = Capsule::empty();
        let r = unsafe { ((*self.codec).encode)(self.obj, &mut out) };
        if r != JUIZ_OK {
            return Err(anyhow::anyhow!(JuizError::ValueTypeError {
                message: format!("encode of C++ value ({}) returned {r}", self.tag.name),
            }));
        }
        match out.to_value() {
            Some(v) => Ok(EncodedValue::Value(v)),
            None => Err(anyhow::anyhow!(JuizError::ValueTypeError {
                message: format!("encode of C++ value ({}) wrote no value", self.tag.name),
            })),
        }
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn type_tag(&self) -> Option<&TypeTag> {
        Some(&self.tag)
    }
}

unsafe fn codec_tag(codec: &NativeCodec) -> Option<TypeTag> {
    Some(TypeTag {
        name: c_str_to_str(codec.name)?.to_owned(),
        fingerprint: c_str_to_str(codec.fingerprint)?.to_owned(),
        abi: c_str_to_str(codec.abi)?.to_owned(),
    })
}

/// C++ の値 obj を encode せずに capsule に入れる。obj の持ち主は juiz になり、要らなくなったら codec の destroy で解放する。
/// codec は obj が解放されるまで (プラグインを外すまで) 有効でなければならない。
/// 失敗したときは obj を destroy で解放する
#[no_mangle]
pub unsafe extern "C" fn capsule_set_native(
    capsule: *mut Capsule,
    obj: *mut c_void,
    codec: *const NativeCodec,
) -> i64 {
    let (Some(cap), Some(c)) = (capsule.as_mut(), codec.as_ref()) else {
        return JUIZ_VALUE_CONVERTER_ERROR;
    };
    let Some(tag) = codec_tag(c) else {
        (c.destroy)(obj);
        return JUIZ_VALUE_CONVERTER_ERROR;
    };
    *cap = Capsule::from_native(CppNative { obj, codec, tag });
    JUIZ_OK
}

/// `capsule_ptr_borrow_native` が貸した値を、返すまで生かしておくための参照
pub struct NativeRef(#[allow(dead_code)] NativeCell);

/// capsule_ptr の値が、型タグ (完全名・指紋・ABI タグ) が codec と同じ C++ のネイティブ値なら、
/// その値を *obj に貸す (encode も複製もしない、lazy-evaluation.md §9.2 の 1 段目)。
/// 貸した値は const として読み、使い終わったら *native_ref を juiz_native_ref_release で返す。
/// そうでなければ JUIZ_NATIVE_NOT_MATCHED (辞書を通して読む)
#[no_mangle]
pub unsafe extern "C" fn capsule_ptr_borrow_native(
    capsule_ptr: *mut CapsulePtr,
    codec: *const NativeCodec,
    obj: *mut *const c_void,
    native_ref: *mut *mut NativeRef,
) -> i64 {
    let (Some(cp), Some(c)) = (capsule_ptr.as_ref(), codec.as_ref()) else {
        return JUIZ_VALUE_CONVERTER_ERROR;
    };
    let Some(tag) = codec_tag(c) else {
        return JUIZ_VALUE_CONVERTER_ERROR;
    };
    let cell = match cp.native_cell() {
        Ok(Some(cell)) => cell,
        Ok(None) => return JUIZ_NATIVE_NOT_MATCHED,
        Err(_) => return JUIZ_CAPSULEPTR_LOCK_ERROR,
    };
    let ptr = match cell.value().as_any().downcast_ref::<CppNative>() {
        Some(n) if n.tag == tag => n.obj as *const c_void,
        _ => return JUIZ_NATIVE_NOT_MATCHED,
    };
    *obj = ptr;
    *native_ref = Box::into_raw(Box::new(NativeRef(cell)));
    JUIZ_OK
}

#[no_mangle]
pub unsafe extern "C" fn juiz_native_ref_release(native_ref: *mut NativeRef) {
    if !native_ref.is_null() {
        drop(Box::from_raw(native_ref));
    }
}
