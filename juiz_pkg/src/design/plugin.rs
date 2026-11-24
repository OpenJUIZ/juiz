//! プラグインとの突き合わせ (`juiz pkg design check --plugin`) と、プラグインから文書を起こす
//! (`juiz pkg design --from-plugin`) (docs/spec/design.md §6.1・§8.5・§8.6)
//!
//! プラグインを開くのは juiz (juiz_core) の仕事なので、ここでは読んだ ComponentManifest だけを扱う。
//! 開く関数は [`PluginLoader`] として渡してもらう (juiz-pkg 単体では開けない)。

use std::path::Path;

use juiz_idl::{collect_named, type_expression, ProcessSignature, Signatures};
use juiz_sdk::prelude::{ArgumentManifest, ComponentManifest, ContainerManifest, ProcessManifest};
use juiz_types::{DataType, TypeSchema};
use serde_json::Value;

use super::{
    ArgumentDesign, ComponentDesign, ContainerDesign, Design, Diag, ProcessDesign, Report,
};
use crate::{Lang, PkgError};

/// プラグインのファイルを開いて ComponentManifest を読む関数。juiz が juiz_core で開くものを渡す
pub type PluginLoader<'a> = &'a dyn Fn(&Path, Lang) -> Result<ComponentManifest, String>;

/// juiz-pkg 単体 (juiz_core を持たない) の [`PluginLoader`]。いつもエラー
pub fn no_loader(path: &Path, _lang: Lang) -> Result<ComponentManifest, String> {
    Err(format!(
        "juiz-pkg cannot open plugins ({}); run the same command with juiz: juiz pkg …",
        path.display()
    ))
}

/// プラグインの言語 (§8.6)。`lang` (-b、check では文書の component.language も) が無ければ、
/// 拡張子 .py を Python とみなす。ほかの拡張子 (.so・.dylib・.dll) は言語を決められないのでエラー
pub fn plugin_lang(path: &Path, lang: Option<Lang>) -> Result<Lang, PkgError> {
    if let Some(l) = lang {
        return Ok(l);
    }
    if path.extension().is_some_and(|e| e == "py") {
        return Ok(Lang::Python);
    }
    Err(PkgError::new(format!(
        "cannot tell the language of the plugin {}; give -b <rust|cpp|haskell|zig>",
        path.display()
    )))
}

/// プラグインを開いて ComponentManifest を読む
pub fn load(loader: PluginLoader, path: &Path, lang: Lang) -> Result<ComponentManifest, PkgError> {
    loader(path, lang).map_err(|e| PkgError::new(format!("cannot read the plugin {}: {e}", path.display())))
}

/// manifest の language (`c++` など) の言語
fn manifest_lang(s: &str) -> Option<Lang> {
    match s.to_ascii_lowercase().as_str() {
        "rust" => Some(Lang::Rust),
        "python" => Some(Lang::Python),
        "c++" | "cpp" => Some(Lang::Cpp),
        "haskell" => Some(Lang::Haskell),
        "zig" => Some(Lang::Zig),
        _ => None,
    }
}

fn text(d: &juiz_sdk::prelude::Description) -> Option<String> {
    Some(d.text.clone()).filter(|t| !t.is_empty())
}

/// 値が同じか。数は値で比べる (`2` と `2.0` は同じ)
fn same_value(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same_value(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same_value(v, w)))
        }
        _ => a == b,
    }
}

fn show_value(v: &Option<Value>) -> String {
    match v {
        None => "none".to_owned(),
        Some(v) => serde_json::to_string(v).unwrap_or_default(),
    }
}

/// 型参照を文書の型の式で表す (読めなければ表示名)
fn show_schema(s: &TypeSchema) -> String {
    match s.data_type() {
        Ok(DataType::Null) => "void".to_owned(),
        Ok(t) => type_expression(&t),
        Err(_) => s.name.clone(),
    }
}

// ---- 突き合わせ (§8.5) ----

struct Matcher<'a> {
    design: &'a Design,
    report: Report,
}

impl Matcher<'_> {
    fn push(&mut self, error: bool, path: String, message: String) {
        let d = Diag {
            pos: if path.is_empty() { None } else { self.design.position(&path) },
            path,
            message,
        };
        if error {
            self.report.errors.push(d);
        } else {
            self.report.warnings.push(d);
        }
    }

    fn error(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.push(true, path.into(), message.into());
    }

    fn warning(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.push(false, path.into(), message.into());
    }

    /// P6: 説明 (違えば警告)
    fn description(&mut self, doc: &Option<String>, plugin: &juiz_sdk::prelude::Description, path: &str, what: &str) {
        let p = text(plugin);
        if *doc != p {
            self.warning(
                format!("{path}.description"),
                format!(
                    "the description of {what} differs from the plugin's ({})",
                    p.map(|t| format!("{t:?}")).unwrap_or_else(|| "none".to_owned())
                ),
            );
        }
    }

    /// P3: 引数の並び (Process・ContainerProcess・Container)
    fn arguments(
        &mut self,
        doc: &[ArgumentDesign],
        sig: &[juiz_idl::ArgumentSignature],
        plugin: &[ArgumentManifest],
        path: &str,
        what: &str,
    ) {
        for (i, (a, s)) in doc.iter().zip(sig).enumerate() {
            let ap = format!("{path}.arguments[{i}]");
            let Some(m) = plugin.iter().find(|m| m.name == a.name) else {
                self.error(ap, format!("the plugin's {what} has no argument '{}'", a.name));
                continue;
            };
            let is_image = s.schema.data_type().is_ok_and(|t| t == DataType::Image);
            match &m.schema {
                Some(ms) => {
                    if ms.fingerprint != s.schema.fingerprint || ms.name != s.schema.name {
                        let (mine, theirs) = (show_schema(&s.schema), show_schema(ms));
                        let same = if mine == theirs { " (the same name, but the definitions differ)" } else { "" };
                        self.error(
                            format!("{ap}.type"),
                            format!("the type is {mine}, but the plugin's is {theirs}{same}"),
                        );
                    }
                }
                None => {
                    if m.type_name.as_str() == s.type_name {
                        self.warning(
                            format!("{ap}.type"),
                            format!(
                                "the plugin's argument has no type (schema), so only its type_name {} was compared",
                                s.type_name
                            ),
                        );
                    } else {
                        self.error(
                            format!("{ap}.type"),
                            format!(
                                "the type is {} (type_name {}), but the plugin's argument has type_name {} (and no schema)",
                                show_schema(&s.schema),
                                s.type_name,
                                m.type_name.as_str()
                            ),
                        );
                    }
                }
            }
            if !is_image {
                let theirs = Some(m.default.clone()).filter(|v| !v.is_null());
                let mine = s.default.clone().filter(|v| !v.is_null());
                let same = match (&mine, &theirs) {
                    (Some(x), Some(y)) => same_value(x, y),
                    (None, None) => true,
                    _ => false,
                };
                if !same {
                    self.error(
                        format!("{ap}.default"),
                        format!(
                            "the default is {}, but the plugin's is {}",
                            show_value(&mine),
                            show_value(&theirs)
                        ),
                    );
                }
            }
            self.description(&a.description, &m.description, &ap, &format!("the argument '{}'", a.name));
        }
        for m in plugin.iter().filter(|m| !doc.iter().any(|a| a.name == m.name)) {
            self.error(
                format!("{path}.arguments"),
                format!("the plugin's {what} has the argument '{}', which is not in the document", m.name),
            );
        }
        // 名前がそろっていて順番だけが違う
        let names = |v: Vec<&str>| v.join(", ");
        let doc_names: Vec<&str> = doc.iter().map(|a| a.name.as_str()).collect();
        let plugin_names: Vec<&str> = plugin.iter().map(|m| m.name.as_str()).collect();
        if doc_names.len() == plugin_names.len()
            && doc_names.iter().all(|n| plugin_names.contains(n))
            && doc_names != plugin_names
        {
            self.warning(
                format!("{path}.arguments"),
                format!("the order of the arguments differs from the plugin's ({})", names(plugin_names)),
            );
        }
    }

    /// P3〜P5: Process・ContainerProcess 1 つ
    fn process(&mut self, d: &ProcessDesign, s: &ProcessSignature, m: &ProcessManifest, path: &str, what: &str) {
        let label = format!("{what} '{}'", d.name);
        self.arguments(&d.arguments, &s.arguments, &m.arguments, path, &label);
        // P4
        match &m.output_schema {
            Some(ms) => {
                if ms.fingerprint != s.output_schema.fingerprint || ms.name != s.output_schema.name {
                    let (mine, theirs) = (show_schema(&s.output_schema), show_schema(ms));
                    let same = if mine == theirs { " (the same name, but the definitions differ)" } else { "" };
                    self.error(
                        format!("{path}.returns"),
                        format!("the return type is {mine}, but the plugin's is {theirs}{same}"),
                    );
                }
            }
            None => self.warning(
                if d.returns.is_some() { format!("{path}.returns") } else { path.to_owned() },
                format!(
                    "the plugin's {label} has no type for the return value (output_schema), so it was not compared"
                ),
            ),
        }
        // P5
        if what == "container process" && d.readonly != m.readonly {
            self.error(
                format!("{path}.readonly"),
                format!(
                    "readonly is {}, but the plugin's is {}",
                    d.readonly, m.readonly
                ),
            );
        }
        self.description(&d.description, &m.description, path, &format!("the {label}"));
        topics(m, path, &label, |p, msg| self.warning(p, msg));
    }
}

/// manifest にあって文書に書けない Topic (§11) の警告
fn topics(m: &ProcessManifest, path: &str, label: &str, mut warn: impl FnMut(String, String)) {
    if !m.publishes.is_empty() || !m.subscribes.is_empty() {
        warn(
            path.to_owned(),
            format!(
                "the plugin's {label} publishes or subscribes Topics, which a design document cannot write yet (design.md §11)"
            ),
        );
    }
}

/// 文書 (検査を通ったもの、`sigs` はその型) とプラグインの manifest を §6.1 の表で突き合わせる (§8.5 の P1〜P7)
pub fn compare(design: &Design, sigs: &Signatures, m: &ComponentManifest) -> Report {
    let mut c = Matcher {
        design,
        report: Report::default(),
    };
    // P1
    if design.component.name != m.type_name {
        c.error(
            "component.name",
            format!("the component name is '{}', but the plugin's is '{}'", design.component.name, m.type_name),
        );
    }
    // P7
    if let Some(l) = design.component.language {
        if manifest_lang(&m.language).is_some_and(|ml| ml != l) {
            c.warning(
                "component.language",
                format!("the language is {}, but the plugin's is {}", l.plugin_language(), m.language),
            );
        }
    }
    // P6 (Rust のマクロは Component の説明を持てないので、プラグインが空なら比べない、§8.2)
    if !m.description.text.is_empty() {
        c.description(&design.component.description, &m.description, "component", "the component");
    }
    // P2: Process
    for (i, (d, s)) in design.processes.iter().zip(&sigs.processes).enumerate() {
        let path = format!("processes[{i}]");
        match m.processes.iter().find(|p| p.type_name == d.name) {
            Some(p) => c.process(d, s, p, &path, "process"),
            None => c.error(path, format!("the plugin has no process '{}'", d.name)),
        }
    }
    for p in m.processes.iter().filter(|p| !design.processes.iter().any(|d| d.name == p.type_name)) {
        c.error("", format!("the plugin has the process '{}', which is not in the document", p.type_name));
    }
    // P2: Container と ContainerProcess
    for (i, (d, s)) in design.containers.iter().zip(&sigs.containers).enumerate() {
        let path = format!("containers[{i}]");
        let Some(k) = m.containers.iter().find(|k| k.type_name == d.name) else {
            c.error(path, format!("the plugin has no container '{}'", d.name));
            continue;
        };
        container(&mut c, d, s, k, &path);
    }
    for k in m.containers.iter().filter(|k| !design.containers.iter().any(|d| d.name == k.type_name)) {
        c.error("", format!("the plugin has the container '{}', which is not in the document", k.type_name));
    }
    c.report
}

fn container(
    c: &mut Matcher,
    d: &ContainerDesign,
    s: &juiz_idl::ContainerSignature,
    k: &ContainerManifest,
    path: &str,
) {
    let label = format!("container '{}'", d.name);
    if cpp_without_arguments(k) {
        if !d.arguments.is_empty() {
            c.warning(format!("{path}.arguments"), CPP_CONTAINER_ARGUMENTS.to_owned());
        }
    } else {
        c.arguments(&d.arguments, &s.arguments, &k.arguments, path, &label);
    }
    c.description(&d.description, &k.description, path, &format!("the {label}"));
    for (j, (p, ps)) in d.processes.iter().zip(&s.container_processes).enumerate() {
        let pp = format!("{path}.processes[{j}]");
        match k.processes.iter().find(|q| q.type_name == p.name) {
            Some(q) => c.process(p, ps, q, &pp, "container process"),
            None => c.error(pp, format!("the plugin's {label} has no container process '{}'", p.name)),
        }
    }
    for q in k.processes.iter().filter(|q| !d.processes.iter().any(|p| p.name == q.type_name)) {
        c.error(
            format!("{path}.processes"),
            format!("the plugin's {label} has the container process '{}', which is not in the document", q.type_name),
        );
    }
}

/// C++ の SDK (cppjuiz) の ContainerManifest は引数を持てないので、C++ の Container の引数は manifest に出ない (§8.5)
const CPP_CONTAINER_ARGUMENTS: &str =
    "the C++ SDK does not put the arguments of a container in its manifest, so they were not compared";

fn cpp_without_arguments(k: &ContainerManifest) -> bool {
    manifest_lang(&k.language) == Some(Lang::Cpp) && k.arguments.is_empty()
}

// ---- プラグインから文書を起こす (§8.6) ----

/// [`from_manifest`] の結果
#[derive(Clone, Debug)]
pub struct FromPlugin {
    pub design: Design,
    /// 書き足す .jidl の中身 (プラグインの record・enum のうち juiz_std でないもの)。無ければ None
    pub jidl: Option<String>,
    /// 警告 (`<道筋>: <文>`)
    pub warnings: Vec<String>,
}

/// 名前付き型の集め役
#[derive(Default)]
struct Types {
    /// juiz_std でない record・enum
    own: Vec<DataType>,
    /// 使う juiz_std の組み込みのファイル (`juiz_std/geometry.jidl`)
    builtins: Vec<String>,
    /// .jidl から import する juiz_std のファイル
    imports: Vec<String>,
}

fn builtin_of(t: &DataType) -> Option<String> {
    let ns = match t {
        DataType::Record(r) => &r.namespace,
        DataType::Enum(e) => &e.namespace,
        _ => return None,
    };
    let rest = ns.strip_prefix("juiz_std.")?;
    let file = format!("juiz_std/{}.jidl", rest.split('.').next()?);
    juiz_idl::builtin_file(&file).map(|(p, _)| p.to_owned())
}

impl Types {
    /// 引数・戻り値の型 `t` が使う名前付き型を集める
    fn add(&mut self, t: &DataType) {
        let mut named = Vec::new();
        collect_named(t, &mut named);
        for n in named {
            match builtin_of(&n) {
                Some(f) => {
                    if !self.builtins.contains(&f) {
                        self.builtins.push(f);
                    }
                }
                None => {
                    if !self.own.contains(&n) {
                        // juiz_std の型をフィールドに使っていれば、.jidl から import する
                        if let DataType::Record(r) = &n {
                            for fld in &r.fields {
                                let mut inner = Vec::new();
                                collect_named(&fld.data_type, &mut inner);
                                for f in inner.iter().filter_map(builtin_of) {
                                    if !self.imports.contains(&f) {
                                        self.imports.push(f);
                                    }
                                }
                            }
                        }
                        self.own.push(n);
                    }
                }
            }
        }
    }
}

/// manifest の引数 1 つを文書の引数にする。型の無い引数は type_name から型を決めて警告する
fn argument(a: &ArgumentManifest, path: &str, types: &mut Types, warnings: &mut Vec<String>) -> Result<ArgumentDesign, PkgError> {
    let (type_expr, is_image) = match &a.schema {
        Some(s) => {
            let t = s
                .data_type()
                .map_err(|e| PkgError::new(format!("{path}: the plugin's type of the argument '{}' cannot be read: {e}", a.name)))?;
            types.add(&t);
            (type_expression(&t), t == DataType::Image)
        }
        None => {
            let e = match a.type_name.as_str() {
                "Bool" => "boolean",
                "Int" => "long",
                "Float" => "double",
                "String" => "string",
                "Array" => "array<any>",
                "Image" => "image",
                _ => "any",
            };
            warnings.push(format!(
                "{path}: the plugin gives no type (schema) for the argument '{}', so it was written as {e} from its type_name {}",
                a.name,
                a.type_name.as_str()
            ));
            (e.to_owned(), e == "image")
        }
    };
    Ok(ArgumentDesign {
        name: a.name.clone(),
        type_expr,
        default: Some(a.default.clone()).filter(|v| !v.is_null() && !is_image),
        description: text(&a.description),
    })
}

fn process(m: &ProcessManifest, path: &str, label: &str, container_process: bool, types: &mut Types, warnings: &mut Vec<String>) -> Result<ProcessDesign, PkgError> {
    let mut arguments = Vec::new();
    for (i, a) in m.arguments.iter().enumerate() {
        arguments.push(argument(a, &format!("{path}.arguments[{i}]"), types, warnings)?);
    }
    let returns = match &m.output_schema {
        Some(s) => {
            let t = s
                .data_type()
                .map_err(|e| PkgError::new(format!("{path}: the plugin's return type of '{}' cannot be read: {e}", m.type_name)))?;
            types.add(&t);
            Some(type_expression(&t)).filter(|_| t != DataType::Null)
        }
        None => {
            warnings.push(format!(
                "{path}: the plugin gives no type (output_schema) for the return value of '{}', so it was written as any",
                m.type_name
            ));
            Some("any".to_owned())
        }
    };
    let label = format!("{label} '{}'", m.type_name);
    topics(m, path, &label, |p, msg| warnings.push(format!("{p}: {msg}")));
    Ok(ProcessDesign {
        name: m.type_name.clone(),
        description: text(&m.description),
        arguments,
        returns,
        readonly: container_process && m.readonly,
    })
}

/// プラグインの manifest から文書を作る (§8.6)。`name` があれば component.name をそれにする。
/// `jidl_path` は .jidl を書くときの `idl:` のパス (文書のディレクトリからの相対パス)
pub fn from_manifest(m: &ComponentManifest, name: Option<&str>, lang: Lang, jidl_path: &str) -> Result<FromPlugin, PkgError> {
    let mut warnings = Vec::new();
    let mut types = Types::default();
    let mut processes = Vec::new();
    for (i, p) in m.processes.iter().enumerate() {
        processes.push(process(p, &format!("processes[{i}]"), "process", false, &mut types, &mut warnings)?);
    }
    let mut containers = Vec::new();
    for (i, k) in m.containers.iter().enumerate() {
        let path = format!("containers[{i}]");
        if cpp_without_arguments(k) {
            warnings.push(format!(
                "{path}.arguments: the C++ SDK does not put the arguments of a container in its manifest, so write them by hand if '{}' has any",
                k.type_name
            ));
        }
        let mut arguments = Vec::new();
        for (j, a) in k.arguments.iter().enumerate() {
            arguments.push(argument(a, &format!("{path}.arguments[{j}]"), &mut types, &mut warnings)?);
        }
        let mut cps = Vec::new();
        for (j, p) in k.processes.iter().enumerate() {
            cps.push(process(p, &format!("{path}.processes[{j}]"), "container process", true, &mut types, &mut warnings)?);
        }
        containers.push(ContainerDesign {
            name: k.type_name.clone(),
            description: text(&k.description),
            arguments,
            state: Vec::new(),
            processes: cps,
        });
    }
    let jidl = types.own.first().map(|first| {
        let ns = match first {
            DataType::Record(r) => r.namespace.clone(),
            DataType::Enum(e) => e.namespace.clone(),
            _ => unreachable!(),
        };
        juiz_idl::jidl_file(&ns, &types.imports, &types.own)
    });
    let mut idl = Vec::new();
    if jidl.is_some() {
        idl.push(jidl_path.to_owned());
    }
    idl.extend(types.builtins);
    let lang_of_plugin = manifest_lang(&m.language);
    if lang_of_plugin.is_some_and(|l| l != lang) {
        warnings.push(format!(
            "component.language: the plugin says its language is {}, but {} was used (-b)",
            m.language,
            lang.plugin_language()
        ));
    }
    Ok(FromPlugin {
        design: Design {
            component: ComponentDesign {
                name: name.map(str::to_owned).unwrap_or_else(|| m.type_name.clone()),
                description: text(&m.description),
                language: Some(lang),
            },
            idl,
            processes,
            containers,
            positions: Vec::new(),
        },
        jidl,
        warnings,
    })
}
