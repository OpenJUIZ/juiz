//! ベンチマーク(`#[ignore]`、release で手動実行。CONTRIBUTING.md の「ベンチマークを実行する」)

mod composite;
mod http_concurrency;
#[cfg(feature = "ipc-broker")]
mod image_transfer;
mod rust_abi;
mod transformations;
