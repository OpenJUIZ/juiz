//! Rust の Process の戻り値をネイティブ値で渡す (遅延評価 L2 #176、docs/spec/lazy-evaluation.md §8・§9)
//!
//! `#[derive(JuizType)]` と juiz-idl が生成する record の型は、Process の戻り値になったとき
//! 辞書に encode せずに [`RustNative`] で `Capsule::Native` に包む。受け取り側の Rust の Process は、
//! 型タグ (完全名・指紋・ABI タグ) が一致すれば値をそのまま受け取り、ほかに参照が無ければ移す。

use std::any::Any;
use std::sync::OnceLock;

use super::{JuizType, TypeTag};
use crate::prelude::*;

/// この cdylib の Rust の ABI タグ (datatype.md §12)。`rust:<rustc の版>:<juiz_sdk の版>:<アドレス>`
///
/// アドレスはこの関数の中の static のもの。juiz_sdk はプラグイン (cdylib) ごとに静的にリンクされるので、
/// 同じプラグインの中でだけ一致する。別のプラグインとは `repr(Rust)` の並びと `TypeId` が一致する
/// 保証がないので、別のプラグインの値はいつも辞書を通す (lazy-evaluation.md §9.1)。
pub fn rust_abi_tag() -> &'static str {
    static TAG: OnceLock<String> = OnceLock::new();
    TAG.get_or_init(|| {
        format!(
            "rust:{}:{}:{:p}",
            env!("JUIZ_SDK_RUSTC_VERSION"),
            env!("CARGO_PKG_VERSION"),
            &TAG as *const _
        )
    })
}

/// Rust の値のネイティブ値。encode は `JuizType::to_value`
pub struct RustNative<T> {
    value: T,
    tag: TypeTag,
}

impl<T: JuizType> RustNative<T> {
    /// 型タグを `T::type_tag()` から作る。`T` が ABI タグを持たなければ None
    pub fn new(value: T) -> Option<Self> {
        T::type_tag().map(|tag| Self { value, tag })
    }

    /// 型タグを指定して作る (テスト用。別のプラグインから来た値を作るときなど)
    pub fn with_tag(value: T, tag: TypeTag) -> Self {
        Self { value, tag }
    }

    pub fn value(&self) -> &T {
        &self.value
    }
}

impl<T> std::fmt::Debug for RustNative<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("RustNative({})", self.tag.name))
    }
}

impl<T: JuizType + Send + Sync + 'static> NativeValue for RustNative<T> {
    fn language(&self) -> &str {
        "rust"
    }
    fn kind(&self) -> NativeKind {
        NativeKind::Value
    }
    fn type_name(&self) -> String {
        self.tag.name.clone()
    }
    fn encode(&self) -> JuizResult<EncodedValue> {
        Ok(EncodedValue::Value(self.value.to_value()?))
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn type_tag(&self) -> Option<&TypeTag> {
        Some(&self.tag)
    }
    fn into_any(self: Box<Self>) -> Option<Box<dyn Any>> {
        Some(self)
    }
}

/// 戻り値を包む。`T` が ABI タグを持てば `Capsule::Native`、持たなければ辞書にする
pub fn native_into_capsule<T: JuizType + Send + Sync + 'static>(value: T) -> JuizResult<Capsule> {
    match T::type_tag() {
        Some(tag) => Ok(Capsule::from_native(RustNative::with_tag(value, tag))),
        None => Ok(value.to_value()?.into()),
    }
}

/// ネイティブ値のまま受け取れるか (照合の 1 段目、lazy-evaluation.md §9.2)
fn is_same_native<T: JuizType + 'static>(native: &dyn NativeValue) -> bool {
    match T::type_tag() {
        // 型タグが同じでも、同じ形の別の Rust の型ならダウンキャストできないので辞書を通す
        Some(tag) => native.type_tag() == Some(&tag) && native.as_any().is::<RustNative<T>>(),
        None => false,
    }
}

/// 辞書を通して読む (照合の 2 段目)。Native なら encode が起きる
fn decode<T: JuizType>(capsule: &CapsulePtr) -> JuizResult<T> {
    capsule.lock_as_value(|v| T::from_value(v))?
}

fn clone_native<T: JuizType + Clone + 'static>(native: &dyn NativeValue) -> Option<T> {
    native
        .as_any()
        .downcast_ref::<RustNative<T>>()
        .map(|n| n.value.clone())
}

/// 引数を参照で読む。型タグが一致するネイティブ値なら複製し、そうでなければ辞書を通す
pub fn native_from_capsule_ptr<T: JuizType + Clone + 'static>(
    capsule: &CapsulePtr,
) -> JuizResult<T> {
    if capsule.is_native()? {
        if let Some(v) = capsule.lock_as_native(|n| {
            if is_same_native::<T>(n) {
                clone_native::<T>(n)
            } else {
                None
            }
        })? {
            return Ok(v);
        }
    }
    decode(capsule)
}

/// 引数を受け取る。型タグが一致するネイティブ値で、ほかに参照が無ければ移す (Composite の中、
/// lazy-evaluation.md §8)。参照があれば複製し、型タグが違えば辞書を通す
pub fn native_extract<T: JuizType + Clone + 'static>(capsule: CapsulePtr) -> JuizResult<T> {
    if !capsule.is_native()? || !capsule.lock_as_native(|n| is_same_native::<T>(n))? {
        return decode(&capsule);
    }
    let capsule = match capsule.try_into_capsule() {
        Err(shared) => return native_from_capsule_ptr(&shared),
        Ok(c) => c,
    };
    let cell = match capsule.into_native_cell() {
        // 確かめた後に差し替わっていた
        Err(c) => return decode(&c.into()),
        Ok(cell) => cell,
    };
    match cell.try_into_value() {
        Ok(boxed) => match boxed.into_any().map(|a| a.downcast::<RustNative<T>>()) {
            Some(Ok(n)) => Ok(n.value),
            // is_same_native で確かめたので来ない
            _ => Err(anyhow::anyhow!(JuizError::ValueTypeError {
                message: format!("native value is not {}", T::type_identifier()),
            })),
        },
        Err(cell) => match clone_native::<T>(cell.value()) {
            Some(v) => Ok(v),
            None => decode(&Capsule::from(cell).into()),
        },
    }
}
