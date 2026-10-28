//! JIDL と .avsc を読み、名前を解決して型とシグネチャのモデル ([`IdlModel`]) にする

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use juiz_types::{
    check_value, is_valid_type_name, DataType, EnumType, RecordField, RecordType, TypeError,
    TypeSchema, RESERVED_NAMESPACE,
};
use serde::Serialize;
use serde_json::Value;

use crate::builtin::{builtin_file, BUILTIN_PREFIX};
use crate::parser::{
    parse_idl, Declaration, EnumDecl, FieldDecl, IdlFile, ImportKind, Pos, ProcessDecl, RecordDecl,
    TypeExpr,
};
use crate::IdlError;

/// 読み込んだ全ファイルの型とシグネチャ
#[derive(Clone, Debug, Default)]
pub struct IdlModel {
    /// 名前付き型 (record と enum)。読んだ順 (import した先が先)
    pub types: Vec<DataType>,
    /// `process` / `container` 宣言 (名前空間ごと、仕様 §9.3)
    pub signatures: Vec<Signatures>,
    /// 読み込んだファイル (import した先も含む)。build.rs の rerun-if-changed に使う
    pub files: Vec<PathBuf>,
}

/// 1 つの名前空間の `<namespace>.processes.json` (仕様 §9.3)
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Signatures {
    pub namespace: String,
    pub processes: Vec<ProcessSignature>,
    pub containers: Vec<ContainerSignature>,
}

/// ProcessFactory・ContainerProcessFactory のシグネチャ
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProcessSignature {
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub arguments: Vec<ArgumentSignature>,
    pub output_schema: TypeSchema,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ArgumentSignature {
    pub name: String,
    /// manifest の `type_name` (仕様 §6 の表)
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub schema: TypeSchema,
    /// 辞書の表し方 (仕様 §4) の既定値
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ContainerSignature {
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub container_processes: Vec<ProcessSignature>,
}

impl IdlModel {
    /// 完全名で名前付き型を探す
    pub fn get_type(&self, full_name: &str) -> Option<&DataType> {
        self.types
            .iter()
            .find(|t| t.full_name().as_deref() == Some(full_name))
    }
}

/// .jidl / .avsc のファイルを読み、import した先も含めてモデルにする
pub fn load_files<P: AsRef<Path>>(paths: &[P]) -> Result<IdlModel, IdlError> {
    load_files_with(paths, &LoadOptions::default())
}

/// ファイルの読み方 (仕様 §24.1)
#[derive(Clone, Debug, Default)]
pub struct LoadOptions {
    /// import の探し先。import を書いたファイルからの相対パスに見つからないとき、この順に探す。
    /// どこにもなければ組み込みの型 (`juiz_std/…`) から探す
    pub import_dirs: Vec<PathBuf>,
}

impl LoadOptions {
    /// 環境変数 `JUIZ_IDL_PATH` (`PATH` と同じ区切りのディレクトリの並び) を import の探し先の後ろに足す
    pub fn with_env_import_path(mut self) -> Self {
        if let Some(v) = std::env::var_os(IMPORT_PATH_ENV) {
            self.import_dirs
                .extend(std::env::split_paths(&v).filter(|p| !p.as_os_str().is_empty()));
        }
        self
    }
}

/// import の探し先を足す環境変数
pub const IMPORT_PATH_ENV: &str = "JUIZ_IDL_PATH";

/// [`LoadOptions`] のとおりに .jidl / .avsc のファイルを読む。
/// ファイルがなく、組み込みの型のパス (`juiz_std/geometry.jidl` など) なら組み込みのものを読む
pub fn load_files_with<P: AsRef<Path>>(
    paths: &[P],
    options: &LoadOptions,
) -> Result<IdlModel, IdlError> {
    let mut loader = Loader {
        import_dirs: options.import_dirs.clone(),
        ..Default::default()
    };
    for p in paths {
        let p = p.as_ref();
        let source = if p.exists() {
            Source::File(p.to_path_buf())
        } else {
            match p.to_str().and_then(builtin_file) {
                Some((path, contents)) => Source::Builtin(path, contents),
                None => Source::File(p.to_path_buf()),
            }
        };
        loader.load_source(source, &mut Vec::new())?;
    }
    Ok(loader.model)
}

/// JIDL の文字列を読む (import は使えない)。`file_name` はエラーの表示に使う
pub fn load_str(source: &str, file_name: &str) -> Result<IdlModel, IdlError> {
    let mut loader = Loader::default();
    let file = parse_idl(source, file_name)?;
    if let Some(i) = file.imports.first() {
        return Err(semantic(
            file_name,
            i.pos,
            "import cannot be used when reading from a string",
        ));
    }
    loader.add_idl(&file, file_name)?;
    Ok(loader.model)
}

fn semantic(file: &str, pos: Pos, message: impl Into<String>) -> IdlError {
    IdlError::Semantic {
        file: file.to_owned(),
        line: pos.line,
        column: pos.column,
        message: message.into(),
    }
}

fn type_error(file: &str, pos: Pos, error: TypeError) -> IdlError {
    IdlError::Type {
        file: file.to_owned(),
        line: pos.line,
        column: pos.column,
        error: Box::new(error),
    }
}

fn io_error(path: &Path, e: impl std::fmt::Display) -> IdlError {
    IdlError::Io {
        path: path.display().to_string(),
        message: e.to_string(),
    }
}

fn join_full_name(namespace: &str, name: &str) -> String {
    if namespace.is_empty() {
        name.to_owned()
    } else {
        format!("{namespace}.{name}")
    }
}

fn is_reserved_namespace(namespace: &str) -> bool {
    namespace == RESERVED_NAMESPACE || namespace.starts_with("juiz.")
}

#[derive(Default)]
struct Loader {
    model: IdlModel,
    /// 完全名 → model.types の添字
    index: HashMap<String, usize>,
    loaded: HashSet<PathBuf>,
    import_dirs: Vec<PathBuf>,
}

/// 読むファイル
enum Source {
    File(PathBuf),
    /// 組み込みのファイル (import に書くパス、中身)
    Builtin(&'static str, &'static str),
}

impl Source {
    fn display(&self) -> String {
        match self {
            Source::File(p) => p.display().to_string(),
            Source::Builtin(p, _) => format!("{BUILTIN_PREFIX}{p}"),
        }
    }

    fn is_avsc(&self) -> bool {
        match self {
            Source::File(p) => p.extension().and_then(|e| e.to_str()) == Some("avsc"),
            Source::Builtin(p, _) => p.ends_with(".avsc"),
        }
    }
}

impl Loader {
    /// `stack` は import の循環を見つけるための読みかけのファイル
    fn load_source(&mut self, source: Source, stack: &mut Vec<PathBuf>) -> Result<(), IdlError> {
        let file_name = source.display();
        let (key, text) = match &source {
            Source::File(path) => {
                let canonical = path.canonicalize().map_err(|e| io_error(path, e))?;
                (canonical, None)
            }
            Source::Builtin(_, contents) => (PathBuf::from(&file_name), Some(*contents)),
        };
        if stack.contains(&key) {
            return Err(IdlError::Io {
                path: file_name,
                message: "import is circular".to_owned(),
            });
        }
        if !self.loaded.insert(key.clone()) {
            return Ok(());
        }
        let source_text = match (&source, text) {
            (_, Some(t)) => t.to_owned(),
            (Source::File(path), None) => {
                self.model.files.push(path.to_path_buf());
                std::fs::read_to_string(path).map_err(|e| io_error(path, e))?
            }
            (Source::Builtin(..), None) => unreachable!(),
        };
        if source.is_avsc() {
            return self.add_avsc(&source_text, &file_name);
        }
        let file = parse_idl(&source_text, &file_name)?;
        stack.push(key);
        for import in file.imports.iter() {
            let target = self.resolve_import(&source, &import.path, &file_name, import.pos)?;
            match (&import.kind, target.is_avsc()) {
                (ImportKind::Idl, true) => {
                    return Err(semantic(
                        &file_name,
                        import.pos,
                        "use 'import schema' for .avsc files",
                    ))
                }
                (ImportKind::Schema, false) => {
                    return Err(semantic(
                        &file_name,
                        import.pos,
                        "'import schema' takes a .avsc file",
                    ))
                }
                _ => {}
            }
            self.load_source(target, stack)?;
        }
        stack.pop();
        self.add_idl(&file, &file_name)
    }

    /// import のパスを、import を書いたファイルからの相対パス、import の探し先、組み込みのファイルの順に探す
    fn resolve_import(
        &self,
        from: &Source,
        path: &str,
        file_name: &str,
        pos: Pos,
    ) -> Result<Source, IdlError> {
        match from {
            Source::File(f) => {
                let relative = f.parent().unwrap_or(Path::new(".")).join(path);
                if relative.exists() {
                    return Ok(Source::File(relative));
                }
            }
            Source::Builtin(f, _) => {
                let dir = f.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
                if let Some((p, c)) = builtin_file(&format!("{dir}/{path}")) {
                    return Ok(Source::Builtin(p, c));
                }
            }
        }
        for dir in self.import_dirs.iter() {
            let candidate = dir.join(path);
            if candidate.exists() {
                return Ok(Source::File(candidate));
            }
        }
        if let Some((p, c)) = builtin_file(path) {
            return Ok(Source::Builtin(p, c));
        }
        Err(semantic(
            file_name,
            pos,
            format!("imported file '{path}' is not found (searched relative to this file, import dirs (-I, {IMPORT_PATH_ENV}) and built-in types)"),
        ))
    }

    /// 名前付き型を登録する。同じ完全名で中身の違う型がすでにあればエラー
    fn register(&mut self, t: DataType, file: &str, pos: Pos) -> Result<(), IdlError> {
        let full_name = t.full_name().unwrap();
        match self.index.get(&full_name) {
            Some(&i) => {
                if self.model.types[i] != t {
                    return Err(semantic(
                        file,
                        pos,
                        format!("type '{full_name}' is defined twice with different contents"),
                    ));
                }
            }
            None => {
                self.index.insert(full_name, self.model.types.len());
                self.model.types.push(t);
            }
        }
        Ok(())
    }

    fn add_avsc(&mut self, source: &str, file_name: &str) -> Result<(), IdlError> {
        let pos = Pos { line: 1, column: 1 };
        let t = DataType::from_avro_str(source).map_err(|e| type_error(file_name, pos, e))?;
        let mut named = Vec::new();
        collect_named(&t, &mut named);
        if named.is_empty() {
            return Err(semantic(
                file_name,
                pos,
                "a .avsc file must define a record or an enum",
            ));
        }
        for n in named {
            self.register(n, file_name, pos)?;
        }
        Ok(())
    }

    fn add_idl(&mut self, file: &IdlFile, file_name: &str) -> Result<(), IdlError> {
        let start = Pos { line: 1, column: 1 };
        check_namespace(&file.namespace, file_name, start)?;
        let mut resolver = Resolver {
            file,
            file_name,
            loader: self,
            local: HashMap::new(),
            resolved: HashMap::new(),
            in_progress: HashSet::new(),
        };
        // 同じファイルの名前付き型 (宣言の順番は自由で、後ろの型を前で参照してよい)
        for (i, d) in file.declarations.iter().enumerate() {
            let (full_name, pos) = match d {
                Declaration::Record(r) => (resolver.full_name_of(&r.name, &r.namespace), r.pos),
                Declaration::Enum(e) => (resolver.full_name_of(&e.name, &e.namespace), e.pos),
                _ => continue,
            };
            if resolver.local.insert(full_name.clone(), i).is_some() {
                return Err(semantic(
                    file_name,
                    pos,
                    format!("type '{full_name}' is declared twice"),
                ));
            }
        }
        let mut types = Vec::new();
        for (i, d) in file.declarations.iter().enumerate() {
            let pos = match d {
                Declaration::Record(r) => r.pos,
                Declaration::Enum(e) => e.pos,
                _ => continue,
            };
            types.push((resolver.resolve_decl(i)?, pos));
        }
        let mut processes = Vec::new();
        let mut containers = Vec::new();
        let mut process_names = HashSet::new();
        let mut container_names = HashSet::new();
        for d in file.declarations.iter() {
            match d {
                Declaration::Process(p) => {
                    if !process_names.insert(p.name.clone()) {
                        return Err(semantic(
                            file_name,
                            p.pos,
                            format!("process '{}' is declared twice", p.name),
                        ));
                    }
                    processes.push(resolver.process(p)?);
                }
                Declaration::Container(c) => {
                    if !container_names.insert(c.name.clone()) {
                        return Err(semantic(
                            file_name,
                            c.pos,
                            format!("container '{}' is declared twice", c.name),
                        ));
                    }
                    let mut names = HashSet::new();
                    let mut cps = Vec::new();
                    for p in c.processes.iter() {
                        if !names.insert(p.name.clone()) {
                            return Err(semantic(
                                file_name,
                                p.pos,
                                format!(
                                    "container_process '{}' is declared twice in '{}'",
                                    p.name, c.name
                                ),
                            ));
                        }
                        cps.push(resolver.process(p)?);
                    }
                    containers.push(ContainerSignature {
                        type_name: c.name.clone(),
                        description: c.doc.clone(),
                        container_processes: cps,
                    });
                }
                _ => {}
            }
        }
        for (t, pos) in types {
            self.register(t, file_name, pos)?;
        }
        if !processes.is_empty() || !containers.is_empty() {
            self.add_signatures(&file.namespace, processes, containers, file_name)?;
        }
        Ok(())
    }

    /// 同じ名前空間のシグネチャは 1 つの processes.json にまとめる
    fn add_signatures(
        &mut self,
        namespace: &str,
        processes: Vec<ProcessSignature>,
        containers: Vec<ContainerSignature>,
        file_name: &str,
    ) -> Result<(), IdlError> {
        let pos = Pos { line: 1, column: 1 };
        let sigs = match self
            .model
            .signatures
            .iter_mut()
            .find(|s| s.namespace == namespace)
        {
            Some(s) => s,
            None => {
                self.model.signatures.push(Signatures {
                    namespace: namespace.to_owned(),
                    processes: Vec::new(),
                    containers: Vec::new(),
                });
                self.model.signatures.last_mut().unwrap()
            }
        };
        for p in processes {
            if sigs.processes.iter().any(|q| q.type_name == p.type_name) {
                return Err(semantic(
                    file_name,
                    pos,
                    format!(
                        "process '{}' is declared twice in namespace '{namespace}'",
                        p.type_name
                    ),
                ));
            }
            sigs.processes.push(p);
        }
        for c in containers {
            if sigs.containers.iter().any(|q| q.type_name == c.type_name) {
                return Err(semantic(
                    file_name,
                    pos,
                    format!(
                        "container '{}' is declared twice in namespace '{namespace}'",
                        c.type_name
                    ),
                ));
            }
            sigs.containers.push(c);
        }
        Ok(())
    }
}

fn check_namespace(namespace: &str, file: &str, pos: Pos) -> Result<(), IdlError> {
    if namespace.is_empty() {
        return Err(semantic(file, pos, "namespace must not be empty"));
    }
    if let Some(e) = namespace.split('.').find(|e| !is_valid_type_name(e)) {
        return Err(semantic(
            file,
            pos,
            format!("namespace '{namespace}' has an invalid element '{e}'"),
        ));
    }
    if is_reserved_namespace(namespace) {
        return Err(semantic(
            file,
            pos,
            format!("namespace '{namespace}' is reserved for juiz"),
        ));
    }
    Ok(())
}

fn check_name(name: &str, file: &str, pos: Pos) -> Result<(), IdlError> {
    if is_valid_type_name(name) {
        Ok(())
    } else {
        Err(semantic(
            file,
            pos,
            format!("name '{name}' must match [A-Za-z_][A-Za-z0-9_]*"),
        ))
    }
}

/// 型の中の名前付き型 (record と enum) を外側から順に集める
pub(crate) fn collect_named(t: &DataType, out: &mut Vec<DataType>) {
    match t {
        DataType::Array(i) | DataType::Map(i) => collect_named(i, out),
        DataType::Optional { inner, .. } => collect_named(inner, out),
        DataType::Record(r) => {
            for f in r.fields.iter() {
                collect_named(&f.data_type, out);
            }
            if !out.contains(t) {
                out.push(t.clone());
            }
        }
        DataType::Enum(_) => {
            if !out.contains(t) {
                out.push(t.clone());
            }
        }
        _ => {}
    }
}

struct Resolver<'a> {
    file: &'a IdlFile,
    file_name: &'a str,
    loader: &'a Loader,
    /// このファイルで宣言した型の完全名 → 宣言の添字
    local: HashMap<String, usize>,
    resolved: HashMap<usize, DataType>,
    in_progress: HashSet<usize>,
}

impl<'a> Resolver<'a> {
    fn full_name_of(&self, name: &str, namespace: &Option<String>) -> String {
        join_full_name(namespace.as_deref().unwrap_or(&self.file.namespace), name)
    }

    fn resolve_decl(&mut self, i: usize) -> Result<DataType, IdlError> {
        if let Some(t) = self.resolved.get(&i) {
            return Ok(t.clone());
        }
        let decl = &self.file.declarations[i];
        let pos = match decl {
            Declaration::Record(r) => r.pos,
            Declaration::Enum(e) => e.pos,
            _ => unreachable!(),
        };
        if !self.in_progress.insert(i) {
            return Err(semantic(
                self.file_name,
                pos,
                "recursive types are not supported",
            ));
        }
        let t = match decl {
            Declaration::Record(r) => self.record(r)?,
            Declaration::Enum(e) => self.enum_type(e)?,
            _ => unreachable!(),
        };
        // 仕様 §3 に合うかを juiz_types で確かめる (書き出して読み直す)
        t.to_avro()
            .map_err(|e| type_error(self.file_name, pos, e))?;
        self.in_progress.remove(&i);
        self.resolved.insert(i, t.clone());
        Ok(t)
    }

    fn type_namespace(&self, namespace: &Option<String>, pos: Pos) -> Result<String, IdlError> {
        match namespace {
            Some(ns) => {
                check_namespace(ns, self.file_name, pos)?;
                Ok(ns.clone())
            }
            None => Ok(self.file.namespace.clone()),
        }
    }

    fn record(&mut self, r: &RecordDecl) -> Result<DataType, IdlError> {
        check_name(&r.name, self.file_name, r.pos)?;
        let mut record = RecordType::new(&self.type_namespace(&r.namespace, r.pos)?, &r.name);
        record.doc = r.doc.clone();
        record.aliases = r.aliases.clone();
        record.attributes = r.attributes.clone();
        let mut names = HashSet::new();
        for f in r.fields.iter() {
            check_name(&f.name, self.file_name, f.pos)?;
            if !names.insert(f.name.clone()) {
                return Err(semantic(
                    self.file_name,
                    f.pos,
                    format!("field '{}' is declared twice", f.name),
                ));
            }
            let data_type = self.type_of(&f.type_expr, f.pos)?;
            if contains_image(&data_type) {
                return Err(semantic(
                    self.file_name,
                    f.pos,
                    "image can be used only as the type of an argument or a return value itself",
                ));
            }
            record = record.field(self.field(f, data_type)?);
        }
        crate::constants::record_constants(&record)
            .map_err(|m| semantic(self.file_name, r.pos, m))?;
        Ok(DataType::Record(record))
    }

    fn field(&self, f: &FieldDecl, data_type: DataType) -> Result<RecordField, IdlError> {
        let written_order = match &f.type_expr {
            TypeExpr::Optional { null_first, .. } => *null_first,
            _ => None,
        };
        let mut field = RecordField::new(&f.name, data_type);
        field.doc = f.doc.clone();
        field.aliases = f.aliases.clone();
        field.attributes = f.attributes.clone();
        if let Some(nf) = written_order {
            if let DataType::Optional { null_first, .. } = &mut field.data_type {
                *null_first = nf;
            }
        }
        if let Some(default) = &f.default {
            field = field
                .default_value(default.clone())
                .map_err(|e| type_error(self.file_name, f.pos, e))?;
            // union { … } と並びを書いたときは、既定値が先頭の型に合わなければエラー (Avro の決まり)
            if let (Some(nf), DataType::Optional { null_first, .. }) =
                (written_order, &field.data_type)
            {
                if nf != *null_first {
                    return Err(semantic(
                        self.file_name,
                        f.pos,
                        "the default of a union must match its first type (write union { null, T } for a null default)",
                    ));
                }
            }
        }
        Ok(field)
    }

    fn enum_type(&mut self, e: &EnumDecl) -> Result<DataType, IdlError> {
        check_name(&e.name, self.file_name, e.pos)?;
        let mut seen = HashSet::new();
        for s in e.symbols.iter() {
            check_name(s, self.file_name, e.pos)?;
            if !seen.insert(s) {
                return Err(semantic(
                    self.file_name,
                    e.pos,
                    format!("symbol '{s}' is declared twice"),
                ));
            }
        }
        let symbols: Vec<&str> = e.symbols.iter().map(|s| s.as_str()).collect();
        let mut t = EnumType::new(
            &self.type_namespace(&e.namespace, e.pos)?,
            &e.name,
            &symbols,
        );
        t.doc = e.doc.clone();
        t.aliases = e.aliases.clone();
        t.attributes = e.attributes.clone();
        if let Some(d) = &e.default {
            if !e.symbols.contains(d) {
                return Err(semantic(
                    self.file_name,
                    e.pos,
                    format!("default '{d}' is not a symbol of '{}'", e.name),
                ));
            }
            t = t.default_symbol(d);
        }
        Ok(DataType::Enum(t))
    }

    fn type_of(&mut self, t: &TypeExpr, pos: Pos) -> Result<DataType, IdlError> {
        Ok(match t {
            TypeExpr::Primitive(p) => match p.as_str() {
                "null" => DataType::Null,
                "boolean" => DataType::Boolean,
                "int" => DataType::Int,
                "long" => DataType::Long,
                "float" => DataType::Float,
                "double" => DataType::Double,
                "string" => DataType::String,
                "bytes" => DataType::Bytes,
                _ => unreachable!(),
            },
            TypeExpr::Void => DataType::Null,
            TypeExpr::Any => DataType::Any,
            TypeExpr::Image => DataType::Image,
            TypeExpr::Array(i) => DataType::array(self.type_of(i, pos)?),
            TypeExpr::Map(i) => DataType::map(self.type_of(i, pos)?),
            TypeExpr::Optional { inner, null_first } => {
                let inner = self.type_of(inner, pos)?;
                if inner == DataType::Null {
                    return Err(semantic(self.file_name, pos, "null cannot be optional"));
                }
                DataType::Optional {
                    inner: Box::new(inner),
                    null_first: null_first.unwrap_or(true),
                }
            }
            TypeExpr::Named(name) => self.named(name, pos)?,
        })
    }

    /// 名前を解決する。同じファイルの名前空間、次に完全名の順 (仕様 §9.2)
    fn named(&mut self, name: &str, pos: Pos) -> Result<DataType, IdlError> {
        let mut candidates = Vec::new();
        if !name.contains('.') {
            candidates.push(join_full_name(&self.file.namespace, name));
        }
        candidates.push(name.to_owned());
        for c in candidates.iter() {
            if let Some(&i) = self.local.get(c) {
                return self.resolve_decl(i);
            }
            if let Some(&i) = self.loader.index.get(c) {
                return Ok(self.loader.model.types[i].clone());
            }
        }
        Err(semantic(
            self.file_name,
            pos,
            format!("unknown type '{name}'"),
        ))
    }

    fn process(&mut self, p: &ProcessDecl) -> Result<ProcessSignature, IdlError> {
        let output = self.type_of(&p.return_type, p.pos)?;
        let output_schema = TypeSchema::from_data_type(&output)
            .map_err(|e| type_error(self.file_name, p.pos, e))?;
        let mut arguments = Vec::new();
        let mut names = HashSet::new();
        for a in p.params.iter() {
            check_name(&a.name, self.file_name, a.pos)?;
            if !names.insert(a.name.clone()) {
                return Err(semantic(
                    self.file_name,
                    a.pos,
                    format!("argument '{}' is declared twice", a.name),
                ));
            }
            let t = self.type_of(&a.type_expr, a.pos)?;
            if t == DataType::Null {
                return Err(semantic(
                    self.file_name,
                    a.pos,
                    "null alone cannot be the type of an argument",
                ));
            }
            let schema =
                TypeSchema::from_data_type(&t).map_err(|e| type_error(self.file_name, a.pos, e))?;
            // 型付きの引数の既定値は辞書の表し方で検査する。T? でない null は「既定値なし」、image は検査しない (仕様 §6)
            if let Some(d) = &a.default {
                let no_default = d.is_null() && !t.is_nullable();
                if t != DataType::Image && !no_default {
                    check_value(&t, d).map_err(|e| type_error(self.file_name, a.pos, e))?;
                }
            }
            arguments.push(ArgumentSignature {
                name: a.name.clone(),
                type_name: t.argument_type_name().to_owned(),
                description: a.doc.clone(),
                schema,
                default: a.default.clone(),
            });
        }
        Ok(ProcessSignature {
            type_name: p.name.clone(),
            description: p.doc.clone(),
            arguments,
            output_schema,
        })
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
