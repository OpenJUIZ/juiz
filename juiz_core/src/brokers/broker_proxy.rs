use juiz_sdk::{
    connection_identifier::ConnectionIdentifier,
    connections::ConnectionProfile,
    manifests::ContainerIdentifier,
    manifests::{CompositeManifest, ContainerProfile, ProcessProfile},
    process_identifier::ProcessIdentifier,
    topic_identifier::TopicIdentifier,
};
use uuid::Uuid;

use crate::prelude::*;
use crate::types::TypeEntry;
use juiz_sdk::types::TypeSchema;

use std::path::PathBuf;

pub trait SystemBrokerProxy {
    /// Systemのプロファイルを取得する
    ///
    /// System登録のプロセスやコンテナの情報を全て列挙したデータを取得する
    ///
    fn system_profile_full(&self) -> JuizResult<Value>;

    /// サブシステムの中身まで含めた System のプロファイルを取得する (issue #116)
    ///
    /// system_profile_full() の subsystems は、サブシステムへの BrokerProxy の profile だけになっている。
    /// こちらは各サブシステムの system_profile_full_recursive() を入れ子にし、
    /// 各 System が持つ接続の profile を connections に加える。
    fn system_profile_full_recursive(&self) -> JuizResult<Value> {
        self.system_profile_full()
    }

    ///
    ///
    ///
    fn system_filesystem_list(&self, path_buf: PathBuf) -> JuizResult<Value>;

    /// サブシステムを追加登録する
    ///
    fn system_add_subsystem(&mut self, profile: Value) -> JuizResult<Value>;

    /// マスターシステムを追加登録する
    ///
    ///
    fn system_add_mastersystem(&mut self, profile: Value) -> JuizResult<Value>;

    /// SystemのUUIDを取得する
    ///
    ///
    fn system_uuid(&self) -> JuizResult<Value>;

    /// Processのfactoryをファイルシステムからロードする
    ///
    ///
    fn system_load_process(&mut self, language: String, filepath: String) -> JuizResult<Value>;

    /// Containerのfactoryをファイルシステムからロードする
    ///
    ///
    fn system_load_container(&mut self, language: String, filepath: String) -> JuizResult<Value>;

    /// ContainerProcessのfactoryをファイルシステムからロードする
    ///
    ///
    fn system_load_container_process(
        &mut self,
        language: String,
        filepath: String,
    ) -> JuizResult<Value>;

    /// Componentのfactoryをファイルシステムからロードする
    ///
    ///
    fn system_load_component(
        &mut self,
        language: String,
        filepath: String,
    ) -> JuizResult<ComponentManifest>;
}

pub trait ProcessBrokerProxy {
    /// プロセス作成。
    /// 引数はマニフェスト
    /// type_name, nameが最低限の引数。
    /// use_memoはオプション
    fn process_create(&mut self, manifest: &ProcessManifest) -> JuizResult<ProcessProfile>;

    fn process_destroy(&mut self, identifier: &ProcessIdentifier) -> JuizResult<ProcessProfile>;

    /// プロセスリスト取得
    ///
    /// Broker支配下のプロセスのIDのリストを取得する
    ///
    /// * `recursive` - サブシステムのプロセスを再起的に読み込む場合はtrue
    fn process_list(
        &self,
        recursive: bool,
        caller_broker_profile: Option<Value>,
    ) -> JuizResult<Vec<ProcessIdentifier>>;

    fn process_profile_full(&self, id: &ProcessIdentifier) -> JuizResult<ProcessProfile>;

    /// プロセスをCallする
    ///
    /// * id: プロセスのID
    /// * args: 引数
    fn process_call(&self, id: &ProcessIdentifier, args: CapsuleMap) -> JuizResult<CapsulePtr>;

    /// プロセスをExecuteする
    ///
    /// * id: プロセスのID
    fn process_execute(&self, id: &ProcessIdentifier) -> JuizResult<CapsulePtr>;

    fn process_push_by(
        &self,
        id: &ProcessIdentifier,
        arg_name: String,
        value: CapsulePtr,
    ) -> JuizResult<CapsulePtr>;

    fn process_try_connect_to(
        &mut self,
        manifest: &ConnectionManifest,
    ) -> JuizResult<ConnectionManifest>;

    fn process_notify_connected_from(
        &mut self,
        manifest: &ConnectionManifest,
    ) -> JuizResult<ConnectionProfile>;

    fn process_p_apply(
        &mut self,
        id: &ProcessIdentifier,
        arg_name: &str,
        value: CapsulePtr,
    ) -> JuizResult<CapsulePtr>;

    fn process_openapi_spec(&self, id: &ProcessIdentifier) -> JuizResult<Value>;
}

pub trait ContainerBrokerProxy {
    fn container_create(
        &mut self,
        manifest: &ContainerManifest,
        args: CapsuleMap,
    ) -> JuizResult<ContainerProfile>;

    fn container_destroy(
        &mut self,
        identifier: &ContainerIdentifier,
    ) -> JuizResult<ContainerProfile>;

    /// コンテナリスト取得
    ///
    /// Broker支配下のプロセスのIDのリストを取得する
    ///
    /// * `recursive` - サブシステムのプロセスを再起的に読み込む場合はtrue
    fn container_list(
        &self,
        recursive: bool,
        caller_broker_profile: Option<Value>,
    ) -> JuizResult<Vec<ContainerIdentifier>>;

    fn container_profile_full(&self, id: &ContainerIdentifier) -> JuizResult<ContainerProfile>;
}

pub trait ContainerProcessBrokerProxy {
    fn container_process_create(
        &mut self,
        container_id: &ContainerIdentifier,
        manifest: &ProcessManifest,
    ) -> JuizResult<ProcessProfile>;

    fn container_process_destroy(
        &mut self,
        identifier: &ProcessIdentifier,
    ) -> JuizResult<ProcessProfile>;

    /// コンテナプロセスのリスト取得
    ///
    /// Broker支配下のプロセスのIDのリストを取得する
    ///
    /// * `recursive` - サブシステムのプロセスを再起的に読み込む場合はtrue
    fn container_process_list(
        &self,
        rucursive: bool,
        caller_broker_profile: Option<Value>,
    ) -> JuizResult<Vec<ProcessIdentifier>>;

    fn container_process_profile_full(&self, id: &ProcessIdentifier) -> JuizResult<ProcessProfile>;

    fn container_process_call(
        &self,
        id: &ProcessIdentifier,
        _args: CapsuleMap,
    ) -> JuizResult<CapsulePtr>;

    fn container_process_execute(&self, id: &ProcessIdentifier) -> JuizResult<CapsulePtr>;

    fn container_process_p_apply(
        &mut self,
        id: &ProcessIdentifier,
        arg_name: &str,
        value: CapsulePtr,
    ) -> JuizResult<CapsulePtr>;

    fn container_process_openapi_spec(&self, id: &ProcessIdentifier) -> JuizResult<Value>;
}

pub trait ExecutionContextBrokerProxy {
    fn ec_create(&mut self, manifest: &Value) -> JuizResult<Value>;

    fn ec_destroy(&mut self, identifier: &Identifier) -> JuizResult<Value>;

    /// 実行コンテキストのリスト取得
    ///
    /// Broker支配下のプロセスのIDのリストを取得する
    ///
    /// * `recursive` - サブシステムのプロセスを再起的に読み込む場合はtrue
    fn ec_list(&self, recursive: bool) -> JuizResult<Value>;

    fn ec_profile_full(&self, id: &Identifier) -> JuizResult<Value>;

    fn ec_get_state(&self, id: &Identifier) -> JuizResult<Value>;

    fn ec_start(&mut self, id: &Identifier) -> JuizResult<Value>;

    fn ec_stop(&mut self, id: &Identifier) -> JuizResult<Value>;
}
pub trait BrokerBrokerProxy {
    /// ブローカのリスト取得
    ///
    /// Broker支配下のプロセスのIDのリストを取得する
    ///
    /// * `recursive` - サブシステムのプロセスを再起的に読み込む場合はtrue
    fn broker_list(&self, recursive: bool) -> JuizResult<Vec<String>>;

    fn broker_profile_full(&self, id: &Identifier) -> JuizResult<BrokerProfile>;
}

pub trait ConnectionBrokerProxy {
    /// Connectionのリスト取得
    ///
    /// Broker支配下のプロセスのIDのリストを取得する
    ///
    /// * `recursive` - サブシステムのプロセスを再起的に読み込む場合はtrue
    fn connection_list(&self, recursive: bool) -> JuizResult<Vec<ConnectionIdentifier>>;

    fn connection_profile_full(&self, id: &ConnectionIdentifier) -> JuizResult<ConnectionProfile>;

    fn connection_create(&mut self, manifest: &ConnectionManifest)
        -> JuizResult<ConnectionProfile>;

    fn connection_destroy(&mut self, id: &ConnectionIdentifier) -> JuizResult<ConnectionProfile>;
}

pub trait TopicBrokerProxy {
    fn topic_list(&self) -> JuizResult<Vec<TopicIdentifier>>;

    fn topic_push(
        &self,
        name: &str,
        capsule: CapsulePtr,
        pushed_system: Option<Uuid>,
    ) -> JuizResult<()>;

    /// Topic をSubscribeする必要があるか問い合わせる
    /// {"subscribe": true/false } が返る
    fn topic_request_subscribe(
        &mut self,
        name: &str,
        system_uuid: Option<Uuid>,
    ) -> JuizResult<Value>;

    /// Topic をPublishする必要があるか問い合わせる
    /// {"subscribe": true/false } が返る
    fn topic_request_publish(&mut self, name: &str, system_uuid: Option<Uuid>)
        -> JuizResult<Value>;
}

/// 実行中に Composite を定義する API (issue #171、仕様 docs/spec/composition.md §9)
///
/// Composite の Factory はその juiz プロセスだけのもので、サブシステムはたどらない。
/// CompositeContainer の削除と一覧は Container と同じ (`container_destroy` / `container_list`)。
pub trait CompositeBrokerProxy {
    /// Composite の Factory を登録し、Factory の manifest (`composition` を含む) を返す。
    /// スロット (`containers`) があれば CompositeContainerProcess の型になる
    fn composite_create(&mut self, manifest: &CompositeManifest) -> JuizResult<ProcessManifest>;

    /// 登録済みの Composite の型名 (名前順)
    fn composite_list(&self) -> JuizResult<Vec<String>>;

    /// Composite の Factory の manifest
    fn composite_profile_full(&self, type_name: &str) -> JuizResult<ProcessManifest>;

    /// Composite の Factory の登録を外す。インスタンスや、子として使う Composite があればエラー
    fn composite_destroy(&mut self, type_name: &str) -> JuizResult<ProcessManifest>;

    /// CompositeContainer とその上の CompositeContainerProcess を作る。
    /// `manifest` は conf の `composite_containers:` の 1 件と同じ形 (`name`, `namespace`, `slots`, `processes`)
    fn composite_container_create(&mut self, manifest: &Value) -> JuizResult<ContainerProfile>;
}

/// 型の登録所の API (issue #193、仕様 docs/spec/datatype.md §19)
///
/// 登録所はその juiz プロセスだけのもので、サブシステムはたどらない。
pub trait TypeBrokerProxy {
    /// 登録しているすべての名前付き型 (識別子の順)
    fn type_list(&self) -> JuizResult<Vec<TypeEntry>>;

    /// 型参照。`identifier` は `<完全名>@sha256:…` か完全名だけ (同じ完全名の型が 1 つのときだけ)
    fn type_schema(&self, identifier: &str) -> JuizResult<TypeSchema>;
}

pub trait BrokerProxy:
    Send
    + JuizObject
    + SystemBrokerProxy
    + ProcessBrokerProxy
    + ContainerBrokerProxy
    + ContainerProcessBrokerProxy
    + ExecutionContextBrokerProxy
    + BrokerBrokerProxy
    + ConnectionBrokerProxy
    + TopicBrokerProxy
    + CompositeBrokerProxy
    + TypeBrokerProxy
{
    fn is_in_charge_for_process(&self, _id: &Identifier) -> JuizResult<bool>;

    fn any_process_list(
        &self,
        recursive: bool,
        caller_broker_profile: Option<Value>,
    ) -> JuizResult<Vec<ProcessIdentifier>> {
        let mut processes = self.process_list(recursive, caller_broker_profile.clone())?;
        let mut container_processes =
            self.container_process_list(recursive, caller_broker_profile)?;
        processes.append(&mut container_processes);
        Ok(processes)
        //Ok(value_merge(processes, &container_processes)?.into())
    }

    fn any_process_profile_full(&self, id: &ProcessIdentifier) -> JuizResult<ProcessProfile> {
        log::info!("BrokerProxy::any_process_profile_full({id}) called");
        //let id_struct = IdentifierStruct::try_from(id.clone())?;
        //log::info!("id_struct{:?}", id_struct);
        if id.class_name == "process" {
            return self.process_profile_full(id);
        }
        self.container_process_profile_full(id)
    }

    fn any_process_call(&self, id: &ProcessIdentifier, args: CapsuleMap) -> JuizResult<CapsulePtr> {
        log::info!("BrokerProxy::any_process_profile_call({id}) called");
        //let id_struct = IdentifierStruct::try_from(id.clone())?;
        if id.class_name == "process" {
            return self.process_call(id, args);
        }
        self.container_process_call(id, args)
    }
}
