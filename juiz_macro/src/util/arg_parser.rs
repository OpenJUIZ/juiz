use syn::{ItemFn, Type, TypePath};

pub(crate) fn parse_arg_map(ast: &ItemFn) -> Vec<(TypePath, syn::Ident)> {
    let mut arg_map: Vec<(TypePath, syn::Ident)> = Vec::new();
    for input in ast.sig.inputs.iter() {
        if let syn::FnArg::Typed(pat_type) = input {
            // println!("Typed: {:?}", pat_type);
            let arg_name = if let syn::Pat::Ident(arg_name) = pat_type.pat.as_ref() {
                arg_name.ident.clone()
            } else {
                panic!("引数の名前宣言が不正");
            };
            let _type_name = if let Type::Path(type_path) = pat_type.ty.as_ref() {
                arg_map.push((type_path.clone(), arg_name));
                type_path
                    .path
                    .segments
                    .iter()
                    .map(|seg| seg.ident.to_string())
                    .collect::<Vec<String>>()
                    .join("::")
            } else {
                panic!("引数の型宣言が不正です。 ({pat_type:?})");
            };
        }
    }
    arg_map
}

pub(crate) fn parse_arg_map_skip_first(ast: &ItemFn) -> Vec<(TypePath, syn::Ident)> {
    let mut arg_map: Vec<(TypePath, syn::Ident)> = Vec::new();
    let mut inputs_iter = ast.sig.inputs.iter();
    let _ = inputs_iter.next();
    for input in inputs_iter {
        if let syn::FnArg::Typed(pat_type) = input {
            //println!("Typed: {:?}", pat_type);
            let arg_name = if let syn::Pat::Ident(arg_name) = pat_type.pat.as_ref() {
                arg_name.ident.clone()
            } else {
                panic!("引数の名前宣言が不正");
            };
            let _type_name = if let Type::Path(type_path) = pat_type.ty.as_ref() {
                arg_map.push((type_path.clone(), arg_name));
                type_path
                    .path
                    .segments
                    .iter()
                    .map(|seg| seg.ident.to_string())
                    .collect::<Vec<String>>()
                    .join("::")
            } else {
                panic!("引数の型宣言が不正です。 ({pat_type:?})");
            };
        }
    }
    arg_map
}
