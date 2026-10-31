use quote::{format_ident, quote};

use crate::util::{build_info_tokenstream, cabi_entry_tokenstream, CabiKind};

pub(crate) fn factory_tokenstream(function_ident: syn::Ident) -> proc_macro::TokenStream {
    let build_info = build_info_tokenstream("process_factory");
    let cabi = cabi_entry_tokenstream(
        "process_factory",
        CabiKind::Process,
        quote! { manifest() },
        Some(&function_ident),
    );
    // まず土台となる関数定義
    quote! {
        // use juiz_sdk::factory::process_factory;

        #build_info
        #cabi

        #[no_mangle]
        pub unsafe extern "Rust" fn process_factory() -> JuizResult<ProcessFactoryStruct> {
            let _ = env_logger::try_init();
            Ok(juiz_sdk::prelude::process_factory(manifest(), #function_ident))
        }
    }
    .into()
}

pub(crate) fn component_factory_tokenstream(
    function_ident: syn::Ident,
    factory_name: String,
) -> proc_macro::TokenStream {
    let factory_name_ident = format_ident!("{}", factory_name);
    let manifest_function_name_ident =
        format_ident!("{}", function_ident.to_string() + "_manifest");
    let build_info = build_info_tokenstream(&factory_name);
    let cabi = cabi_entry_tokenstream(
        &factory_name,
        CabiKind::Process,
        quote! { #manifest_function_name_ident() },
        Some(&function_ident),
    );
    // まず土台となる関数定義
    quote! {
        // use juiz_sdk::factory::process_factory;

        #build_info
        #cabi

        #[no_mangle]
        pub unsafe extern "Rust" fn #factory_name_ident() -> JuizResult<ProcessFactoryStruct> {
            let _ = env_logger::try_init();
            Ok(juiz_sdk::prelude::process_factory(#manifest_function_name_ident(), #function_ident))
        }
    }
    .into()
}
