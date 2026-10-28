//! QUIC (qmp) ブローカー
//!
//! 要求ごとに双方向ストリームを開き、要求のフレームを 1 つ書いて閉じ、応答のフレームを 1 つ読む。
//! フレームは IPC ブローカーと同じ生バイトの形式 (docs/spec/large-data.md §3、先頭は "JZF1") で、
//! 画像は PNG にせず画素をそのまま送る。0.0.x の MessagePack の qmp とは互換が無い。

use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex};

use juiz_core::{
    anyhow::{self, anyhow},
    frame::FrameLimits,
    futures,
    prelude::*,
    {create_broker_factory_impl, create_broker_proxy_factory_impl, CRUDBroker, CRUDBrokerHolder},
};

mod qmp_broker;
mod qmp_broker_proxy;

/// qmp の CRUD を直接使うとき (list の namespace 絞り込みなど BrokerProxy に無い引数を渡すとき) に使う
pub use qmp_broker_proxy::QuicBrokerProxy;

/// ストリームを読むときのバッファの大きさ。本体の大きな読み込みはバッファを通さずに直接読む
pub(crate) const READ_BUFFER_BYTES: usize = 64 * 1024;

/// manifest の max_frame_bytes から、フレームを読むときの上限を求める (docs/spec/large-data.md §3.5)
pub(crate) fn frame_limits(manifest: &Value) -> FrameLimits {
    let mut limits = FrameLimits::default();
    if let Some(max) = manifest.get("max_frame_bytes").and_then(|v| v.as_u64()) {
        limits.max_frame_bytes = max;
    }
    limits
}

/// quinn の受信ストリームを std::io::Read として使う。
///
/// フレームの読み書き (juiz_core::frame) は同期の Read / Write で書かれているので、非同期の
/// 読み込みを block_on で待つ。tokio の非同期タスクの中では使わない (spawn_blocking かふつうのスレッドで使う)。
/// read は受け取ったデータを渡されたバッファへ直接写すので、画像の本体は最終的な領域へ読み込まれる。
pub(crate) struct BlockingRecv(pub quinn::RecvStream);

impl Read for BlockingRecv {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match futures::executor::block_on(self.0.read(buf)) {
            Ok(Some(n)) => Ok(n),
            Ok(None) => Ok(0),
            Err(e) => Err(e.into()),
        }
    }
}

/// quinn の送信ストリームを std::io::Write として使う (BlockingRecv と同じく block_on で待つ)
pub(crate) struct BlockingSend(pub quinn::SendStream);

impl Write for BlockingSend {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        futures::executor::block_on(self.0.write(buf)).map_err(|e| e.into())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl BlockingSend {
    /// フレームを書き終えたことを相手に伝える
    pub(crate) fn finish(mut self) -> anyhow::Result<()> {
        self.0.finish().map_err(|e| anyhow!(e))
    }
}

pub async fn on_start(broker_manifest: Value, crud_broker: Arc<Mutex<CRUDBroker>>) -> () {
    qmp_broker::on_start(broker_manifest, crud_broker).await
}

/// qmp ブローカーのファクトリを作る
pub fn qmp_broker_factory(core_broker: CoreBrokerPtr) -> JuizResult<Arc<Mutex<dyn BrokerFactory>>> {
    fn create_broker_function(core_broker: CoreBrokerPtr, manifest: Value) -> JuizResult<BrokerPtr> {
        Ok(BrokerPtr::new(CRUDBrokerHolder::new(
            "QuinnBroker",
            "qmp",
            core_broker,
            &on_start,
            manifest.clone(),
        )?))
    }

    let manifest = jvalue!({
        "type_name": "qmp"
    });

    create_broker_factory_impl(core_broker, manifest, create_broker_function)
}

/// qmp ブローカープロキシのファクトリを作る
pub fn qmp_broker_proxy_factory() -> JuizResult<Arc<Mutex<dyn BrokerProxyFactory>>> {
    let manifest = jvalue!({
        "type_name": "qmp"
    });
    create_broker_proxy_factory_impl(manifest, qmp_broker_proxy::create_broker_proxy_function)
}

/// qmp ブローカーを組み込みのブローカーとして登録する。これ以降に setup() する System は、
/// 設定ファイルの plugins.broker_factories でプラグインを読み込まなくても qmp を使える。
pub fn register() {
    juiz_core::register_builtin_broker_factory(qmp_broker_factory, qmp_broker_proxy_factory);
}

// 以下はプラグイン(libqmp_broker.so など)として読み込まれたときの入口

#[no_mangle]
pub unsafe extern "Rust" fn broker_factory(
    core_broker: CoreBrokerPtr,
) -> JuizResult<Arc<Mutex<dyn BrokerFactory>>> {
    let _ = juiz_core::env_logger::try_init();
    qmp_broker_factory(core_broker)
}

#[no_mangle]
pub unsafe extern "Rust" fn broker_proxy_factory() -> JuizResult<Arc<Mutex<dyn BrokerProxyFactory>>>
{
    qmp_broker_proxy_factory()
}
