use quote::{format_ident, quote};

use crate::util::{build_info_tokenstream, cabi_entry_tokenstream, CabiKind};

/// readonly (#377) なら読み出しロックで呼ぶ Factory とグルーを使う
fn kind_and_factory(readonly: bool) -> (CabiKind, proc_macro2::TokenStream) {
    if readonly {
        (
            CabiKind::ContainerProcessReadonly,
            quote! { juiz_sdk::factory::container_process_factory_readonly },
        )
    } else {
        (
            CabiKind::ContainerProcess,
            quote! { juiz_sdk::factory::container_process_factory },
        )
    }
}

pub(crate) fn factory_tokenstream(function_ident: syn::Ident, readonly: bool) -> proc_macro::TokenStream {
    let build_info = build_info_tokenstream("container_process_factory");
    let (kind, factory_fn) = kind_and_factory(readonly);
    let cabi = cabi_entry_tokenstream(
        "container_process_factory",
        kind,
        quote! { manifest2() },
        Some(&function_ident),
    );
    // まず土台となる関数定義
    quote!{
        #build_info
        #cabi

        #[no_mangle]
        pub unsafe extern "Rust" fn container_process_factory() -> JuizResult<ContainerProcessFactoryStruct> {
            let _ = env_logger::try_init();
            Ok(#factory_fn(manifest2(), #function_ident))
        }
    }.into()
}

pub(crate) fn component_factory_tokenstream(
    function_ident: syn::Ident,
    factory_name: String,
    readonly: bool,
) -> proc_macro::TokenStream {
    let factory_name_ident = format_ident!("{}", factory_name);
    let manifest_function_name_ident =
        format_ident!("{}", function_ident.to_string() + "_manifest");

    let build_info = build_info_tokenstream(&factory_name);
    let (kind, factory_fn) = kind_and_factory(readonly);
    let cabi = cabi_entry_tokenstream(
        &factory_name,
        kind,
        quote! { #manifest_function_name_ident() },
        Some(&function_ident),
    );
    // まず土台となる関数定義
    quote!{
        #build_info
        #cabi

        #[no_mangle]
        pub unsafe extern "Rust" fn #factory_name_ident() -> JuizResult<ContainerProcessFactoryStruct> {
            // env_logger::init();
            Ok(#factory_fn(#manifest_function_name_ident(), #function_ident))
        }
    }.into()
}
