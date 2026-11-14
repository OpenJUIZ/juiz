//! フレームを Zenoh の payload (ZBytes) にする。feature shared-memory のときは、大きなフレームを
//! Zenoh の共有メモリの領域へ直接書く (docs/spec/zenoh-broker.md §11)
//!
//! 共有メモリの領域は Zenoh のセッションが持つ provider (transport_optimization の pool) から借りる。
//! 相手が同じホストで共有メモリを使えるなら Zenoh は領域の場所だけを送り、そうでなければ Zenoh が
//! 中身を TCP で送る。どちらになるかは Zenoh がセッションを開くときに決める。

use juiz_core::anyhow::anyhow;
use juiz_core::frame::OutgoingFrame;
use juiz_core::prelude::*;
use zenoh::bytes::ZBytes;
use zenoh::Session;

/// manifest に shm_pool_bytes が無いときの共有メモリの pool の大きさ (4K の RGB のフレームが 2 つ入る)
pub(crate) const DEFAULT_SHM_POOL_BYTES: u64 = 64 << 20;
/// これより小さなフレームは共有メモリに書かない (領域を借りる手間の方が大きいため)
#[cfg_attr(not(feature = "shared-memory"), allow(dead_code))]
pub(crate) const SHM_THRESHOLD: usize = 64 << 10;

/// manifest の shared_memory (省くと true)。feature shared-memory が無いときは常に false
pub(crate) fn shm_enabled(manifest: &Value) -> bool {
    cfg!(feature = "shared-memory")
        && manifest
            .get("shared_memory")
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
}

/// 共有メモリを使えるか (§11.3)。manifest で切っていなくても、pool を mlock できなければ使わない。
/// Zenoh は pool の全体を mlock するので、RLIMIT_MEMLOCK (多くの Linux で一般ユーザーは 8 MiB) が
/// pool より小さいと pool を作れず、Zenoh が ERROR を出し続ける。そうなる前にここで切る
pub(crate) fn shm_usable(manifest: &Value) -> bool {
    if !shm_enabled(manifest) {
        return false;
    }
    match memlock_allows(shm_pool_bytes(manifest)) {
        Ok(()) => true,
        Err(message) => {
            // 設定とセッションごとに何度も呼ばれるので、警告はプロセスで 1 度だけ
            static WARNED: std::sync::Once = std::sync::Once::new();
            WARNED.call_once(|| log::warn!("zenoh: shared memory is turned off. {message}"));
            false
        }
    }
}

/// pool (bytes) を mlock できるか。ソフトの上限が足りなければハードの上限まで上げてみる
#[cfg(unix)]
pub fn memlock_allows(bytes: u64) -> Result<(), String> {
    let mut lim = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: lim は有効な rlimit を指す
    if unsafe { libc::getrlimit(libc::RLIMIT_MEMLOCK, &mut lim) } != 0 {
        return Ok(()); // 調べられないときは Zenoh に任せる
    }
    let enough = |v: libc::rlim_t| v == libc::RLIM_INFINITY || v as u64 >= bytes;
    // root は CAP_IPC_LOCK で上限を超えて mlock できる
    // SAFETY: 引数の無い呼び出し
    if enough(lim.rlim_cur) || unsafe { libc::geteuid() } == 0 {
        return Ok(());
    }
    if enough(lim.rlim_max) {
        let raised = libc::rlimit {
            rlim_cur: lim.rlim_max,
            rlim_max: lim.rlim_max,
        };
        // SAFETY: raised は有効な rlimit を指す
        if unsafe { libc::setrlimit(libc::RLIMIT_MEMLOCK, &raised) } == 0 {
            return Ok(());
        }
    }
    Err(format!(
        "the shared memory pool (shm_pool_bytes = {bytes}) can not be locked in memory \
         (RLIMIT_MEMLOCK = {} bytes). raise it (ulimit -l, or LimitMEMLOCK in systemd) \
         or lower shm_pool_bytes. frames are sent over TCP",
        lim.rlim_cur
    ))
}

/// Windows は mlock しない
#[cfg(not(unix))]
pub fn memlock_allows(_bytes: u64) -> Result<(), String> {
    Ok(())
}

/// manifest の shm_pool_bytes (省くと DEFAULT_SHM_POOL_BYTES)
pub(crate) fn shm_pool_bytes(manifest: &Value) -> u64 {
    manifest
        .get("shm_pool_bytes")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_SHM_POOL_BYTES)
}

/// フレームの書き出しの長さ (先頭 4 + ヘッダの長さ 4 + ヘッダ + 本体ごとに長さ 8 と本体、docs/spec/large-data.md §3.1)
pub(crate) fn frame_len(frame: &OutgoingFrame) -> JuizResult<usize> {
    let mut len = 8 + frame.header_bytes()?.len() as u64;
    for i in 0..frame.body_count() {
        len += 8 + frame.body_len(i).unwrap_or_default();
    }
    usize::try_from(len).map_err(|_| anyhow!("the frame is too large ({len} bytes)"))
}

#[cfg(feature = "shared-memory")]
type Provider = std::sync::Arc<zenoh::shm::ShmProvider<zenoh::shm::PosixShmProviderBackend>>;

/// フレームを ZBytes にする道具。セッションごとに 1 つ作る
pub(crate) struct FrameEncoder {
    /// 共有メモリを使うときのセッション
    #[cfg_attr(not(feature = "shared-memory"), allow(dead_code))]
    session: Option<Session>,
    /// pool。Zenoh は pool を作ると全体を mlock するので、大きなフレームを初めて送るときに作る
    #[cfg(feature = "shared-memory")]
    provider: std::sync::OnceLock<Option<Provider>>,
    #[cfg_attr(not(feature = "shared-memory"), allow(dead_code))]
    pool_bytes: u64,
}

impl FrameEncoder {
    /// 共有メモリを使わないもの
    pub(crate) fn plain() -> Self {
        FrameEncoder {
            session: None,
            #[cfg(feature = "shared-memory")]
            provider: std::sync::OnceLock::new(),
            pool_bytes: 0,
        }
    }

    /// セッションの共有メモリの provider を使うもの。manifest で切ったとき・使えないときは plain() と同じ
    pub(crate) fn for_session(session: &Session, manifest: &Value) -> Self {
        if shm_usable(manifest) {
            return FrameEncoder {
                session: Some(session.clone()),
                pool_bytes: shm_pool_bytes(manifest),
                ..Self::plain()
            };
        }
        Self::plain()
    }

    /// 共有メモリを使うか
    pub(crate) fn uses_shm(&self) -> bool {
        self.session.is_some()
    }

    /// フレームを ZBytes にする。len は frame_len(frame)
    pub(crate) fn encode(&self, frame: &OutgoingFrame, len: usize) -> JuizResult<ZBytes> {
        #[cfg(feature = "shared-memory")]
        if let Some(buf) = self.alloc(len) {
            let mut buf = buf;
            let mut dst: &mut [u8] = &mut buf[..];
            frame.write_to(&mut dst)?;
            if !dst.is_empty() {
                return Err(anyhow!(
                    "the frame was shorter than expected ({} bytes left)",
                    dst.len()
                ));
            }
            return Ok(ZBytes::from(buf));
        }
        let bytes = frame.to_bytes()?;
        if bytes.len() != len {
            return Err(anyhow!(
                "the frame length ({}) does not match the expected length ({len})",
                bytes.len()
            ));
        }
        Ok(ZBytes::from(bytes))
    }

    /// 共有メモリの領域を借りる。小さなフレーム・pool に入らないフレームと、
    /// 待っても空かないとき (受け手が領域を持ち続けているなど) は None で、TCP で送る
    #[cfg(feature = "shared-memory")]
    fn alloc(&self, len: usize) -> Option<zenoh::shm::ZShmMut> {
        use zenoh::shm::{Defragment, GarbageCollect, ZAllocError, ZLayoutAllocError};
        use zenoh::Wait;
        const WAIT: std::time::Duration = std::time::Duration::from_secs(1);
        let session = self.session.as_ref()?;
        if len < SHM_THRESHOLD || len as u64 > self.pool_bytes {
            return None;
        }
        // 最初に使うときに pool を作り、できるまで待つ (Zenoh の shared_memory の mode は lazy)
        let provider = self
            .provider
            .get_or_init(|| {
                let p = session.get_shm_provider().wait();
                if p.is_none() {
                    log::warn!(
                        "zenoh: can not create the shared memory pool. frames are sent over TCP"
                    );
                }
                p
            })
            .as_ref()?;
        // Zenoh の BlockOn は空くまで (pool より大きくても) 待ち続けるので使わず、ここで待つ時間を区切る
        let deadline = std::time::Instant::now() + WAIT;
        loop {
            match provider
                .alloc(len)
                .with_policy::<GarbageCollect<Defragment>>()
                .wait()
            {
                Ok(buf) => return Some(buf),
                Err(ZLayoutAllocError::Alloc(
                    ZAllocError::OutOfMemory | ZAllocError::NeedDefragment,
                )) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                Err(e) => {
                    log::warn!("zenoh: can not allocate {len} bytes of shared memory ({e:?}). the frame is sent over TCP");
                    return None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_len_matches_to_bytes() {
        let mut map = CapsuleMap::new();
        map.insert("a".to_owned(), jvalue!(1).into());
        map.insert(
            "arr".to_owned(),
            jvalue!((0..10000).collect::<Vec<i64>>()).into(),
        );
        let frame = OutgoingFrame::request(3, &map).unwrap();
        assert_eq!(frame_len(&frame).unwrap(), frame.to_bytes().unwrap().len());
        let plain = FrameEncoder::plain();
        assert!(!plain.uses_shm());
        let len = frame_len(&frame).unwrap();
        assert_eq!(plain.encode(&frame, len).unwrap().len(), len);
    }

    #[test]
    fn manifest_options() {
        assert_eq!(shm_enabled(&jvalue!({})), cfg!(feature = "shared-memory"));
        assert!(!shm_enabled(&jvalue!({"shared_memory": false})));
        assert_eq!(shm_pool_bytes(&jvalue!({})), DEFAULT_SHM_POOL_BYTES);
        assert_eq!(shm_pool_bytes(&jvalue!({"shm_pool_bytes": 1024})), 1024);
        // 小さな pool はどこでも mlock できる
        assert!(memlock_allows(4096).is_ok());
    }
}
