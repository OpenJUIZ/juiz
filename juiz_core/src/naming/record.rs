//! 登録ファイルの中身と読み書き (docs/spec/naming.md §3)

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use juiz_sdk::anyhow::anyhow;
use serde::{Deserialize, Serialize};

use crate::prelude::*;

use super::dir::create_dir_0700;

/// 登録ファイルの形の版 (§3.2 の `format`)
pub const FORMAT: u64 = 1;

/// 読めない登録ファイルを壊れたものとみなすまでの時間 (§3.2)
pub const BROKEN_AFTER: Duration = Duration::from_secs(10);

/// 登録ファイルの `brokers` の 1 つ (§3.3)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredBroker {
    /// ブローカーの種類名 (`ipc`・`http`・`qmp`・`zenoh` など)
    pub type_name: String,
    /// ブローカーの ID (`<type_name>://<name>`)。待ち受けたアドレスのまま
    pub id: String,
    /// ポートを持つ種類なら、実際に取れた番号
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}

impl RegisteredBroker {
    /// ブローカーの種類と名前 (system/profile の `core_store.brokers` の要素の `type_name`・`name`) から作る
    pub fn new(type_name: &str, name: &str) -> Self {
        let port = if type_name == "ipc" {
            None
        } else {
            name.rsplit_once(':')
                .and_then(|(_, p)| p.parse::<u16>().ok())
        };
        RegisteredBroker {
            type_name: type_name.to_owned(),
            id: format!("{type_name}://{name}"),
            port,
        }
    }

    /// 同じ機械からつなぐときの URL。host が `0.0.0.0`・`::`・`[::]`・空なら `127.0.0.1` にする (§3.3・§7.3)
    pub fn local_url(&self) -> String {
        let Some((scheme, rest)) = self.id.split_once("://") else {
            return self.id.clone();
        };
        if scheme == "ipc" {
            return self.id.clone();
        }
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, ""),
        };
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() => (h, Some(p)),
            _ => (authority, None),
        };
        let host = match host {
            "" | "0.0.0.0" | "::" | "[::]" => "127.0.0.1",
            h => h,
        };
        match port {
            Some(p) => format!("{scheme}://{host}:{p}{path}"),
            None => format!("{scheme}://{host}{path}"),
        }
    }
}

/// 登録ファイル 1 つの中身 (§3.2)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Registration {
    pub format: u64,
    /// 登録名の名前の部分
    pub name: String,
    /// System の既定ネームスペース。ルートは `""`
    pub namespace: String,
    /// `system/profile` の `system_store.uuid`
    pub uuid: String,
    pub pid: u32,
    /// 機械のホスト名 (表示用)
    pub host: String,
    pub juiz_version: String,
    /// 登録した時刻 (RFC 3339)
    pub started_at: String,
    /// `juiz run` で起動したときの `package`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<Value>,
    pub brokers: Vec<RegisteredBroker>,
}

impl Registration {
    /// `robot1/mobile_sim` の形の登録名。ルートのネームスペースなら名前だけ
    pub fn registered_name(&self) -> String {
        registered_name(&self.namespace, &self.name)
    }

    /// `package.name`
    pub fn package_name(&self) -> Option<&str> {
        self.package.as_ref()?.get("name")?.as_str()
    }

    /// `package.version`
    pub fn package_version(&self) -> Option<&str> {
        self.package.as_ref()?.get("version")?.as_str()
    }

    /// `package.kind`
    pub fn package_kind(&self) -> Option<&str> {
        self.package.as_ref()?.get("kind")?.as_str()
    }

    /// 登録ファイルの JSON
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

/// `robot1/mobile_sim` の形の登録名
pub fn registered_name(namespace: &str, name: &str) -> String {
    if namespace.is_empty() {
        name.to_owned()
    } else {
        format!("{namespace}/{name}")
    }
}

/// 登録名・ネームスペースの要素の文字の規則 (§4.1)。IPC ブローカーの名前と同じ
pub fn validate_name(name: &str) -> JuizResult<()> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.');
    if ok {
        Ok(())
    } else {
        Err(anyhow!(
            "'{name}' must be 1 to 64 characters of [A-Za-z0-9_-.], not starting with '.'"
        ))
    }
}

/// ネームスペースを要素に分けて、各要素を確かめる。ルート (`""`・`/`) は空の列
pub fn namespace_segments(namespace: &str) -> JuizResult<Vec<&str>> {
    let ns = namespace.trim_matches('/');
    if ns.is_empty() {
        return Ok(vec![]);
    }
    ns.split('/').map(|s| validate_name(s).map(|_| s)).collect()
}

/// 登録ファイルを読んだ結果
#[derive(Debug)]
pub enum ReadResult {
    Ok(Registration),
    /// 知らない版 (§3.2)
    UnknownFormat(u64),
    /// JSON として読めない、または必須のキーが無い (書いている途中か、壊れている)
    Unreadable,
    /// ファイルが無い
    Missing,
}

/// 登録ファイルを 1 つ読む
pub fn read_registration(path: &Path) -> ReadResult {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return ReadResult::Missing,
        Err(_) => return ReadResult::Unreadable,
    };
    let Ok(v) = serde_json::from_str::<Value>(&text) else {
        return ReadResult::Unreadable;
    };
    match v.get("format").and_then(Value::as_u64) {
        Some(FORMAT) => {}
        Some(f) if f > FORMAT => return ReadResult::UnknownFormat(f),
        _ => return ReadResult::Unreadable,
    }
    match serde_json::from_value::<Registration>(v) {
        Ok(r) => ReadResult::Ok(r),
        Err(_) => ReadResult::Unreadable,
    }
}

/// 読めないファイルが、作られてから [`BROKEN_AFTER`] より古いか
pub fn is_stale(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    let t = meta.created().or_else(|_| meta.modified());
    match t {
        Ok(t) => SystemTime::now()
            .duration_since(t)
            .map(|d| d > BROKEN_AFTER)
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// 登録ファイルのパス `<names>/<ns の要素>/…/<名前>.json` (§3.1)
pub fn registration_path(names_dir: &Path, namespace: &str, name: &str) -> JuizResult<PathBuf> {
    validate_name(name)?;
    let mut p = names_dir.to_path_buf();
    for s in namespace_segments(namespace)? {
        p.push(s);
    }
    p.push(format!("{name}.json"));
    Ok(p)
}

/// 登録ファイルを `create_new` で作って書く。既にあれば `AlreadyExists` の io エラー (§5.1)
pub fn write_new(path: &Path, reg: &Registration) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        create_dir_0700(parent).map_err(|e| std::io::Error::other(format!("{e}")))?;
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(path)?;
    let text = serde_json::to_string_pretty(reg).map_err(std::io::Error::other)?;
    let r = f
        .write_all(text.as_bytes())
        .and_then(|_| f.write_all(b"\n"));
    if r.is_err() {
        // 書きかけのファイルを残さない
        drop(f);
        let _ = std::fs::remove_file(path);
    }
    r
}

/// 登録ファイルを消し、空になった途中のディレクトリを `names_dir` の手前まで消す (消せなくてもよい)
pub fn remove_registration(names_dir: &Path, path: &Path) {
    let _ = std::fs::remove_file(path);
    let mut dir = path.parent();
    while let Some(d) = dir {
        if d == names_dir || !d.starts_with(names_dir) {
            break;
        }
        if std::fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}

/// `uuid` が一致するときだけ消す (§5.2)。他人の登録は消さない
pub fn remove_if_uuid(names_dir: &Path, path: &Path, uuid: &str) -> bool {
    match read_registration(path) {
        ReadResult::Ok(r) if r.uuid == uuid => {
            remove_registration(names_dir, path);
            true
        }
        _ => false,
    }
}

/// `names_dir` の下の登録ファイル (`*.json`) をすべて、パスの順に返す
pub fn registration_files(names_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![names_dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                stack.push(p);
            } else if ft.is_file() && p.extension().is_some_and(|x| x == "json") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// この機械のホスト名
pub fn host_name() -> String {
    #[cfg(unix)]
    {
        let mut buf = [0u8; 256];
        // SAFETY: buf の大きさを渡している
        let r = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
        if r == 0 {
            let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            if let Ok(s) = std::str::from_utf8(&buf[..end]) {
                if !s.is_empty() {
                    return s.to_owned();
                }
            }
        }
    }
    for key in ["COMPUTERNAME", "HOSTNAME"] {
        if let Some(h) = std::env::var_os(key).and_then(|h| h.into_string().ok()) {
            if !h.is_empty() {
                return h;
            }
        }
    }
    "localhost".to_owned()
}

/// 今の時刻を RFC 3339 (UTC、秒まで) で
pub fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // civil_from_days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}
