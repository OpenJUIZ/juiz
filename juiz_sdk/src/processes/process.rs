use crate::connections::{
    ConnectionManifest, ConnectionProfile, DestinationConnection, SourceConnection,
};
use crate::manifests::ProcessProfile;
use crate::prelude::*;
use crate::process_identifier::ProcessIdentifier;
use mopa::mopafy;

use super::ProcessPtr;

pub type ProcessBodyFunctionType = fn(CapsuleMap) -> JuizResult<Capsule>;
pub type ProcessBodyFunctionTrait = dyn Fn(CapsuleMap) -> JuizResult<Capsule>;

pub trait Process: Send + Sync + mopa::Any + std::fmt::Debug + 'static {
    fn call(&self, _args: CapsuleMap) -> JuizResult<CapsulePtr>;

    fn is_updated(&self) -> JuizResult<bool>;

    //fn is_updated_exclude(& self, inlet_name: &str) -> JuizResult<bool>;

    fn identifier(&self) -> ProcessIdentifier;

    fn profile(&self) -> JuizResult<ProcessProfile>;

    // fn profile_full(&self) -> JuizResult<Value>;
    /*
     * invokeは自身の入力側接続をすべてinvokeしてデータを収集した後に、収集したデータで自身をcallして結果を返す。
     * 出力はmemo化されるので、配下がupdateされなければメモを返す。
     * この方法は配下すべてに問い合わせが波及するので、updateされたかどうかをconnectionにpushする仕組みが必要
     * TODO:
     */
    fn invoke<'b>(&self) -> JuizResult<CapsulePtr>;

    //fn invoke_exclude<'b>(&self, arg_name: &str, value: CapsulePtr) -> JuizResult<CapsulePtr>;

    /*
     * executeは自信をinvokeしてから、自分の出力側接続先をすべてexecuteする。
     * memo化があるので自身を2度実行はしないはず (スレッドやマルチプロセスがあると問題があるので、invoke_excludeを実装すべきだ)
     * TODO:
     *
     * 自分の配下はinvokeによってinvokeされるが、配下の枝分かれ先はexecuteされない
     */
    fn execute(&self) -> JuizResult<CapsulePtr>;

    fn push_by(&self, arg_name: &str, value: CapsulePtr) -> JuizResult<CapsulePtr>;

    fn get_output(&self) -> CapsulePtr;

    fn notify_connected_from<'b>(
        &'b mut self,
        source: ProcessPtr,
        connection_manifest: &ConnectionManifest,
    ) -> JuizResult<ConnectionProfile>;

    fn try_connect_to(
        &mut self,
        target: ProcessPtr,
        connection_manifest: &ConnectionManifest,
    ) -> JuizResult<ConnectionManifest>;

    fn source_connections(&self) -> JuizResult<Vec<&Box<dyn SourceConnection>>>;

    fn destination_connections(&self) -> JuizResult<Vec<&Box<dyn DestinationConnection>>>;

    fn p_apply(&mut self, arg_name: &str, value: CapsulePtr) -> JuizResult<CapsulePtr>;

    fn purge(&mut self) -> JuizResult<()>;

    fn openapi_spec(&self) -> JuizResult<Value>;
}

mopafy!(Process);
