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
use juiz_sdk::geometry::{Pose, PoseStamped, Quaternion, Vector3};
use juiz_sdk::manifests::{PoseListenerManifest, TransformManifest};
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

    /// サブシステムの登録を外す (issue #303)
    ///
    /// System が終わるときに、つないでいる相手 (マスターシステムとサブシステム) へ自分の UUID を渡して呼ぶ。
    /// 相手は subsystems の一覧・マスターシステム・Topic の転送先からその System を外す。
    fn system_remove_subsystem(&mut self, uuid: Uuid) -> JuizResult<Value>;

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

    /// Topic の push で共有メモリ (B) を使えるか。IPC だけ (docs/spec/large-data.md §6.1)
    fn supports_shared_memory(&self) -> bool {
        false
    }

    /// 共有メモリに書いた本体の場所を送る Topic の push。supports_shared_memory が true のときだけ呼ぶ
    #[cfg(feature = "ipc-broker")]
    fn topic_push_shared(
        &self,
        name: &str,
        _push: &crate::topics::shared_memory::SharedPush,
        _pushed_system: Option<Uuid>,
    ) -> JuizResult<()> {
        Err(juiz_sdk::anyhow::anyhow!(
            "topic_push_shared({name}) is not supported by this broker proxy"
        ))
    }

    /// Topic をSubscribeする必要があるか問い合わせる
    /// {"subscribe": true/false } が返る
    fn topic_request_subscribe(
        &mut self,
        name: &str,
        system_uuid: Option<Uuid>,
    ) -> JuizResult<Value> {
        self.topic_request_subscribe_via(name, system_uuid, &[])
    }

    /// topic_request_subscribe の転送版。`via` はこの問い合わせが通ってきた System の UUID の列で、
    /// 列にある System には転送しない (サブシステムのつなぎ方が輪でも止まる、issue #449)
    fn topic_request_subscribe_via(
        &mut self,
        name: &str,
        system_uuid: Option<Uuid>,
        via: &[Uuid],
    ) -> JuizResult<Value>;

    /// Topic をPublishする必要があるか問い合わせる
    /// {"publish": true/false } が返る
    fn topic_request_publish(
        &mut self,
        name: &str,
        system_uuid: Option<Uuid>,
    ) -> JuizResult<Value> {
        self.topic_request_publish_via(name, system_uuid, &[])
    }

    /// topic_request_publish の転送版。`via` は topic_request_subscribe_via と同じ (issue #449)
    fn topic_request_publish_via(
        &mut self,
        name: &str,
        system_uuid: Option<Uuid>,
        via: &[Uuid],
    ) -> JuizResult<Value>;
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

/// Transformations (Container の位置姿勢と座標変換) の API (issue #370、仕様 docs/spec/transformations.md §13.1)
///
/// 登録所はその juiz プロセスだけのもので、サブシステムはたどらない (§15)。
/// Container・Process は ID か名前で指定する。名前は根のネームスペースから解決する (`robot1/base0`、`base0::mobile_base` も可)。
/// Anchor の作成・削除は Container の API (`container_create` の type_name が `periodic_anchor` / `static_anchor`)。
pub trait TransformationsBrokerProxy {
    /// Container `identifier` の、基準 `reference` から見た ContainerPose (§7.1)。
    /// `identifier` が別 System の Container (ブローカー付きの ID) なら、その System に回す (§19.8)
    fn container_pose(&self, identifier: &str, reference: &str) -> JuizResult<PoseStamped> {
        self.container_pose_from_system(identifier, reference, None)
    }

    /// [`Self::container_pose`] の、基準を System `reference_system` (UUID) の写しから探すもの (§19.5 の 4・§19.8)。
    /// `None` なら [`Self::container_pose`] と同じ
    fn container_pose_from_system(
        &self,
        identifier: &str,
        reference: &str,
        reference_system: Option<&str>,
    ) -> JuizResult<PoseStamped>;

    /// Container の PoseState を版番号つきで返す (`container/pose_state`、§19.4)。
    /// 別 System の写しを取り込む System が呼ぶ。戻り値は [`crate::transformations::api_types::PoseStateResponse`] の形
    fn container_pose_state(
        &self,
        query: &crate::transformations::api_types::PoseStateQuery,
    ) -> JuizResult<Value>;

    /// Container `identifier` を、基準 `reference` から見て `pose` にする (§7.2)。書いた後の値を返す
    fn container_set_pose(
        &self,
        identifier: &str,
        reference: &str,
        pose: &Pose,
    ) -> JuizResult<PoseStamped> {
        self.container_set_pose_from_system(identifier, reference, None, pose)
    }

    /// [`Self::container_set_pose`] の、基準を System `reference_system` の写しから探すもの (§19.8)
    fn container_set_pose_from_system(
        &self,
        identifier: &str,
        reference: &str,
        reference_system: Option<&str>,
        pose: &Pose,
    ) -> JuizResult<PoseStamped>;

    /// Transform の ID (`transform://<始点の ID>|<終点の ID>`) の一覧
    fn transform_list(&self) -> JuizResult<Vec<String>>;

    /// `{identifier, source, destination, transform, stamp}`。System をまたぐ Transform には `remote` (§19.10)。
    /// ID の終点が別 System の Container なら、その System に回す (§19.2 の 7)
    fn transform_profile_full(&self, identifier: &str) -> JuizResult<Value> {
        self.transform_profile_full_from_system(identifier, None)
    }

    /// 別 System `source_system` (UUID) から回された [`Self::transform_profile_full`]。
    /// ID の始点は `source_system` での core の ID (§19.2 の 7)
    fn transform_profile_full_from_system(
        &self,
        identifier: &str,
        source_system: Option<&str>,
    ) -> JuizResult<Value>;

    /// Transform を作り、profile を返す。木の規則 (§5.2) を破ればエラーで何も作らない。
    /// 終点が別 System の Container なら、その System に回して作る (§19.2 の 5)
    fn transform_create(&self, manifest: &TransformManifest) -> JuizResult<Value>;

    /// Transform の値を変える (§7.3)。`None` のものは今のまま
    fn transform_update(
        &self,
        identifier: &str,
        translation: Option<Vector3>,
        rotation: Option<Quaternion>,
    ) -> JuizResult<Value> {
        self.transform_update_from_system(identifier, None, translation, rotation)
    }

    /// 別 System `source_system` から回された [`Self::transform_update`] (§19.2 の 7)
    fn transform_update_from_system(
        &self,
        identifier: &str,
        source_system: Option<&str>,
        translation: Option<Vector3>,
        rotation: Option<Quaternion>,
    ) -> JuizResult<Value>;

    /// Transform を消し、消したものの profile を返す
    fn transform_destroy(&self, identifier: &str) -> JuizResult<Value> {
        self.transform_destroy_from_system(identifier, None)
    }

    /// 別 System `source_system` から回された [`Self::transform_destroy`] (§19.2 の 7)
    fn transform_destroy_from_system(
        &self,
        identifier: &str,
        source_system: Option<&str>,
    ) -> JuizResult<Value>;

    /// Anchor の ID の一覧
    fn anchor_list(&self) -> JuizResult<Vec<String>>;

    /// Anchor の木を今すぐ更新する (§4.4 の 3)。`{identifier, stamp}`
    fn anchor_update(&self, identifier: &str) -> JuizResult<Value>;

    /// Anchor の木 `{identifier, pose, transform?, children: [...]}` の入れ子
    fn transformations_tree(&self, anchor: &str) -> JuizResult<Value>;

    /// Pose 通知の ID (`pose_listener://<Container の ID>|<Process の ID>|<arg>`) の一覧
    fn pose_listener_list(&self) -> JuizResult<Vec<String>>;

    /// `{identifier, container, process, arg, reference, on_update, initial}`
    fn pose_listener_profile_full(&self, identifier: &str) -> JuizResult<Value>;

    /// Pose 通知を登録し、profile を返す (§8.3)
    fn pose_listener_create(&mut self, manifest: &PoseListenerManifest) -> JuizResult<Value>;

    /// Pose 通知の登録を外し、外したものの profile を返す
    fn pose_listener_destroy(&self, identifier: &str) -> JuizResult<Value>;
}

pub trait BrokerProxy:
    Send
    + Sync
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
    + TransformationsBrokerProxy
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
