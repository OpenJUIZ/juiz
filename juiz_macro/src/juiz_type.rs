//! `#[derive(JuizType)]` (issue #190、仕様 docs/spec/datatype.md §11)

use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Attribute, Data, DeriveInput, Expr, Fields, LitStr};

fn doc_of(attrs: &[Attribute]) -> Option<String> {
    let lines: Vec<String> = attrs
        .iter()
        .filter(|a| a.path().is_ident("doc"))
        .filter_map(|a| match &a.meta {
            syn::Meta::NameValue(nv) => match &nv.value {
                Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) => Some(s.value().trim().to_owned()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

fn doc_tokens(doc: Option<String>) -> TokenStream {
    match doc {
        Some(d) => quote! { .doc(#d) },
        None => quote! {},
    }
}

/// `#[juiz(...)]` の中身
#[derive(Default)]
struct JuizAttrs {
    namespace: Option<String>,
    default: Option<Expr>,
    is_default_symbol: bool,
}

fn parse_juiz_attrs(attrs: &[Attribute]) -> syn::Result<JuizAttrs> {
    let mut out = JuizAttrs::default();
    for attr in attrs.iter().filter(|a| a.path().is_ident("juiz")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("namespace") {
                let lit: LitStr = meta.value()?.parse()?;
                out.namespace = Some(lit.value());
                Ok(())
            } else if meta.path.is_ident("default") {
                if meta.input.peek(syn::Token![=]) {
                    out.default = Some(meta.value()?.parse()?);
                } else {
                    out.is_default_symbol = true;
                }
                Ok(())
            } else {
                Err(meta.error("unknown juiz attribute (use namespace or default)"))
            }
        })?;
    }
    Ok(out)
}

pub(crate) fn derive_juiz_type_inner(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match derive(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn derive(input: &DeriveInput) -> syn::Result<TokenStream> {
    let ident = &input.ident;
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "JuizType cannot be derived for generic types",
        ));
    }
    let attrs = parse_juiz_attrs(&input.attrs)?;
    let namespace = attrs.namespace.ok_or_else(|| {
        syn::Error::new_spanned(
            ident,
            "#[juiz(namespace = \"...\")] is required for #[derive(JuizType)]",
        )
    })?;
    let name = ident.to_string();
    let doc = doc_tokens(doc_of(&input.attrs));
    match &input.data {
        Data::Struct(data) => {
            let fields = match &data.fields {
                Fields::Named(f) => &f.named,
                _ => {
                    return Err(syn::Error::new_spanned(
                        ident,
                        "JuizType can be derived only for structs with named fields",
                    ))
                }
            };
            let mut field_defs = Vec::new();
            let mut to_values = Vec::new();
            let mut from_values = Vec::new();
            for f in fields.iter() {
                let fident = f.ident.as_ref().unwrap();
                let fname = fident.to_string();
                let ty = &f.ty;
                let fattrs = parse_juiz_attrs(&f.attrs)?;
                let fdoc = doc_tokens(doc_of(&f.attrs));
                let with_default = match fattrs.default {
                    Some(expr) => {
                        let message = format!("invalid default of {name}.{fname}");
                        quote! { .default_value(juiz_sdk::prelude::serde_json::json!(#expr)).expect(#message) }
                    }
                    None => quote! {},
                };
                field_defs.push(quote! {
                    .field(
                        juiz_sdk::types::RecordField::new(
                            #fname,
                            <#ty as juiz_sdk::types::JuizType>::data_type(),
                        )
                        #fdoc
                        #with_default
                    )
                });
                to_values.push(quote! {
                    m.insert(#fname.to_owned(), juiz_sdk::types::JuizType::to_value(&self.#fident)?);
                });
                from_values.push(quote! {
                    #fident: <#ty as juiz_sdk::types::JuizType>::from_value(&obj[#fname])?,
                });
            }
            Ok(quote! {
                impl juiz_sdk::types::JuizType for #ident {
                    fn data_type() -> juiz_sdk::types::DataType {
                        juiz_sdk::types::DataType::Record(
                            juiz_sdk::types::RecordType::new(#namespace, #name)
                                #doc
                                #(#field_defs)*
                        )
                    }
                    fn to_value(&self) -> juiz_sdk::prelude::JuizResult<juiz_sdk::prelude::Value> {
                        let mut m = juiz_sdk::prelude::serde_json::Map::new();
                        #(#to_values)*
                        Ok(juiz_sdk::prelude::Value::Object(m))
                    }
                    fn from_value(value: &juiz_sdk::prelude::Value) -> juiz_sdk::prelude::JuizResult<Self> {
                        // 足りないフィールドを既定値で埋めてから読む (仕様 §4)
                        let t = <Self as juiz_sdk::types::JuizType>::data_type();
                        let obj = juiz_sdk::types::read_value(&t, value, #name).map_err(juiz_sdk::types::type_error)?;
                        Ok(Self {
                            #(#from_values)*
                        })
                    }
                }
            })
        }
        Data::Enum(data) => {
            let mut symbols = Vec::new();
            let mut default_symbol = None;
            for v in data.variants.iter() {
                if !matches!(v.fields, Fields::Unit) {
                    return Err(syn::Error::new_spanned(
                        v,
                        "JuizType can be derived only for enums whose variants have no fields",
                    ));
                }
                let vattrs = parse_juiz_attrs(&v.attrs)?;
                if vattrs.is_default_symbol {
                    if default_symbol.is_some() {
                        return Err(syn::Error::new_spanned(
                            v,
                            "only one variant can be #[juiz(default)]",
                        ));
                    }
                    default_symbol = Some(v.ident.to_string());
                }
                symbols.push(v.ident.clone());
            }
            let symbol_strs: Vec<String> = symbols.iter().map(|s| s.to_string()).collect();
            let with_default = match default_symbol {
                Some(d) => quote! { .default_symbol(#d) },
                None => quote! {},
            };
            Ok(quote! {
                impl juiz_sdk::types::JuizType for #ident {
                    fn data_type() -> juiz_sdk::types::DataType {
                        juiz_sdk::types::DataType::Enum(
                            juiz_sdk::types::EnumType::new(#namespace, #name, &[#(#symbol_strs),*])
                                #doc
                                #with_default
                        )
                    }
                    fn to_value(&self) -> juiz_sdk::prelude::JuizResult<juiz_sdk::prelude::Value> {
                        Ok(match self {
                            #(Self::#symbols => #symbol_strs.into(),)*
                        })
                    }
                    fn from_value(value: &juiz_sdk::prelude::Value) -> juiz_sdk::prelude::JuizResult<Self> {
                        let t = <Self as juiz_sdk::types::JuizType>::data_type();
                        let v = juiz_sdk::types::read_value(&t, value, #name).map_err(juiz_sdk::types::type_error)?;
                        match v.as_str() {
                            #(Some(#symbol_strs) => Ok(Self::#symbols),)*
                            _ => unreachable!(),
                        }
                    }
                }
            })
        }
        Data::Union(_) => Err(syn::Error::new_spanned(
            ident,
            "JuizType cannot be derived for unions",
        )),
    }
}
