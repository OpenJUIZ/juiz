//! Rust プラグインの C ABI の入口 `<symbol>_cabi` を生成する (docs/spec/rust-plugin-abi.md §3・§7、issue #329)
//!
//! 生成するのは、入口の表 (`juiz_sdk::cabi::JuizRustEntry`) の `static` と、juiz_sdk のグルーを呼ぶ
//! 薄い `extern "C"` 関数だけ。フレームの読み書き、`catch_unwind`、エラーコードはグルーの中にある。
//! 移行期間の間 (§11)、今の `extern "Rust"` の入口と `<symbol>_build_info` も今までどおり生成する。

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

/// 入口の表の種類
pub(crate) enum CabiKind {
    Process,
    Container,
    ContainerProcess,
    Component,
}

/// `<symbol>_cabi` を生成する。
///
/// - `manifest`: manifest を返す式 (例 `manifest()`)
/// - `function`: 利用者の関数 (マクロで `CapsuleMap` を受け取る形にしたもの)。Component では使わない
pub(crate) fn cabi_entry_tokenstream(
    symbol_name: &str,
    kind: CabiKind,
    manifest: TokenStream,
    function: Option<&syn::Ident>,
) -> TokenStream {
    let cabi_ident = format_ident!("{}_cabi", symbol_name);
    let common = quote! {
        extern "C" fn __juiz_cabi_manifest(out: *const juiz_sdk::cabi::JuizBytesSink) -> i64 {
            juiz_sdk::cabi::write_manifest(out, || #manifest)
        }
    };
    let (functions, entry) = match kind {
        CabiKind::Process => (
            quote! {
                extern "C" fn __juiz_cabi_info(out: *const juiz_sdk::cabi::JuizBytesSink) -> i64 {
                    juiz_sdk::cabi::process_info(out)
                }
                extern "C" fn __juiz_cabi_call(
                    args: *const juiz_sdk::cabi::JuizFrameIn,
                    out: *const juiz_sdk::cabi::JuizFrameOut,
                ) -> i64 {
                    juiz_sdk::cabi::call_process(args, out, #function)
                }
            },
            quote! {
                juiz_sdk::cabi::JuizRustEntry::process(__juiz_cabi_manifest, __juiz_cabi_info, __juiz_cabi_call)
            },
        ),
        CabiKind::Container => (
            quote! {
                extern "C" fn __juiz_cabi_info(out: *const juiz_sdk::cabi::JuizBytesSink) -> i64 {
                    juiz_sdk::cabi::container_info(out, #function)
                }
                extern "C" fn __juiz_cabi_create(
                    manifest_json: juiz_sdk::cabi::JuizBytes,
                    args: *const juiz_sdk::cabi::JuizFrameIn,
                    handle: *mut *mut ::std::ffi::c_void,
                    error: *const juiz_sdk::cabi::JuizBytesSink,
                ) -> i64 {
                    juiz_sdk::cabi::create_container(manifest_json, args, handle, error, #function)
                }
                extern "C" fn __juiz_cabi_destroy(handle: *mut ::std::ffi::c_void) {
                    juiz_sdk::cabi::destroy_container(handle, #function)
                }
            },
            quote! {
                juiz_sdk::cabi::JuizRustEntry::container(
                    __juiz_cabi_manifest, __juiz_cabi_info, __juiz_cabi_create, __juiz_cabi_destroy)
            },
        ),
        CabiKind::ContainerProcess => (
            quote! {
                extern "C" fn __juiz_cabi_info(out: *const juiz_sdk::cabi::JuizBytesSink) -> i64 {
                    juiz_sdk::cabi::container_process_info(out, #function)
                }
                extern "C" fn __juiz_cabi_container_call(
                    handle: *mut ::std::ffi::c_void,
                    args: *const juiz_sdk::cabi::JuizFrameIn,
                    out: *const juiz_sdk::cabi::JuizFrameOut,
                ) -> i64 {
                    juiz_sdk::cabi::call_container_process(handle, args, out, #function)
                }
            },
            quote! {
                juiz_sdk::cabi::JuizRustEntry::container_process(
                    __juiz_cabi_manifest, __juiz_cabi_info, __juiz_cabi_container_call)
            },
        ),
        CabiKind::Component => (
            quote! {
                extern "C" fn __juiz_cabi_info(out: *const juiz_sdk::cabi::JuizBytesSink) -> i64 {
                    juiz_sdk::cabi::component_info(out)
                }
            },
            quote! {
                juiz_sdk::cabi::JuizRustEntry::component(__juiz_cabi_manifest, __juiz_cabi_info)
            },
        ),
    };
    quote! {
        /// Rust プラグインの C ABI の入口 (juiz_sdk::cabi、docs/spec/rust-plugin-abi.md §3)
        #[no_mangle]
        pub extern "C" fn #cabi_ident() -> *const juiz_sdk::cabi::JuizRustEntry {
            #common
            #functions
            static ENTRY: juiz_sdk::cabi::JuizRustEntry = #entry;
            &ENTRY
        }
    }
}
