//! 文書の検査 (docs/spec/design.md §5 の F4・N1〜N6・T1〜T6・W2・W3)
//!
//! F1〜F3 は読むとき ([`super::parse`]) に見る。ここでは読めた文書の名前・型・値と、言語の規則を見る。

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use juiz_idl::{
    parse_idl, ArgumentSignature, ContainerSignature, Declaration, IdlModel, LoadOptions,
    OutputKind, ProcessSignature, Signatures, TypeExpr,
};
use juiz_types::{check_value, is_valid_type_name, DataType, TypeSchema};
use serde_json::Value;

use super::{ArgumentDesign, Design, Diag, ProcessDesign, Report};
use crate::idl::IdlPlan;
use crate::{names, Lang};

/// 型の式を書く所 (§4.1 の使えない型が違う)
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Argument,
    Returns,
    ContainerArgument,
    State,
}

fn lang_label(lang: Lang) -> &'static str {
    match lang {
        Lang::Rust => "Rust",
        Lang::Python => "Python",
        Lang::Cpp => "C++",
        Lang::Haskell => "Haskell",
        Lang::Zig => "Zig",
    }
}

fn contains_image(t: &DataType) -> bool {
    match t {
        DataType::Image => true,
        DataType::Array(i) | DataType::Map(i) => contains_image(i),
        DataType::Optional { inner, .. } => contains_image(inner),
        _ => false,
    }
}

struct Checker<'a> {
    design: &'a Design,
    report: Report,
    /// `idl:` を読んだもの。読めなかったら None (名前付き型の解決を飛ばす)
    model: Option<IdlModel>,
    /// 型の式が 1 つでも読めなかった (言語の規則の検査を飛ばす)
    type_failed: bool,
}

impl<'a> Checker<'a> {
    fn error(&mut self, path: impl Into<String>, message: impl Into<String>) {
        let path = path.into();
        self.report.errors.push(Diag {
            pos: self.design.position(&path),
            path,
            message: message.into(),
        });
    }

    fn warning(&mut self, path: impl Into<String>, message: impl Into<String>) {
        let path = path.into();
        self.report.warnings.push(Diag {
            pos: self.design.position(&path),
            path,
            message: message.into(),
        });
    }

    // ---- 型 (T1・T2) ----

    /// 型の式を読む。読めない・使えない型ならエラーを出して None
    fn resolve(&mut self, expr: &str, path: &str, role: Role) -> Option<DataType> {
        match self.resolve_inner(expr, role) {
            Ok(Some(t)) => Some(t),
            Ok(None) => {
                // idl: が読めず、名前付き型を解決できない (エラーは idl: に出してある)
                self.type_failed = true;
                None
            }
            Err(m) => {
                self.type_failed = true;
                self.error(path, m);
                None
            }
        }
    }

    fn resolve_inner(&self, expr: &str, role: Role) -> Result<Option<DataType>, String> {
        let bad = |m: &str| format!("cannot read the type '{expr}': {m}");
        // JIDL の process の戻り値として読む (型の書き方は JIDL と同じ、§4.1)
        let source = format!("namespace juiz_design;\nprocess {expr} p();\n");
        let file = parse_idl(&source, "type").map_err(|e| match e {
            juiz_idl::IdlError::Syntax { message, .. } | juiz_idl::IdlError::Semantic { message, .. } => {
                bad(&message)
            }
            other => bad(&other.to_string()),
        })?;
        let type_expr = match file.declarations.as_slice() {
            [Declaration::Process(p)] if p.name == "p" && p.params.is_empty() && file.imports.is_empty() => {
                p.return_type.clone()
            }
            _ => return Err(bad("it is not one type")),
        };
        if type_expr == TypeExpr::Void && role != Role::Returns {
            return Err("void can be used only for returns".to_owned());
        }
        let Some(t) = self.data_type(&type_expr)? else {
            return Ok(None);
        };
        // T2: 使えない型
        match role {
            Role::Argument | Role::Returns => {
                if t == DataType::Null && role == Role::Argument {
                    return Err("null alone cannot be the type of an argument".to_owned());
                }
                if t != DataType::Image && contains_image(&t) {
                    return Err("image can be used only as the type of an argument or a return value itself (not in image?, array<image> and so on)".to_owned());
                }
            }
            Role::ContainerArgument | Role::State => {
                if t == DataType::Null {
                    return Err("null alone cannot be the type".to_owned());
                }
                if contains_image(&t) {
                    return Err(if role == Role::State {
                        "image cannot be the type of a state (it has no zero value)".to_owned()
                    } else {
                        "image cannot be the type of a container argument (the values are written in the conf)".to_owned()
                    });
                }
            }
        }
        TypeSchema::from_data_type(&t).map_err(|e| bad(&e.to_string()))?;
        Ok(Some(t))
    }

    /// 解決する前の型を型にする。名前付き型を解決できない (idl: が読めなかった) ときは None
    fn data_type(&self, t: &TypeExpr) -> Result<Option<DataType>, String> {
        Ok(Some(match t {
            TypeExpr::Primitive(p) => match p.as_str() {
                "null" => DataType::Null,
                "boolean" => DataType::Boolean,
                "int" => DataType::Int,
                "long" => DataType::Long,
                "float" => DataType::Float,
                "double" => DataType::Double,
                "string" => DataType::String,
                "bytes" => DataType::Bytes,
                other => return Err(format!("unknown type '{other}'")),
            },
            TypeExpr::Void => DataType::Null,
            TypeExpr::Any => DataType::Any,
            TypeExpr::Image => DataType::Image,
            TypeExpr::Array(i) => match self.data_type(i)? {
                Some(i) => DataType::array(i),
                None => return Ok(None),
            },
            TypeExpr::Map(i) => match self.data_type(i)? {
                Some(i) => DataType::map(i),
                None => return Ok(None),
            },
            TypeExpr::Optional { inner, null_first } => match self.data_type(inner)? {
                Some(DataType::Null) => return Err("null cannot be optional".to_owned()),
                Some(i) => DataType::Optional {
                    inner: Box::new(i),
                    null_first: null_first.unwrap_or(true),
                },
                None => return Ok(None),
            },
            TypeExpr::Named(name) => {
                let Some(model) = &self.model else {
                    return Ok(None);
                };
                if let Some(t) = model.get_type(name) {
                    return Ok(Some(t.clone()));
                }
                let short: Vec<String> = model
                    .types
                    .iter()
                    .filter(|t| match t {
                        DataType::Record(r) => r.name == *name,
                        DataType::Enum(e) => e.name == *name,
                        _ => false,
                    })
                    .filter_map(|t| t.full_name())
                    .collect();
                return Err(if !short.is_empty() {
                    format!(
                        "unknown type '{name}'; write the full name ({})",
                        short.join(" or ")
                    )
                } else if self.design.idl.is_empty() {
                    format!("unknown type '{name}'; record and enum types are read from the .jidl files in idl:")
                } else {
                    format!("unknown type '{name}' (it is not in the .jidl files in idl:)")
                });
            }
        }))
    }

    // ---- 値 (T3) ----

    /// 既定値・初期値を型で検査する。Some(値) は「既定値あり」
    fn check_default(&mut self, t: &DataType, d: &Option<Value>, path: &str) {
        let Some(d) = d else { return };
        if d.is_null() && !t.is_nullable() {
            // 既定値なし (§4.2)
            return;
        }
        if *t == DataType::Image {
            self.warning(path, "an image argument cannot have a default; it is ignored");
            return;
        }
        if let Err(e) = check_value(t, d) {
            self.error(path, format!("the value does not match the type {}: {e}", t.display_name()));
        }
    }

    // ---- 名前 (N1〜N5) ----

    fn check_type_name(&mut self, name: &str, path: &str) {
        if !is_valid_type_name(name) || !name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') {
            self.error(path, format!("'{name}' is not a valid type name ([A-Za-z_][A-Za-z0-9_]*)"));
        } else if let Err(e) = juiz_sdk::identifier::validate_name(name) {
            self.error(path, e.to_string());
        }
    }

    fn check_field_name(&mut self, name: &str, path: &str, what: &str) {
        let ok = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !ok {
            self.error(path, format!("'{name}' is not a valid {what} name ([A-Za-z_][A-Za-z0-9_]*)"));
        }
    }

    fn arguments(&mut self, args: &[ArgumentDesign], path: &str, role: Role) -> Vec<Option<DataType>> {
        let mut seen: Vec<&str> = Vec::new();
        let mut types = Vec::new();
        for (i, a) in args.iter().enumerate() {
            let p = format!("{path}[{i}]");
            self.check_field_name(&a.name, &format!("{p}.name"), "argument");
            if seen.contains(&a.name.as_str()) {
                self.error(format!("{p}.name"), format!("the argument '{}' is written twice", a.name));
            }
            seen.push(&a.name);
            let t = self.resolve(&a.type_expr, &format!("{p}.type"), role);
            if let Some(t) = &t {
                self.check_default(t, &a.default, &format!("{p}.default"));
            }
            types.push(t);
        }
        types
    }

    fn process(&mut self, p: &ProcessDesign, path: &str) -> Option<ProcessSignature> {
        self.check_type_name(&p.name, &format!("{path}.name"));
        let types = self.arguments(&p.arguments, &format!("{path}.arguments"), Role::Argument);
        let output = match &p.returns {
            None => Some(DataType::Null),
            Some(r) => self.resolve(r, &format!("{path}.returns"), Role::Returns),
        };
        // 言語の規則の検査に使うシグネチャ
        let mut arguments = Vec::new();
        for (a, t) in p.arguments.iter().zip(types) {
            let t = t?;
            arguments.push(ArgumentSignature {
                name: a.name.clone(),
                type_name: t.argument_type_name().to_owned(),
                description: a.description.clone(),
                schema: TypeSchema::from_data_type(&t).ok()?,
                default: a.default.clone().filter(|d| !(d.is_null() && !t.is_nullable())),
            });
        }
        Some(ProcessSignature {
            type_name: p.name.clone(),
            description: p.description.clone(),
            arguments,
            output_schema: TypeSchema::from_data_type(&output?).ok()?,
        })
    }

    // ---- idl: (T5) ----

    fn load_idl(&mut self, base: &Path) {
        if self.design.idl.is_empty() {
            self.model = Some(IdlModel::default());
            return;
        }
        let mut sources: Vec<PathBuf> = Vec::new();
        let mut ok = true;
        for (i, p) in self.design.idl.iter().enumerate() {
            let path = format!("idl[{i}]");
            let rel = Path::new(p);
            if p.contains('\\') {
                self.error(&path, format!("'{p}': use / as the separator"));
                ok = false;
            } else if rel.is_absolute() || p.starts_with('/') || rel.components().any(|c| !matches!(c, Component::Normal(_) | Component::CurDir)) {
                self.error(&path, format!("'{p}': write a path relative to the directory of the document, without '..' (the .jidl files must be in that directory)"));
                ok = false;
            } else if p.ends_with(".avsc") {
                self.error(&path, format!("'{p}': a .avsc cannot be written in idl:; import it from a .jidl with import schema"));
                ok = false;
            } else if !p.ends_with(".jidl") {
                self.error(&path, format!("'{p}' is not a .jidl file"));
                ok = false;
            } else {
                let file = base.join(rel);
                if file.is_file() {
                    sources.push(file);
                } else if juiz_idl::builtin_file(p).is_some() {
                    sources.push(PathBuf::from(p));
                } else {
                    self.error(&path, format!("'{p}' is not found (relative to the directory of the document, or a built-in file such as juiz_std/geometry.jidl)"));
                    ok = false;
                }
            }
        }
        if !ok {
            return;
        }
        let model = match juiz_idl::load_files_with(&sources, &LoadOptions::default()) {
            Ok(m) => m,
            Err(e) => {
                self.error("idl", e.to_string());
                return;
            }
        };
        // 組み込みでない .jidl と import するものは、文書のディレクトリの中に無ければならない (§4.3)
        let base_abs = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
        for f in &model.files {
            let abs = f.canonicalize().unwrap_or_else(|_| f.clone());
            if !abs.starts_with(&base_abs) {
                self.error(
                    "idl",
                    format!(
                        "{} is outside the directory of the document ({}); put the .jidl files and the files they import in that directory",
                        f.display(),
                        base_abs.display()
                    ),
                );
                ok = false;
            }
        }
        if ok {
            self.model = Some(model);
        }
    }

    // ---- 言語の規則 (N6・T6) ----

    fn idl_plan(&self, lang: Lang, signatures: Signatures, hs_modules: &[String]) -> IdlPlan {
        let types = self.model.as_ref().map(|m| m.types.clone()).unwrap_or_default();
        let uses_juiz_std = types.iter().any(|t| {
            t.full_name()
                .is_some_and(|n| n.starts_with("juiz_std.") && matches!(t, DataType::Record(_) | DataType::Enum(_)))
        });
        IdlPlan {
            lang,
            generated: Vec::new(),
            hs_modules: hs_modules.to_vec(),
            files: Vec::new(),
            main: super::FILE_NAME.to_owned(),
            signatures,
            uses_juiz_std,
            types,
        }
    }

    fn try_skeleton(&self, lang: Lang, s: Signatures, hs_modules: &[String]) -> Result<(), String> {
        self.idl_plan(lang, s, hs_modules)
            .source(&self.design.component.name)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn language_rules(&mut self, lang: Lang, processes: Vec<ProcessSignature>, containers: Vec<ContainerSignature>) {
        let name = self.design.component.name.clone();
        let label = lang_label(lang);
        let sigs = |processes: Vec<ProcessSignature>, containers: Vec<ContainerSignature>| Signatures {
            namespace: name.clone(),
            processes,
            containers,
        };
        // .jidl の型をその言語で作れるか (名前空間のキーワードなど)。Haskell は型のモジュール名も見る
        let mut hs_modules = Vec::new();
        if let Some(model) = &self.model {
            if !model.types.is_empty() {
                let kind = match lang {
                    Lang::Rust => OutputKind::Rust,
                    Lang::Python => OutputKind::Python,
                    Lang::Cpp => OutputKind::Cpp,
                    Lang::Haskell => OutputKind::Haskell,
                    Lang::Zig => OutputKind::Zig,
                };
                match juiz_idl::generate(model, &[kind]) {
                    Ok(files) if lang == Lang::Haskell => {
                        for f in files {
                            let p = f.path.to_string_lossy().replace('\\', "/");
                            if let Some(m) = p.strip_suffix(".hs") {
                                hs_modules.push(m.replace('/', "."));
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) => self.error("idl", format!("the types cannot be made in {label}: {e}")),
                }
            }
        }
        if lang == Lang::Haskell {
            let module = names::camel(&name);
            if hs_modules.contains(&module) {
                self.error(
                    "idl",
                    format!("the Haskell module of a namespace of the types is {module}, the same as the module of the component; use another component name or namespace"),
                );
                hs_modules.retain(|m| *m != module);
            }
        }
        for (i, p) in processes.into_iter().enumerate() {
            if let Err(e) = self.try_skeleton(lang, sigs(vec![p], vec![]), &hs_modules) {
                self.error(format!("processes[{i}]"), format!("cannot be made in {label}: {e}"));
            }
        }
        for (i, c) in containers.into_iter().enumerate() {
            let empty = ContainerSignature {
                container_processes: Vec::new(),
                ..c.clone()
            };
            if let Err(e) = self.try_skeleton(lang, sigs(vec![], vec![empty.clone()]), &hs_modules) {
                self.error(format!("containers[{i}]"), format!("cannot be made in {label}: {e}"));
                continue;
            }
            for (j, p) in c.container_processes.into_iter().enumerate() {
                let one = ContainerSignature {
                    container_processes: vec![p],
                    ..empty.clone()
                };
                if let Err(e) = self.try_skeleton(lang, sigs(vec![], vec![one]), &hs_modules) {
                    self.error(
                        format!("containers[{i}].processes[{j}]"),
                        format!("cannot be made in {label}: {e}"),
                    );
                }
            }
        }
        // Container の引数と状態の名前 (その言語のキーワードなど)。名前 1 つの引数の関数を作って確かめる
        let design = self.design;
        for (i, c) in design.containers.iter().enumerate() {
            let fields = c
                .arguments
                .iter()
                .enumerate()
                .map(|(j, a)| (format!("containers[{i}].arguments[{j}].name"), &a.name))
                .chain(
                    c.state
                        .iter()
                        .enumerate()
                        .map(|(j, s)| (format!("containers[{i}].state[{j}].name"), &s.name)),
                );
            for (path, field) in fields {
                let probe = ProcessSignature {
                    type_name: "juiz_design_probe".to_owned(),
                    description: None,
                    arguments: vec![ArgumentSignature {
                        name: field.clone(),
                        type_name: "Int".to_owned(),
                        description: None,
                        schema: TypeSchema::from_data_type(&DataType::Long).expect("long"),
                        default: None,
                    }],
                    output_schema: TypeSchema::from_data_type(&DataType::Long).expect("long"),
                };
                if self.try_skeleton(lang, sigs(vec![probe], vec![]), &hs_modules).is_err() {
                    self.error(path, format!("'{field}' cannot be used as a name in {label} (a keyword or a reserved name)"));
                }
            }
        }
    }
}

/// 文書を検査する (§5)。`lang` は決まっている言語 (§8.1)、`base` は文書のあるディレクトリ (`idl:` の基準)
pub fn check(design: &Design, lang: Option<Lang>, base: &Path) -> Report {
    let mut c = Checker {
        design,
        report: Report::default(),
        model: None,
        type_failed: false,
    };
    // F4
    if design.processes.is_empty() && design.containers.is_empty() {
        c.error("", "the document has no type; write processes or containers");
    }
    // N1
    let name = &design.component.name;
    if let Err(e) = names::validate(name, lang.unwrap_or(Lang::Rust)) {
        c.error("component.name", e.to_string());
    } else if lang == Some(Lang::Zig) && name.len() > names::ZIG_MAX_NAME_LEN {
        c.error(
            "component.name",
            format!(
                "for Zig, the component name must be at most {} characters (the .name of build.zig.zon)",
                names::ZIG_MAX_NAME_LEN
            ),
        );
    }
    // T5
    c.load_idl(base);

    // 型 (N2・N5・T1〜T4)
    let mut processes = Vec::new();
    for (i, p) in design.processes.iter().enumerate() {
        processes.push(c.process(p, &format!("processes[{i}]")));
    }
    let mut containers = Vec::new();
    for (i, k) in design.containers.iter().enumerate() {
        let path = format!("containers[{i}]");
        c.check_type_name(&k.name, &format!("{path}.name"));
        let arg_types = c.arguments(&k.arguments, &format!("{path}.arguments"), Role::ContainerArgument);
        let mut seen: Vec<&str> = Vec::new();
        for (j, s) in k.state.iter().enumerate() {
            let sp = format!("{path}.state[{j}]");
            c.check_field_name(&s.name, &format!("{sp}.name"), "state");
            if seen.contains(&s.name.as_str()) {
                c.error(format!("{sp}.name"), format!("the state '{}' is written twice", s.name));
            }
            seen.push(&s.name);
            let t = c.resolve(&s.type_expr, &format!("{sp}.type"), Role::State);
            // T4
            if s.init.is_some() && s.default.is_some() {
                c.error(format!("{sp}.init"), "write init or default, not both");
            }
            if let Some(init) = &s.init {
                match k.arguments.iter().position(|a| a.name == *init) {
                    None => c.error(
                        format!("{sp}.init"),
                        format!("'{init}' is not an argument of the container '{}'", k.name),
                    ),
                    Some(ai) => {
                        if let (Some(st), Some(Some(at))) = (&t, arg_types.get(ai)) {
                            let ok = st == at
                                || matches!(st, DataType::Optional { inner, .. } if **inner == *at);
                            if !ok {
                                c.error(
                                    format!("{sp}.init"),
                                    format!(
                                        "the argument '{init}' is {}, but the state '{}' is {}",
                                        at.display_name(),
                                        s.name,
                                        st.display_name()
                                    ),
                                );
                            }
                        }
                    }
                }
            }
            if let Some(t) = &t {
                c.check_default(t, &s.default, &format!("{sp}.default"));
            }
        }
        let mut cps = Vec::new();
        let mut ok = true;
        for (j, p) in k.processes.iter().enumerate() {
            match c.process(p, &format!("{path}.processes[{j}]")) {
                Some(s) => cps.push(s),
                None => ok = false,
            }
        }
        containers.push(ok.then(|| ContainerSignature {
            type_name: k.name.clone(),
            description: k.description.clone(),
            container_processes: cps,
        }));
    }

    // N3・N4: 型名と、型名から作る識別子が重ならない
    let mut types: Vec<(String, String, bool)> = Vec::new(); // (道筋, 型名, Container か)
    for (i, p) in design.processes.iter().enumerate() {
        types.push((format!("processes[{i}].name"), p.name.clone(), false));
    }
    for (i, k) in design.containers.iter().enumerate() {
        types.push((format!("containers[{i}].name"), k.name.clone(), true));
        for (j, p) in k.processes.iter().enumerate() {
            types.push((format!("containers[{i}].processes[{j}].name"), p.name.clone(), false));
        }
    }
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut idents: HashMap<String, usize> = HashMap::new();
    for (i, (path, name, is_container)) in types.iter().enumerate() {
        if let Some(&j) = seen.get(name) {
            c.error(path.clone(), format!("the type name '{name}' is already used by {}", types[j].0.trim_end_matches(".name")));
            continue;
        }
        seen.insert(name.clone(), i);
        let mut mine: Vec<String> = ["", "_manifest", "_manif", "_create", "_entry_point", "_manifest_entry_point"]
            .iter()
            .map(|s| format!("{name}{s}"))
            .collect();
        if *is_container {
            let camel = names::camel(name);
            mine.push(format!("{camel}State"));
            mine.push(camel);
        }
        for ident in mine {
            match idents.get(&ident) {
                Some(&j) if j != i => {
                    c.error(
                        path.clone(),
                        format!(
                            "the identifier '{ident}' made from the type '{name}' clashes with the one made from the type '{}' ({})",
                            types[j].1,
                            types[j].0.trim_end_matches(".name")
                        ),
                    );
                    break;
                }
                _ => {
                    idents.insert(ident, i);
                }
            }
        }
    }

    // N6・T6 (ほかにエラーが無いときだけ。型が読めないと生成できないので)
    match lang {
        Some(l) => {
            if c.report.errors.is_empty() && !c.type_failed {
                let processes: Option<Vec<_>> = processes.into_iter().collect();
                let containers: Option<Vec<_>> = containers.into_iter().collect();
                if let (Some(p), Some(k)) = (processes, containers) {
                    c.language_rules(l, p, k);
                }
            }
        }
        None => c.warning(
            "",
            "the language is not decided, so the rules of each language (keywords and types the language cannot use) were not checked; give -b <lang> or write component.language",
        ),
    }
    c.report
}
