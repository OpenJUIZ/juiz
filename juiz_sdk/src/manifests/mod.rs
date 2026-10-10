//! プロセスやコンテナのバイナリの機能記述 (マニフェスト) に関する機能パッケージ
//! 

mod process_manifest;
mod process_profile;

mod container_manifest;
mod container_profile;

mod argument_type;
mod argument_manifest;
mod argument_profile;

mod manifest_description;
// pub mod container_process_manifest;
mod component_manifest;
mod topic_manifest;
mod topic_profile;
mod container_identifier;

pub use container_identifier::ContainerIdentifier;
pub use container_manifest::ContainerManifest;
pub use container_profile::ContainerProfile;
pub use process_manifest::ProcessManifest;
pub use process_profile::ProcessProfile;
pub use component_manifest::ComponentManifest;
pub use topic_manifest::TopicManifest;
pub use topic_profile::TopicProfile;
pub use argument_type::ArgumentType;
pub use argument_profile::ArgumentProfile;
pub use argument_manifest::ArgumentManifest;
pub use manifest_description::Description;
