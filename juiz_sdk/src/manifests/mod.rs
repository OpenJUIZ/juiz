//! プロセスやコンテナのバイナリの機能記述 (マニフェスト) に関する機能パッケージ
//!

mod process_manifest;
mod process_profile;

mod container_manifest;
mod container_profile;

mod argument_manifest;
mod argument_profile;
mod argument_type;

mod manifest_description;
// pub mod container_process_manifest;
mod broker_identifier;
mod broker_manifest;
mod broker_profile;
mod component_manifest;
mod container_identifier;
mod output_profile;
mod topic_manifest;
mod topic_profile;

pub use argument_manifest::ArgumentManifest;
pub use argument_profile::ArgumentProfile;
pub use argument_type::ArgumentType;
pub use broker_identifier::BrokerIdentifier;
pub use broker_manifest::BrokerManifest;
pub use broker_profile::BrokerProfile;
pub use component_manifest::ComponentManifest;
pub use container_identifier::ContainerIdentifier;
pub use container_manifest::ContainerManifest;
pub use container_profile::ContainerProfile;
pub use manifest_description::Description;
pub use output_profile::{OutputProfile, PrimitiveProfile, StructProfile};
pub use process_manifest::ProcessManifest;
pub use process_profile::ProcessProfile;
pub use topic_manifest::TopicManifest;
pub use topic_profile::TopicProfile;
