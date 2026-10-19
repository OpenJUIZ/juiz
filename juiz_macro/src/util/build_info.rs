use quote::{format_ident, quote};

/// ファクトリ関数 `symbol_name` の隣に置く、ビルド情報を返す関数を生成する。
///
/// juiz_core はファクトリを呼ぶ前に `<symbol_name>_build_info` を呼び、
/// プラグインをビルドしたコンパイラと juiz_sdk の版がホストと同じか確かめる
/// (juiz_sdk::build_info)。ABI に依存しないよう `extern "C"` で C 文字列を返すだけにする。
pub(crate) fn build_info_tokenstream(symbol_name: &str) -> proc_macro2::TokenStream {
    let build_info_ident = format_ident!("{}_build_info", symbol_name);
    quote! {
        #[no_mangle]
        pub extern "C" fn #build_info_ident() -> *const ::std::os::raw::c_char {
            juiz_sdk::build_info::build_info_ptr()
        }
    }
}
