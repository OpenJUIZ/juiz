//! Container を含まない Composite の Factory (issue #167)
//!
//! 仕様は `docs/spec/composition.md` の §5 (評価)、§6 (エラーの位置)。
//! 子の Factory の本体関数を式の木の順に呼ぶ合成クロージャを作り、インスタンスは今の [`ProcessImpl`] で作る。
//! 子のインスタンスは作らない。

use std::sync::Arc;

use juiz_sdk::anyhow::{self, anyhow, Context};
use juiz_sdk::manifests::{
    CompositeArgument, CompositeManifest, CompositeNode, ProcessProfile, COMPOSITE_ROOT_PATH,
};
use juiz_sdk::utils::check_manifest_before_call;

use crate::connections::ConnectionFactoryImpl;
use crate::prelude::*;
use crate::processes::implementations::process_impl::ProcessImpl;
use crate::processes::ProcessBodyFunctionTrait;

/// 戻り値の要約の最大の長さ (trace 用)
const TRACE_SUMMARY_MAX_LEN: usize = 200;

/// 本体関数を取り出した、評価できる形のノード
struct CompiledNode {
    /// 式の木の中の位置 (`body.img` など)
    path: String,
    /// 子の type_name
    process: String,
    /// 子の引数の検査用 (`check_manifest_before_call`)
    profile: ProcessProfile,
    function: Arc<ProcessBodyFunctionTrait>,
    /// 子の arguments の宣言順に並べた引数
    args: Vec<(String, CompiledArgument)>,
}

enum CompiledArgument {
    Node(Box<CompiledNode>),
    Const(Value),
    Expose(String),
}

/// 合成した本体。Composite の type_name とノードの木を持つ
struct CompositeBody {
    type_name: String,
    root: CompiledNode,
}

impl CompositeBody {
    fn call(&self, args: &CapsuleMap, trace: bool) -> JuizResult<Capsule> {
        self.eval(&self.root, args, trace)
    }

    /// ノードを評価する (仕様 §5.1)。引数を子の宣言順・深さ優先に評価してから子を呼ぶ
    fn eval(&self, node: &CompiledNode, args: &CapsuleMap, trace: bool) -> JuizResult<Capsule> {
        let mut values: Vec<(String, CapsulePtr)> = Vec::with_capacity(node.args.len());
        for (name, arg) in node.args.iter() {
            let value = match arg {
                // 下のノードのエラーは、そのノードの位置が付いているのでそのまま返す
                CompiledArgument::Node(child) => self.eval(child, args, trace)?.into(),
                CompiledArgument::Const(v) => v.clone().into(),
                CompiledArgument::Expose(exposed) => {
                    args.get(exposed).map_err(|e| self.node_error(node, e))?
                }
            };
            values.push((name.clone(), value));
        }
        let child_args: CapsuleMap = values.into();
        let result = check_manifest_before_call(&node.profile, &child_args)
            .and_then(|_| (node.function)(child_args))
            .map_err(|e| self.node_error(node, e))?;
        if trace {
            log::info!(
                "Composite({}) {} ({}) -> {}",
                self.type_name,
                node.path,
                node.process,
                summarize(&result)
            );
        }
        Ok(result)
    }

    /// 子のエラーに式の木の中の位置を付ける (仕様 §6)
    fn node_error(&self, node: &CompiledNode, error: anyhow::Error) -> anyhow::Error {
        let message = error.to_string();
        error.context(JuizError::CompositeNodeError {
            type_name: self.type_name.clone(),
            path: node.path.clone(),
            child_type_name: node.process.clone(),
            error: message,
        })
    }
}

/// trace で出す戻り値の要約
fn summarize(capsule: &Capsule) -> String {
    if let Some(v) = capsule.as_value() {
        let s = v.to_string();
        if s.chars().count() > TRACE_SUMMARY_MAX_LEN {
            let head: String = s.chars().take(TRACE_SUMMARY_MAX_LEN).collect();
            return format!("{head}...");
        }
        return s;
    }
    if let Some(img) = capsule.as_image() {
        return format!("image({}x{})", img.width(), img.height());
    }
    if capsule.is_empty() {
        return "empty".to_owned();
    }
    "(value)".to_owned()
}

/// Container を含まない Composite の Factory (仕様 §2、§3)
pub struct CompositeProcessFactory {
    core: ObjectCore,
    manifest: ProcessManifest,
    body: Arc<CompositeBody>,
}

impl CompositeProcessFactory {
    /// Composite の定義と子の Factory から Factory を作る
    ///
    /// `lookup` は子の type_name から、同じ juiz プロセスに登録された ProcessFactory を返す。
    /// 先に登録した Composite も子にできる (入れ子)。スロットを持つ Composite (C3) はまだ扱えない。
    pub fn new<F>(composite: &CompositeManifest, lookup: F) -> JuizResult<Self>
    where
        F: Fn(&str) -> Option<ProcessFactoryPtr>,
    {
        if composite.has_containers() {
            return Err(anyhow!(JuizError::CompositeManifestInvalidError {
                type_name: composite.type_name.clone(),
                path: String::new(),
                message: "composites with container slots are not supported yet (issue #168)"
                    .to_owned(),
            }));
        }
        let lookup_manifest = |type_name: &str| -> Option<ProcessManifest> {
            lookup(type_name).and_then(|pf| pf.lock().ok()?.manifest().ok())
        };
        let manifest = composite.build_process_manifest(&lookup_manifest)?;
        let composition = manifest.composition.as_ref().ok_or_else(|| {
            anyhow!(JuizError::CompositeManifestInvalidError {
                type_name: composite.type_name.clone(),
                path: String::new(),
                message: "composition is missing in the built manifest".to_owned(),
            })
        })?;
        check_cycle(
            &composite.type_name,
            &composition.body,
            COMPOSITE_ROOT_PATH,
            &lookup_manifest,
        )?;
        let root = compile(
            &composite.type_name,
            &composition.body,
            COMPOSITE_ROOT_PATH,
            &lookup,
        )?;
        Ok(CompositeProcessFactory {
            core: ObjectCore::create_factory(
                JuizObjectClass::ProcessFactory("CompositeProcessFactory"),
                manifest.type_name.clone(),
            ),
            body: Arc::new(CompositeBody {
                type_name: manifest.type_name.clone(),
                root,
            }),
            manifest,
        })
    }

    fn function(&self, trace: bool) -> Arc<ProcessBodyFunctionTrait> {
        let body = self.body.clone();
        Arc::new(move |args: CapsuleMap| body.call(&args, trace))
    }
}

fn invalid(type_name: &str, path: &str, message: String) -> anyhow::Error {
    anyhow!(JuizError::CompositeManifestInvalidError {
        type_name: type_name.to_owned(),
        path: path.to_owned(),
        message,
    })
}

/// 入れ子の Composite をたどって、自分自身を含んでいないか調べる (仕様 §3.2)
fn check_cycle<F>(type_name: &str, node: &CompositeNode, path: &str, lookup: &F) -> JuizResult<()>
where
    F: Fn(&str) -> Option<ProcessManifest>,
{
    if node.process == type_name {
        return Err(invalid(
            type_name,
            path,
            format!("composite '{type_name}' refers to itself (cycle)"),
        ));
    }
    if let Some(composition) = lookup(&node.process).and_then(|m| m.composition) {
        check_cycle(type_name, &composition.body, path, lookup)?;
    }
    for (name, arg) in node.args.iter() {
        if let CompositeArgument::Node(child) = arg {
            check_cycle(type_name, child, &format!("{path}.{name}"), lookup)?;
        }
    }
    Ok(())
}

/// 正規化したノードから本体関数を取り出し、評価できる形にする
fn compile<F>(
    type_name: &str,
    node: &CompositeNode,
    path: &str,
    lookup: &F,
) -> JuizResult<CompiledNode>
where
    F: Fn(&str) -> Option<ProcessFactoryPtr>,
{
    let pf = lookup(&node.process).ok_or_else(|| {
        invalid(
            type_name,
            path,
            format!("child process '{}' is not found", node.process),
        )
    })?;
    let (child_manifest, function) = {
        let f = pf.lock()?;
        (f.manifest()?, f.body()?)
    };
    let mut args = Vec::with_capacity(child_manifest.arguments.len());
    for child_arg in child_manifest.arguments.iter() {
        let arg_path = format!("{path}.{}", child_arg.name);
        let compiled = match node.args.get(&child_arg.name) {
            Some(CompositeArgument::Node(n)) => {
                CompiledArgument::Node(Box::new(compile(type_name, n, &arg_path, lookup)?))
            }
            Some(CompositeArgument::Const(v)) => CompiledArgument::Const(v.clone()),
            Some(CompositeArgument::Expose(spec)) => CompiledArgument::Expose(spec.name.clone()),
            // build_process_manifest で正規化した木には、子のすべての引数がある
            None => CompiledArgument::Expose(child_arg.name.clone()),
        };
        args.push((child_arg.name.clone(), compiled));
    }
    let profile: ProcessProfile = child_manifest
        .clone()
        .name(&child_manifest.type_name)
        .try_into()
        .with_context(|| format!("Composite({type_name}) at {path}"))?;
    Ok(CompiledNode {
        path: path.to_owned(),
        process: node.process.clone(),
        profile,
        function,
        args,
    })
}

impl JuizObjectCoreHolder for CompositeProcessFactory {
    fn core(&self) -> &ObjectCore {
        &self.core
    }
}

impl JuizObject for CompositeProcessFactory {
    fn profile_full(&self) -> JuizResult<Value> {
        let mut v = self.core.profile_full()?;
        let arguments = self
            .manifest
            .arguments
            .iter()
            .map(|v| serde_json::to_value(v))
            .collect::<serde_json::Result<Vec<Value>>>()?;
        obj_merge_mut(
            &mut v,
            &jvalue!({
                "arguments": arguments,
                "language": self.manifest.language,
                "composition": serde_json::to_value(&self.manifest.composition)?,
            }),
        )?;
        Ok(v)
    }
}

impl ProcessFactory for CompositeProcessFactory {
    fn create_process(&self, manifest: &ProcessManifest) -> JuizResult<ProcessPtr> {
        log::trace!("CompositeProcessFactory::create_process(manifest={manifest}) called");
        let trace = manifest.trace;
        let mut instance_manifest = self.manifest.build_instance_manifest(manifest.clone())?;
        instance_manifest.language = self.manifest.language.clone();
        instance_manifest.factory = self.manifest.factory.clone();
        // memo は Composite のインスタンス単位 (仕様 §5.3)。Factory の値ではなくインスタンスの指定を使う
        instance_manifest.use_memo = manifest.use_memo;
        Ok(ProcessPtr::new(ProcessImpl::new_from_clousure_ref(
            instance_manifest,
            self.function(trace),
            Box::new(ConnectionFactoryImpl::new()),
        )?))
    }

    fn manifest(&self) -> JuizResult<ProcessManifest> {
        Ok(self.manifest.clone())
    }

    fn body(&self) -> JuizResult<Arc<ProcessBodyFunctionTrait>> {
        Ok(self.function(false))
    }
}
