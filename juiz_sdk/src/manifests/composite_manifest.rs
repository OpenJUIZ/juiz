//! Composition (Composite Process / CompositeContainerProcess) の定義と manifest 生成 (issue #166)
//!
//! 仕様は `docs/spec/composition.md`。ここでは conf の `composites:` の 1 要素にあたる
//! [`CompositeManifest`] と、子の [`ProcessManifest`] 群から Composite の `ProcessManifest` を作る
//! [`CompositeManifest::build_process_manifest`] を持つ。評価 (本体関数の合成) は juiz_core 側で行う。

use std::collections::{BTreeMap, HashSet};

use super::argument_manifest::ArgumentManifest;
use super::argument_type::type_check;
use super::manifest_description::Description;
use crate::identifier::validate_name;
use crate::prelude::*;
use anyhow::anyhow;
use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};

/// CompositeContainer の Container の type_name
pub const COMPOSITE_CONTAINER_TYPE_NAME: &str = "composite_container";
/// Composite Process の factory 名
pub const COMPOSITE_PROCESS_FACTORY_NAME: &str = "composite_process_factory";
/// CompositeContainerProcess の factory 名
pub const COMPOSITE_CONTAINER_PROCESS_FACTORY_NAME: &str = "composite_container_process_factory";
/// 子の言語が混ざっているときの `language`
pub const MIXED_LANGUAGE: &str = "mixed";
/// 式の木の根の位置
pub const COMPOSITE_ROOT_PATH: &str = "body";

/// Composite の定義 (conf の `composites:` の 1 要素、仕様 §3.1)
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompositeManifest {
    pub type_name: String,
    #[serde(default)]
    pub description: Description,
    /// スロット名 → Container の type_name
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub containers: BTreeMap<String, String>,
    pub body: CompositeNode,
}

/// manifest / profile に入れる Composition の情報 (仕様 §8.2)。`body` は正規化した木
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Composition {
    #[serde(default)]
    pub containers: BTreeMap<String, String>,
    pub body: CompositeNode,
}

/// 式の木のノード (子の呼び出し、仕様 §3.2)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompositeNode {
    /// 子の Factory の type_name
    pub process: String,
    /// 子が ContainerProcess のときに使うスロット名
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    /// 子の引数名 → 引数の値
    #[serde(default)]
    pub args: BTreeMap<String, CompositeArgument>,
}

/// ノードの引数の値。conf では `process` / `const` / `expose` のどれか 1 つのキーを持つオブジェクト
#[derive(Clone, Debug, PartialEq)]
pub enum CompositeArgument {
    /// 子の式。その戻り値を渡す
    Node(Box<CompositeNode>),
    /// 定数で拘束する (公開しない)
    Const(Value),
    /// Composite の引数として公開する
    Expose(ExposeSpec),
}

/// `expose` の中身。文字列 (公開名だけ) か `{ name, default, description }`
#[derive(Clone, Debug, PartialEq)]
pub struct ExposeSpec {
    pub name: String,
    pub default: Option<Value>,
    pub description: Option<String>,
}

impl ExposeSpec {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            default: None,
            description: None,
        }
    }

    pub fn default(mut self, default: Value) -> Self {
        self.default = Some(default);
        self
    }

    pub fn description(mut self, description: &str) -> Self {
        self.description = Some(description.to_owned());
        self
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExposeObject {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

impl Serialize for ExposeSpec {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.default.is_none() && self.description.is_none() {
            serializer.serialize_str(&self.name)
        } else {
            ExposeObject {
                name: self.name.clone(),
                default: self.default.clone(),
                description: self.description.clone(),
            }
            .serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for ExposeSpec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match Value::deserialize(deserializer)? {
            Value::String(name) => Ok(ExposeSpec::new(&name)),
            v @ Value::Object(_) => {
                let o: ExposeObject = serde_json::from_value(v).map_err(D::Error::custom)?;
                Ok(ExposeSpec {
                    name: o.name,
                    default: o.default,
                    description: o.description,
                })
            }
            v => Err(D::Error::custom(format!(
                "expose must be a string or {{name, default, description}} (got {v})"
            ))),
        }
    }
}

impl Serialize for CompositeArgument {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        match self {
            CompositeArgument::Node(node) => node.serialize(serializer),
            CompositeArgument::Const(v) => {
                let mut m = serializer.serialize_map(Some(1))?;
                m.serialize_entry("const", v)?;
                m.end()
            }
            CompositeArgument::Expose(e) => {
                let mut m = serializer.serialize_map(Some(1))?;
                m.serialize_entry("expose", e)?;
                m.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for CompositeArgument {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let Some(obj) = value.as_object() else {
            return Err(D::Error::custom(format!(
                "composite argument must be an object with one of 'process', 'const' or 'expose' (got {value})"
            )));
        };
        let kinds: Vec<&str> = ["process", "const", "expose"]
            .into_iter()
            .filter(|k| obj.contains_key(*k))
            .collect();
        if kinds.len() != 1 {
            return Err(D::Error::custom(format!(
                "composite argument must have exactly one of 'process', 'const' or 'expose' (got {value})"
            )));
        }
        match kinds[0] {
            "process" => Ok(CompositeArgument::Node(Box::new(
                serde_json::from_value(value).map_err(D::Error::custom)?,
            ))),
            kind => {
                if obj.len() != 1 {
                    return Err(D::Error::custom(format!(
                        "composite argument '{kind}' must not have other keys (got {value})"
                    )));
                }
                let inner = obj.get(kind).unwrap().clone();
                if kind == "const" {
                    Ok(CompositeArgument::Const(inner))
                } else {
                    Ok(CompositeArgument::Expose(
                        serde_json::from_value(inner).map_err(D::Error::custom)?,
                    ))
                }
            }
        }
    }
}

impl CompositeNode {
    pub fn new(process: &str) -> Self {
        Self {
            process: process.to_owned(),
            container: None,
            args: BTreeMap::new(),
        }
    }

    pub fn container(mut self, slot: &str) -> Self {
        self.container = Some(slot.to_owned());
        self
    }

    pub fn arg(mut self, name: &str, arg: CompositeArgument) -> Self {
        self.args.insert(name.to_owned(), arg);
        self
    }

    pub fn arg_node(self, name: &str, node: CompositeNode) -> Self {
        self.arg(name, CompositeArgument::Node(Box::new(node)))
    }

    pub fn arg_const(self, name: &str, value: Value) -> Self {
        self.arg(name, CompositeArgument::Const(value))
    }

    pub fn arg_expose(self, name: &str, expose: ExposeSpec) -> Self {
        self.arg(name, CompositeArgument::Expose(expose))
    }
}

impl CompositeManifest {
    pub fn new(type_name: &str, body: CompositeNode) -> Self {
        Self {
            type_name: type_name.to_owned(),
            description: Description::default(),
            containers: BTreeMap::new(),
            body,
        }
    }

    pub fn description(mut self, description: &str) -> Self {
        self.description = description.into();
        self
    }

    /// スロットを宣言する (スロット名 → Container の type_name)
    pub fn add_container(mut self, slot: &str, container_type_name: &str) -> Self {
        self.containers
            .insert(slot.to_owned(), container_type_name.to_owned());
        self
    }

    /// スロットを持つ (CompositeContainerProcess の型になる) なら true
    pub fn has_containers(&self) -> bool {
        !self.containers.is_empty()
    }

    /// 子の manifest から Composite の `ProcessManifest` (Factory の manifest) を作る (仕様 §3.2, §4, §8.2)
    ///
    /// `lookup` は子の type_name から、同じ juiz プロセスに登録された子の Factory の manifest を返す。
    /// 見つからなければ `None` を返す (先に定義した Composite だけを返せば循環は作れない)。
    ///
    /// 定義の規則違反は [`JuizError::CompositeManifestInvalidError`] で、ノードの位置 (`body.img.img` など) を付けて返す。
    pub fn build_process_manifest<F>(&self, lookup: F) -> JuizResult<ProcessManifest>
    where
        F: Fn(&str) -> Option<ProcessManifest>,
    {
        if self.type_name.is_empty() {
            return Err(self.error("", "type_name must not be empty".to_owned()));
        }
        for slot in self.containers.keys() {
            validate_name(slot).map_err(|e| {
                self.error("", format!("container slot name '{slot}' is invalid: {e}"))
            })?;
        }
        let mut builder = Builder {
            composite: self,
            lookup: &lookup,
            exposed: Vec::new(),
            used_slots: HashSet::new(),
            languages: Vec::new(),
        };
        let (body, root) = builder.visit(&self.body, COMPOSITE_ROOT_PATH)?;
        for slot in self.containers.keys() {
            if !builder.used_slots.contains(slot) {
                return Err(self.error(
                    "",
                    format!("container slot '{slot}' is declared but not used"),
                ));
            }
        }

        let language = match builder.languages.first() {
            Some(first) if builder.languages.iter().all(|l| l == first) => first.clone(),
            _ => MIXED_LANGUAGE.to_owned(),
        };
        let factory = if self.has_containers() {
            COMPOSITE_CONTAINER_PROCESS_FACTORY_NAME
        } else {
            COMPOSITE_PROCESS_FACTORY_NAME
        };
        let mut manifest = ProcessManifest::new(&self.type_name)
            .description(self.description.as_str())
            .language(&language)
            .factory(factory)
            .set_outputs(root.outputs.clone());
        if self.has_containers() {
            manifest = manifest.container_type(Some(COMPOSITE_CONTAINER_TYPE_NAME.to_owned()));
        }
        for e in builder.exposed.into_iter() {
            manifest = manifest.add_arg(e.arg);
        }
        manifest.composition = Some(Composition {
            containers: self.containers.clone(),
            body,
        });
        Ok(manifest)
    }

    fn error(&self, path: &str, message: String) -> anyhow::Error {
        anyhow!(JuizError::CompositeManifestInvalidError {
            type_name: self.type_name.clone(),
            path: path.to_owned(),
            message
        })
    }
}

/// 公開した引数 1 つ分
struct Exposed {
    arg: ArgumentManifest,
    /// 最初に出てきたノードの位置と子の type_name (エラーメッセージ用)
    first_at: String,
    explicit_default: bool,
    explicit_description: bool,
}

struct Builder<'a, F> {
    composite: &'a CompositeManifest,
    lookup: &'a F,
    exposed: Vec<Exposed>,
    used_slots: HashSet<String>,
    languages: Vec<String>,
}

impl<'a, F> Builder<'a, F>
where
    F: Fn(&str) -> Option<ProcessManifest>,
{
    /// ノードを評価順 (子の arguments の宣言順、深さ優先) にたどり、正規化したノードと子の manifest を返す
    fn visit(
        &mut self,
        node: &CompositeNode,
        path: &str,
    ) -> JuizResult<(CompositeNode, ProcessManifest)> {
        let composite = self.composite;
        let err = |message: String| composite.error(path, message);
        if node.process == self.composite.type_name {
            return Err(err(format!(
                "composite '{}' must not refer to itself",
                node.process
            )));
        }
        let child = (self.lookup)(&node.process).ok_or_else(|| {
            err(format!(
                "child process '{}' is not found (only factories in this juiz process and composites defined earlier can be used)",
                node.process
            ))
        })?;
        if let Some(c) = child.composition.as_ref() {
            if !c.containers.is_empty() {
                return Err(err(format!(
                    "child '{}' is a composite with container slots, which cannot be nested",
                    node.process
                )));
            }
        }

        match (child.container_type.as_ref(), node.container.as_ref()) {
            (Some(child_container_type), Some(slot)) => {
                let slot_type = self.composite.containers.get(slot).ok_or_else(|| {
                    err(format!(
                        "container slot '{slot}' is not declared in 'containers'"
                    ))
                })?;
                if slot_type != child_container_type {
                    return Err(err(format!(
                        "container slot '{slot}' is '{slot_type}' but child '{}' needs '{child_container_type}'",
                        node.process
                    )));
                }
                self.used_slots.insert(slot.clone());
            }
            (Some(child_container_type), None) => {
                return Err(err(format!(
                    "child '{}' is a container process ('{child_container_type}'), so 'container' (slot name) is required",
                    node.process
                )));
            }
            (None, Some(_)) => {
                return Err(err(format!(
                    "child '{}' is not a container process, so 'container' must not be given",
                    node.process
                )));
            }
            (None, None) => {}
        }

        for name in node.args.keys() {
            if !child.arguments.iter().any(|a| &a.name == name) {
                return Err(err(format!(
                    "child '{}' does not have argument '{name}'",
                    node.process
                )));
            }
        }

        self.languages.push(child.language.clone());
        let mut args = BTreeMap::new();
        for child_arg in child.arguments.iter() {
            let arg_path = format!("{path}.{}", child_arg.name);
            let normalized = match node.args.get(&child_arg.name) {
                None => {
                    let spec = ExposeSpec::new(&child_arg.name);
                    self.expose(&spec, child_arg, path, &node.process)?;
                    CompositeArgument::Expose(spec)
                }
                Some(CompositeArgument::Node(n)) => {
                    let (n, _) = self.visit(n, &arg_path)?;
                    CompositeArgument::Node(Box::new(n))
                }
                Some(CompositeArgument::Const(v)) => {
                    type_check(&child_arg.type_name, v).map_err(|_| {
                        err(format!(
                            "const value {v} for argument '{}' does not match type {}",
                            child_arg.name, child_arg.type_name
                        ))
                    })?;
                    CompositeArgument::Const(v.clone())
                }
                Some(CompositeArgument::Expose(spec)) => {
                    self.expose(spec, child_arg, path, &node.process)?;
                    CompositeArgument::Expose(spec.clone())
                }
            };
            args.insert(child_arg.name.clone(), normalized);
        }
        Ok((
            CompositeNode {
                process: node.process.clone(),
                container: node.container.clone(),
                args,
            },
            child,
        ))
    }

    /// 子の引数を公開名で公開する (仕様 §4)
    fn expose(
        &mut self,
        spec: &ExposeSpec,
        child_arg: &ArgumentManifest,
        path: &str,
        child_type_name: &str,
    ) -> JuizResult<()> {
        let composite = self.composite;
        let err = |message: String| composite.error(path, message);
        validate_name(&spec.name).map_err(|e| {
            err(format!(
                "exposed name '{}' for argument '{}' is invalid: {e}",
                spec.name, child_arg.name
            ))
        })?;
        if let Some(d) = spec.default.as_ref() {
            type_check(&child_arg.type_name, d).map_err(|_| {
                err(format!(
                    "default value {d} of exposed argument '{}' does not match type {}",
                    spec.name, child_arg.type_name
                ))
            })?;
        }
        let here = format!("{path} ({child_type_name})");
        if let Some(e) = self.exposed.iter_mut().find(|e| e.arg.name == spec.name) {
            if e.arg.type_name != child_arg.type_name {
                return Err(self.composite.error(
                    path,
                    format!(
                        "exposed argument '{}' collides: {} has type {} but {} has type {}. Give one of them another name with 'expose'",
                        spec.name, e.first_at, e.arg.type_name, here, child_arg.type_name
                    ),
                ));
            }
            if let Some(d) = spec.default.as_ref() {
                if e.explicit_default && &e.arg.default != d {
                    return Err(self.composite.error(
                        path,
                        format!(
                            "exposed argument '{}' has different defaults ({} at {}, {} at {})",
                            spec.name, e.arg.default, e.first_at, d, here
                        ),
                    ));
                }
                e.arg.default = d.clone();
                e.explicit_default = true;
            }
            if let Some(desc) = spec.description.as_ref() {
                if e.explicit_description && e.arg.description.as_str() != desc {
                    return Err(self.composite.error(
                        path,
                        format!(
                            "exposed argument '{}' has different descriptions (at {} and {})",
                            spec.name, e.first_at, here
                        ),
                    ));
                }
                e.arg.description = desc.as_str().into();
                e.explicit_description = true;
            }
            return Ok(());
        }
        let description = match spec.description.as_ref() {
            Some(d) => d.as_str().into(),
            None => child_arg.description.clone(),
        };
        self.exposed.push(Exposed {
            arg: ArgumentManifest::new(
                child_arg.type_name.clone(),
                &spec.name,
                description,
                spec.default.clone().unwrap_or(child_arg.default.clone()),
            ),
            first_at: here,
            explicit_default: spec.default.is_some(),
            explicit_description: spec.description.is_some(),
        });
        Ok(())
    }
}
