//! Container / ContainerImpl / ProcessPtr とファクトリ関数のテスト

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use juiz_sdk::connections::ConnectionProfile;
use juiz_sdk::manifests::ProcessProfile;
use juiz_sdk::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;
use juiz_sdk::processes::{Process, ProcessPtr};

struct Counter {
    value: i64,
}

fn counter_manifest() -> ContainerManifest {
    ContainerManifest::new("counter").name("counter0").add_int_arg("initial", "", 0)
}

/// コンテナに登録するためだけの最小のプロセス
#[derive(Debug)]
struct DummyProcess {
    id: ProcessIdentifier,
    purged: Arc<AtomicUsize>,
}

impl DummyProcess {
    fn new(name: &str, purged: Arc<AtomicUsize>) -> Self {
        Self {
            id: ProcessIdentifier::new_container_process_id("core".into(), "core".into(), "get".into(), "counter0".into(), name.into()),
            purged,
        }
    }
}

impl Process for DummyProcess {
    fn call(&self, _args: CapsuleMap) -> JuizResult<CapsulePtr> { Ok(jvalue!(1).into()) }
    fn is_updated(&self) -> JuizResult<bool> { Ok(false) }
    fn identifier(&self) -> ProcessIdentifier { self.id.clone() }
    fn profile(&self) -> JuizResult<ProcessProfile> { Ok(ProcessProfile::new(&self.id.name, "get")) }
    fn invoke<'b>(&self) -> JuizResult<CapsulePtr> { unimplemented!() }
    fn execute(&self) -> JuizResult<CapsulePtr> { unimplemented!() }
    fn push_by(&self, _arg_name: &str, _value: CapsulePtr) -> JuizResult<CapsulePtr> { unimplemented!() }
    fn get_output(&self) -> CapsulePtr { CapsulePtr::new() }
    fn notify_connected_from<'b>(&'b mut self, _source: ProcessPtr, _m: &ConnectionManifest) -> JuizResult<ConnectionProfile> { unimplemented!() }
    fn try_connect_to(&mut self, _target: ProcessPtr, _m: &ConnectionManifest) -> JuizResult<ConnectionManifest> { unimplemented!() }
    fn source_connections(&self) -> JuizResult<Vec<&Box<dyn SourceConnection>>> { Ok(vec![]) }
    fn destination_connections(&self) -> JuizResult<Vec<&Box<dyn DestinationConnection>>> { Ok(vec![]) }
    fn p_apply(&mut self, _arg_name: &str, _value: CapsulePtr) -> JuizResult<CapsulePtr> { unimplemented!() }
    fn purge(&mut self) -> JuizResult<()> {
        self.purged.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn openapi_spec(&self) -> JuizResult<Value> { Ok(jvalue!({})) }
}

#[test]
fn container_impl_derefs_to_user_struct() -> JuizResult<()> {
    let mut c = ContainerImpl::new(counter_manifest(), Box::new(Counter { value: 3 }))?;
    assert_eq!(c.value, 3);
    c.value += 1;
    assert_eq!(c.t.value, 4);
    assert_eq!(c.profile()?.name, "counter0");
    assert_eq!(c.profile()?.type_name, "counter");
    assert!(format!("{}", c).starts_with("ContainerImpl(identifier="));
    assert!(format!("{:?}", c).contains("User object."));
    Ok(())
}

#[test]
fn container_impl_requires_name() {
    assert!(ContainerImpl::new(ContainerManifest::new("counter"), Box::new(Counter { value: 0 })).is_err());
}

#[test]
fn container_registers_finds_and_purges_processes() -> JuizResult<()> {
    let purged = Arc::new(AtomicUsize::new(0));
    let mut c = ContainerImpl::new(counter_manifest(), Box::new(Counter { value: 0 }))?;
    assert!(c.processes().is_empty());

    let p = ProcessPtr::new(DummyProcess::new("get0", purged.clone()));
    let id = p.identifier().to_string();
    c.register_process(p.clone())?;
    c.register_process(ProcessPtr::new(DummyProcess::new("get1", purged.clone())))?;
    assert_eq!(c.processes().len(), 2);

    // 識別子で検索できる
    let found = c.process(&id).expect("process should be found by identifier");
    assert_eq!(found.identifier(), p.identifier());
    assert!(c.process(&"core://core/container_process/none::get:counter0".to_owned()).is_none());
    assert!(c.process(&"unknown".to_owned()).is_none());

    c.purge_process(&id)?;
    assert_eq!(c.processes().len(), 1);
    assert!(c.purge_process(&id).is_err());

    // clear は残っているプロセスを purge してから空にする
    c.clear()?;
    assert!(c.processes().is_empty());
    assert_eq!(purged.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test]
fn process_ptr_locks_and_downcasts() -> JuizResult<()> {
    let purged = Arc::new(AtomicUsize::new(0));
    let p = ProcessPtr::new(DummyProcess::new("get0", purged.clone()));
    assert_eq!(p.lock()?.call(CapsuleMap::new())?.lock_as_value(|v| v.clone())?, jvalue!(1));
    assert!(!p.lock()?.is_updated()?);
    p.lock_mut()?.purge()?;
    assert_eq!(purged.load(Ordering::SeqCst), 1);

    assert_eq!(p.downcast_and_then(|d: &DummyProcess| d.id.name.clone())?, "get0");
    p.downcast_mut_and_then(|d: &mut DummyProcess| d.id.name = "renamed".to_owned())?;
    assert_eq!(p.lock()?.identifier().name, "renamed");
    // ProcessPtr の識別子は生成時のもののまま
    assert_eq!(p.identifier().name, "get0");
    assert!(format!("{:?}", p).contains("ProcessPtr"));
    Ok(())
}

#[test]
fn container_ptr_locks_and_downcasts() -> JuizResult<()> {
    let c = ContainerImpl::new(counter_manifest(), Box::new(Counter { value: 10 }))?;
    let id = c.identifier();
    let ptr = ContainerPtr::new(c);
    assert_eq!(ptr.identifier(), id);
    assert_eq!(ptr.lock()?.identifier(), id);
    {
        let mut guard = ptr.lock_mut()?;
        let inner = guard.downcast_mut::<ContainerImpl<Counter>>().unwrap();
        inner.value = 11;
    }
    assert_eq!(ptr.lock()?.downcast_ref::<ContainerImpl<Counter>>().unwrap().value, 11);
    assert!(ptr.lock()?.downcast_ref::<ContainerImpl<String>>().is_none());
    assert!(format!("{:?}", ptr).starts_with("ContainerPtr"));
    Ok(())
}

fn add_one(args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(jvalue!(args.get_int("a")? + 1).into())
}

#[test]
fn process_factory_wraps_function() -> JuizResult<()> {
    let f = process_factory(ProcessManifest::new("add_one").add_int_arg("a", "", 0), add_one);
    assert_eq!(f.0.type_name, "add_one");
    let out = (f.1)(vec![("a", jvalue!(41))].into())?;
    assert_eq!(out.as_value(), Some(&jvalue!(42)));
    Ok(())
}

#[test]
fn container_factory_and_container_process_factory() -> JuizResult<()> {
    let cf = container_factory(counter_manifest(), |args: CapsuleMap| -> JuizResult<Box<Counter>> {
        Ok(Box::new(Counter { value: args.get_int("initial")? }))
    });
    assert_eq!(cf.0.type_name, "counter");
    let container = (cf.1)(cf.0.clone(), vec![("initial", jvalue!(5))].into())?;
    // コンストラクタのエラーはそのまま伝わる
    assert!((cf.1)(cf.0.clone(), CapsuleMap::new()).is_err());

    let increment = container_process_factory(
        ProcessManifest::new("increment").add_int_arg("by", "", 1),
        |c: &mut ContainerImpl<Counter>, args: CapsuleMap| -> JuizResult<Capsule> {
            c.value += args.get_int("by")?;
            Ok(jvalue!(c.value).into())
        },
    );
    let out = (increment.1)(container.clone(), vec![("by", jvalue!(2))].into())?;
    assert_eq!(out.as_value(), Some(&jvalue!(7)));
    let out = (increment.1)(container.clone(), vec![("by", jvalue!(3))].into())?;
    assert_eq!(out.as_value(), Some(&jvalue!(10)));

    // 型の違うコンテナに対して呼ぶとダウンキャストに失敗する
    let wrong = container_process_factory(
        ProcessManifest::new("wrong"),
        |_c: &mut ContainerImpl<String>, _args: CapsuleMap| -> JuizResult<Capsule> { Ok(Capsule::empty()) },
    );
    assert!((wrong.1)(container, CapsuleMap::new()).is_err());
    Ok(())
}

#[test]
fn container_stack_factory_receives_parent() -> JuizResult<()> {
    let parent = ContainerPtr::new(ContainerImpl::new(counter_manifest(), Box::new(Counter { value: 100 }))?);
    let stack = container_stack_factory(
        ContainerManifest::new("child").name("child0"),
        |_m: ContainerManifest, parent: ContainerPtr| -> JuizResult<Box<Counter>> {
            let v = parent.lock()?.downcast_ref::<ContainerImpl<Counter>>().unwrap().value;
            Ok(Box::new(Counter { value: v * 2 }))
        },
    );
    let child = (stack.1)(stack.0.clone(), parent)?;
    assert_eq!(child.lock()?.downcast_ref::<ContainerImpl<Counter>>().unwrap().value, 200);
    assert_eq!(child.lock()?.profile()?.type_name, "child");
    Ok(())
}

#[test]
fn container_impl_with_parent() -> JuizResult<()> {
    let parent = ContainerPtr::new(ContainerImpl::new(counter_manifest(), Box::new(Counter { value: 1 }))?);
    let child = ContainerImpl::new_with_parent(ContainerManifest::new("child").name("child0"), Box::new(Counter { value: 2 }), parent)?;
    assert_eq!(child.value, 2);
    assert!(format!("{:?}", child).contains("parent_container: Some"));
    assert!(ContainerImpl::new_with_parent(
        ContainerManifest::new("child"),
        Box::new(Counter { value: 2 }),
        ContainerPtr::new(ContainerImpl::new(counter_manifest(), Box::new(Counter { value: 1 }))?),
    ).is_err());
    Ok(())
}
