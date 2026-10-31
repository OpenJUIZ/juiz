use quote::{format_ident, quote};

use crate::util::{build_info_tokenstream, cabi_entry_tokenstream, CabiKind};

pub(crate) fn factory_tokenstream(function_ident: syn::Ident) -> proc_macro::TokenStream {
    let build_info = build_info_tokenstream("container_factory");
    let cabi = cabi_entry_tokenstream(
        "container_factory",
        CabiKind::Container,
        quote! { manifest2() },
        Some(&function_ident),
    );
    // まず土台となる関数定義
    quote! {
        #build_info
        #cabi

        #[no_mangle]
        pub unsafe extern "Rust" fn container_factory() -> JuizResult<ContainerFactoryStruct> {
            let _ = env_logger::try_init();
            Ok(juiz_sdk::factory::container_factory(manifest2(), #function_ident))
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
        CabiKind::Container,
        quote! { #manifest_function_name_ident() },
        Some(&function_ident),
    );
    // まず土台となる関数定義
    quote!{
        #build_info
        #cabi

        #[no_mangle]
        pub unsafe extern "Rust" fn #factory_name_ident() -> JuizResult<ContainerFactoryStruct> {
            // env_logger::init();
            Ok(juiz_sdk::factory::container_factory(#manifest_function_name_ident(), #function_ident))
        }
    }.into()
}
