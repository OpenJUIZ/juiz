//! プロセスの接続に関する機能パッケージ
//! 

// mod connection_core;
pub mod connection;
mod connection_type;
mod connection_manifest;
mod connection_profile;

pub mod destination_connection;
pub mod source_connection;

// pub use connection_core::ConnectionCore;
pub use connection::Connection;
pub use connection_manifest::ConnectionManifest;
pub use connection_profile::ConnectionProfile;
pub use connection_type::ConnectionType;
pub use source_connection::SourceConnection;
pub use destination_connection::DestinationConnection;
