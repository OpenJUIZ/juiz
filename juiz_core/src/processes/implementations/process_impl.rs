





use std::sync::Arc;
use juiz_sdk::anyhow;
use juiz_sdk::manifests::ProcessProfile;
use juiz_sdk::process_identifier::ProcessIdentifier;

use crate::connections::{ConnectionFactory, ConnectionFactoryImpl};
use crate::prelude::*;

use juiz_sdk::utils::check_manifest_before_call;
use juiz_sdk::connections::{ConnectionManifest, ConnectionProfile, DestinationConnection, SourceConnection};

//use crate::value::CapsuleMap;
use super::inlet::Inlet;
use super::outlet::Outlet;
use crate::processes::{ProcessBodyFunctionTrait, ProcessBodyFunctionType};
//use crate::manifests::ProcessManifest;

pub struct ProcessImpl {
    // core: ObjectCore,
    // manifest: ProcessManifest,
    profile: ProcessProfile,
    function: Arc<ProcessBodyFunctionTrait>,
    identifier: ProcessIdentifier,
    outlet: Outlet,
    inlets: Vec<Inlet>,
    connection_factory: Box<dyn ConnectionFactory + 'static>,
}

impl std::fmt::Debug for ProcessImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessImpl").field("profile", &self.profile).field("identifier", &self.identifier).field("outlet", &self.outlet).field("inlets", &self.inlets).finish()
    }
}

// pub fn argument_manifest(process_manifest: &ProcessManifest) -> &Vec<ArgumentManifest> {//JuizResult<&Map<String, Value>>{
//     // obj_get_obj(process_manifest, "arguments")
//     &process_manifest.arguments
// }

// pub fn process_from_clousure(manif: ProcessManifest, func: impl Fn(CapsuleMap) -> JuizResult<Capsule> + 'static, connection_factory: Box<impl ConnectionFactory + 'static>) -> JuizResult<impl Process> {
//     ProcessImpl::new_from_clousure(manif, func, connection_factory)
// }

fn _process_from_clousure_new_with_class_name(class_name: JuizObjectClass, manif: ProcessManifest, func: impl Fn(CapsuleMap) -> JuizResult<Capsule> + 'static, connection_factory: Box<impl ConnectionFactory + 'static>) -> JuizResult<impl Process> {
    ProcessImpl::new_from_clousure_and_class_name(class_name, manif, func, connection_factory)
}
     
fn process_new_with_connection_factory(manif: ProcessManifest, func: ProcessBodyFunctionType, connection_factory: Box<impl ConnectionFactory+'static>) -> JuizResult<impl Process> {
    ProcessImpl::new_from_fn(manif, func, connection_factory)
}

pub fn process_new(manif: ProcessManifest, func: ProcessBodyFunctionType) -> JuizResult<impl Process> {
    process_new_with_connection_factory(manif, func, Box::new(ConnectionFactoryImpl::new()))
}
    

impl ProcessImpl {

    pub(crate) fn new_from_clousure_and_class_name(class_name: JuizObjectClass, manif: ProcessManifest, func: impl Fn(CapsuleMap) -> JuizResult<Capsule> + 'static, connection_factory: Box<impl ConnectionFactory + 'static>) -> JuizResult<Self> {
        ProcessImpl::new_from_clousure_ref_and_class_name(class_name, manif, Arc::new(func), connection_factory)
    }

    pub(crate) fn new_from_clousure_ref_and_class_name(_class_name: JuizObjectClass, manifest: ProcessManifest, func: Arc<dyn Fn(CapsuleMap) -> JuizResult<Capsule> + 'static>, connection_factory: Box<impl ConnectionFactory + 'static>) -> JuizResult<Self> {
        log::trace!("ProcessImpl::new_from_clousure_ref_and_class_name(manifest={manifest:})が呼ばれました");
        let process_profile = manifest.clone().try_into()?;
        log::debug!("ProcessImplオブジェクト (prof={process_profile:}) を作成します。");
        Ok(Self{
            //core: ObjectCore::create(class_name, manifest.type_name.clone(), manifest.name.as_ref().unwrap()),
            function: func, 
            identifier: manifest.identifier()?, //identifier_from_manifest("core", "core", "Process", &manifest)?,
            outlet: Outlet::new(manifest.name.as_ref().unwrap().as_str(), manifest.use_memo),
            inlets: Self::create_inlets(&manifest),
            profile: process_profile,
            connection_factory,
        })
    }

    pub fn new_with_class(class_name: JuizObjectClass, manif: ProcessManifest, func: ProcessBodyFunctionType, connection_factory: Box<impl ConnectionFactory + 'static>) -> JuizResult<Self> {
        // log::trace!("ProcessImpl::new_with_class(manifest={manif:})が呼ばれました");
        // log::debug!("ProcessImplオブジェクトを作成します。");
        ProcessImpl::new_from_clousure_and_class_name(class_name, manif, func, connection_factory)
    }

    pub fn new_from_fn(manif: ProcessManifest, func: ProcessBodyFunctionType, connection_factory: Box<impl ConnectionFactory + 'static>) -> JuizResult<Self> {
        Self::new_with_class(JuizObjectClass::Process("ProcessImpl"), manif, func, connection_factory)
    }

    // pub fn new_from_clousure(manif: ProcessManifest, func: impl Fn(CapsuleMap) -> JuizResult<Capsule> + 'static, connection_factory: Box<impl ConnectionFactory + 'static>) -> JuizResult<Self> {
    //     ProcessImpl::new_from_clousure_and_class_name(JuizObjectClass::Process("ProcessImpl"), manif, func, connection_factory)
    // }

    pub fn new_from_clousure_ref(manif: ProcessManifest, func: Arc<dyn Fn(CapsuleMap) -> JuizResult<Capsule> + 'static>, connection_factory: Box<impl ConnectionFactory + 'static>) -> JuizResult<Self> {
        ProcessImpl::new_from_clousure_ref_and_class_name(JuizObjectClass::Process("ProcessImpl"), manif, func, connection_factory)
    }

    fn create_inlets(manifest: &ProcessManifest) -> Vec<Inlet> {
        manifest.arguments.iter().map(|v| {
            Inlet::new(v.name.as_str(), v.default.clone())
        }).collect::<Vec<Inlet>>()
    }

    #[allow(unused)]
    pub fn inlet(&self, name: &str) -> JuizResult<&Inlet> {
        self.inlets.iter().find(|inlet| { (*inlet).name() == name }).ok_or_else(|| { anyhow::Error::from(JuizError::CanNotFindError { target: format!("Process::Inlet({name})") }) } )
    }
    
    pub fn inlet_mut(&mut self, name: &str) -> JuizResult<&mut Inlet> {
        self.inlets.iter_mut().find(|inlet| { (*inlet).name() == name }).ok_or_else(|| { anyhow::Error::from(JuizError::CanNotFindError { target: format!("Process::Inlet({name})") }) } )
    }

    fn collect_values(&self) -> CapsuleMap {
        log::trace!("ProcessImpl({}).collect_values() called.", &self.identifier);
        self.inlets.iter().map(|inlet| { (inlet.name().clone(), inlet.collect_value() )} ).collect::<Vec<(String, CapsulePtr)>>().into()
    }

    /// invokeが起こった時にinletから入力データを収集する関数。
    fn collect_values_exclude(&self, arg_name: &str, arg_value: CapsulePtr) -> CapsuleMap {
        log::trace!("ProcessImpl({}).collect_values_exclude(arg_name={}) called.", &self.identifier, arg_name);
        // excludeされるべきarg_nameでないinletにはcollect_valueをそれぞれ呼び出す。
        self.inlets.iter().map(|inlet| {
            if inlet.name() == arg_name { return (arg_name.to_owned(), arg_value.clone()); }
            return (inlet.name().clone(), inlet.collect_value());
        }).collect::<Vec<(String, CapsulePtr)>>().into()
    }

}

impl Process for ProcessImpl {
    
    fn profile(&self) -> JuizResult<ProcessProfile> { 
        Ok(self.profile.clone())
    }

    fn call(&self, args: CapsuleMap) -> JuizResult<CapsulePtr> {
        // log::trace!("ProcessImpl({})::call(args=**) called", self.identifier());
        log::trace!("ProcessImpl({:})::call({args})が呼ばれました", self.identifier());
        check_manifest_before_call(&(self.profile), &args)?;
        Ok( (self.function)(args)?.into() )
    }

    fn is_updated(&self) -> JuizResult<bool> {
        log::trace!("ProcessImpl({})::is_updated() called now", self.identifier());
        if self.outlet.memo().is_empty()? {
            log::trace!(" - MEMO is empty. Must be updated.");
            return Ok(true)
        }
        for inlet in self.inlets.iter() {
            if inlet.is_updated()? {
                log::trace!(" - Inlet({}) is updated. Must be updated.", inlet.name());
                return Ok(true)
            }
        }
        log::trace!("ProcessImpl({})::is_updated() returned false", self.identifier());
        Ok(false)
    }

    /// invokeする
    /// 
    /// inletから入力を受け取ってcallをして、出力を得る。無事に出力が得られたらmemoに書き込む。
    fn invoke<'b>(&'b self) -> JuizResult<CapsulePtr> {
        log::trace!("Processimpl({})::invoke()が呼ばれました。", self.identifier());
        if self.outlet.memo().is_empty()? || self.is_updated()? {
            log::debug!("【invoke】memoが空か、updatedフラグが立ったので、コネクタからデータ収集します。");
            return Ok(self.outlet.set_value(self.call(self.collect_values())?));
        }
        log::debug!("【invoke】memoが有効です。memoを使います。");
        return Ok(self.outlet.memo().clone());
    }

    /// invokeをするが、inletのうちarg_nameで指定されるものに関してはデータ収集を行わずに引数valueとして受け取った値を入力として使う
    /// 
    /// これは後段のsource_connectionからpushされた場合に使う。
    /// 
    //fn invoke_exclude<'b>(&self, arg_name: &str, value: CapsulePtr) -> JuizResult<CapsulePtr> {
    //    log::trace!("Processimpl({})::invoke_exclude(arg_name={}) called", self.identifier(), arg_name);
    //    // invoke_excludeは後ろからpushされた時にのみ呼ばれるので、必ず引数はupdateされている。なのでcallする。
    //    return Ok(self.outlet.set_value(self.call(self.collect_values_exclude(arg_name, value))?));
    //}

    fn execute(&self) -> JuizResult<CapsulePtr> {
        log::trace!("Processimpl({})::execute()が呼ばれました。", self.identifier());
        self.outlet.push(self.invoke()?)
    }

    fn push_by(&self, arg_name: &str, value: CapsulePtr) -> JuizResult<CapsulePtr> {
        log::trace!("ProcessImpl::push_by({}) called", self.identifier());
        let v = self.outlet.set_value(self.call(self.collect_values_exclude(arg_name, value))?);
        self.outlet.push(v)
    }
    
    fn get_output(&self) -> CapsulePtr {
        self.outlet.memo().clone()
    }

    fn notify_connected_from(&mut self, source: ProcessPtr, connection_manifest: &ConnectionManifest) -> JuizResult<ConnectionProfile> {
        log::trace!("ProcessImpl(id={:?}).notify_connected_from(source=Process()) called", self.identifier());
        let _id = self.identifier().clone();
        let con = self.connection_factory.create_source_connection(
            source, connection_manifest.clone());
        self.inlet_mut(&connection_manifest.arg_name)?.insert(
            con
            );
        log::trace!("ProcessImpl(id={:?}).notify_connected_from(source=Process()) exit", self.identifier());
        Ok(connection_manifest.clone().into())
    }

    fn try_connect_to(&mut self, destination: ProcessPtr, connection_manifest: &ConnectionManifest) -> JuizResult<ConnectionManifest> {
        log::trace!("ProcessImpl(id={:?}).try_connect_to(destination=Process()) called", self.identifier());
        // let destination_id = destination.identifier().clone();
        let con = self.connection_factory.create_destination_connection(
            destination, connection_manifest.clone());
        self.outlet.insert(
            connection_manifest.arg_name.clone(), 
            con);
        log::trace!("ProcessImpl(id={:?}).try_connect_to(destination=Process()) exit", self.identifier());
        Ok(connection_manifest.clone())
    }

    
    fn source_connections(&self) -> JuizResult<Vec<&Box<dyn SourceConnection>>> {
        Ok(self.inlets.iter().map(|inlet| { inlet.source_connections() } ).flatten().collect::<Vec<&Box<dyn SourceConnection>>>())
    }
    

    fn destination_connections(&self) -> JuizResult<Vec<&Box<dyn DestinationConnection>>> {
        self.outlet.destination_connections()
    }
    
    fn p_apply(&mut self, arg_name: &str, value: CapsulePtr) -> JuizResult<CapsulePtr> {
        self.inlet_mut(arg_name)?.bind(value)
    }
    
    fn purge(&mut self) -> JuizResult<()> {
        log::trace!("ProcessImpl({})::purge() called", self.identifier());
        Ok(())
    }
    
    fn identifier(&self) -> ProcessIdentifier {
        self.identifier.clone()
    }
}

impl Drop for ProcessImpl {
    fn drop(&mut self) {
        log::info!("ProcessImpl({})::drop() called", self.identifier());
        log::trace!("ProcessImpl({})::drop() exit", self.identifier());
    }
}

unsafe impl Send for ProcessImpl {

}

unsafe impl Sync for ProcessImpl {

}