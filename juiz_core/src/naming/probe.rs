//! 死活の確かめ (docs/spec/naming.md §6)

use std::time::Duration;

use super::record::{RegisteredBroker, Registration};

/// `system/profile` を待つ時間 (§6 の 2)
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(1);

/// 同じ機械の登録から、つなぐブローカーを選ぶ順 (§7.3)
pub const LOCAL_ORDER: [&str; 4] = ["ipc", "zenoh", "qmp", "http"];

/// ブローカーに `system/profile` を問い合わせた結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    /// 応答した。`system_store.uuid`
    Uuid(String),
    /// つながらない、タイムアウト、juiz の応答でない
    NoResponse,
    /// この種類には問い合わせられない
    Unsupported,
}

/// `system/profile` の問い合わせ。`juiz name` は qmp なども足す (juiz_app)、テストでは差し替える
pub trait ProfileProbe: Send + Sync {
    /// `url` は [`RegisteredBroker::local_url`]
    fn probe(&self, broker: &RegisteredBroker, url: &str) -> Probe;
}

/// juiz_core だけで問い合わせられる種類 (`ipc` は feature "ipc-broker" のとき、`http`)
pub struct CoreProbe;

impl ProfileProbe for CoreProbe {
    fn probe(&self, broker: &RegisteredBroker, url: &str) -> Probe {
        match broker.type_name.as_str() {
            "http" => probe_http(url),
            #[cfg(feature = "ipc-broker")]
            "ipc" => probe_ipc(url),
            _ => Probe::Unsupported,
        }
    }
}

/// juiz_core の BrokerProxy でつなげる種類
pub fn core_kinds() -> Vec<String> {
    let mut v = vec![];
    if cfg!(feature = "ipc-broker") {
        v.push("ipc".to_owned());
    }
    v.push("http".to_owned());
    v
}

fn uuid_of(profile: &crate::prelude::Value) -> Option<String> {
    profile
        .get("system_store")?
        .get("uuid")?
        .as_str()
        .map(str::to_owned)
}

fn probe_http(url: &str) -> Probe {
    let Some(rest) = url.strip_prefix("http://") else {
        return Probe::Unsupported;
    };
    let authority = rest.split('/').next().unwrap_or("");
    let Some((host, port)) = authority
        .rsplit_once(':')
        .and_then(|(h, p)| p.parse::<i64>().ok().map(|p| (h.to_owned(), p)))
    else {
        return Probe::Unsupported;
    };
    // 呼ぶ側が tokio の中かどうかによらないよう、別スレッドで小さな runtime を回す
    let profile = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok()?;
        rt.block_on(crate::port_policy::fetch_http_profile(&host, port))
    })
    .join()
    .ok()
    .flatten();
    match profile.as_ref().and_then(uuid_of) {
        Some(u) => Probe::Uuid(u),
        None => Probe::NoResponse,
    }
}

#[cfg(feature = "ipc-broker")]
fn probe_ipc(url: &str) -> Probe {
    use crate::prelude::*;
    let name = url.strip_prefix("ipc://").unwrap_or(url);
    let name = name.split('/').next().unwrap_or(name);
    let result = crate::brokers::ipc::IpcBrokerProxy::new(
        &jvalue!({ "name": name, "timeout_ms": PROBE_TIMEOUT.as_millis() as u64 }),
    )
    .and_then(|p| {
        crate::brokers::CRUDBrokerProxyHolder::new("IpcBrokerProxy", "ipc", name, Box::new(p))
    })
    .and_then(|h| {
        h.read()
            .map_err(|_| juiz_sdk::anyhow::anyhow!("lock"))?
            .system_profile()
    });
    let Ok(v) = result else {
        return Probe::NoResponse;
    };
    let v = match v {
        Value::Object(mut m) if m.contains_key("__value__") && m.len() == 1 => {
            m.remove("__value__").unwrap()
        }
        v => v,
    };
    match uuid_of(&v) {
        Some(u) => Probe::Uuid(u),
        None => Probe::NoResponse,
    }
}

/// プロセスが生きているか (§6 の 1)
#[cfg(unix)]
pub fn pid_alive(pid: u32) -> bool {
    if pid == 0 || pid > i32::MAX as u32 {
        return false;
    }
    // SAFETY: シグナル 0 は送らずに存在だけを確かめる
    let r = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if r == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

/// プロセスが生きているか (§6 の 1)
#[cfg(windows)]
pub fn pid_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_INVALID_PARAMETER, STILL_ACTIVE,
    };
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    if pid == 0 {
        return false;
    }
    // SAFETY: 得たハンドルは閉じる
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return GetLastError() != ERROR_INVALID_PARAMETER;
        }
        let mut code: u32 = 0;
        let ok = GetExitCodeProcess(h, &mut code);
        CloseHandle(h);
        // 終わったがハンドルが残っているプロセスは死んでいる
        !(ok != 0 && code != STILL_ACTIVE as u32)
    }
}

/// 登録の死活 (§6)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Liveness {
    /// 生きている (応答の uuid が一致した。問い合わせられる種類が無ければ pid だけで)
    Alive,
    /// 死んでいる。ファイルを消してよい
    Dead,
    /// pid は生きているが、どのブローカーも応答しない。候補から外すが消さない
    NoResponse,
    /// つなげる種類のブローカーが無い。候補から外すが消さない
    NotConnectable,
}

/// pid だけで確かめる (§6 の 1)。死んでいれば Dead、そうでなければ Alive
pub fn check_pid(reg: &Registration) -> Liveness {
    if pid_alive(reg.pid) {
        Liveness::Alive
    } else {
        Liveness::Dead
    }
}

/// §6 の 1〜3 で確かめる。`kinds` は引く側がつなげる種類
pub fn check(reg: &Registration, probe: &dyn ProfileProbe, kinds: &[String]) -> Liveness {
    if !pid_alive(reg.pid) {
        return Liveness::Dead;
    }
    let mut connectable = false;
    let mut asked = false;
    for kind in LOCAL_ORDER {
        if !kinds.iter().any(|k| k == kind) {
            continue;
        }
        for b in reg.brokers.iter().filter(|b| b.type_name == kind) {
            connectable = true;
            match probe.probe(b, &b.local_url()) {
                Probe::Uuid(u) if u == reg.uuid => return Liveness::Alive,
                Probe::Uuid(_) => return Liveness::Dead,
                Probe::NoResponse => asked = true,
                Probe::Unsupported => {}
            }
            // 種類ごとに最初の 1 つだけ問い合わせる
            break;
        }
    }
    if !connectable {
        Liveness::NotConnectable
    } else if asked {
        Liveness::NoResponse
    } else {
        // つなげるが問い合わせられない種類だけ (zenoh など)。pid が生きているので生きているとみなす
        Liveness::Alive
    }
}
