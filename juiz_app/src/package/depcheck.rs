//! `juiz run` で、アプリの依存(基礎)につながるかを起動前に確かめ、つながらなければ選び直させる
//! (docs/spec/app-package.md §15.4〜§15.7、issue #394)
//!
//! つなぎ先の決め方(§10.2)は [`depends::resolve`] のまま。ここでは、決まったつなぎ先の profile を読んで
//! §10.3 の確認をし、失敗したら対話できるときだけ選択を出す。選び直した先は `--depend` と同じ扱いで
//! conf を作り直す(`transforms` などの ID の書き換え §10.4 も、ipc / http のブローカーの用意も、選び直した先に合う)。

use super::depends::{self, Depend, DependError, NameList};
use super::install::{find_newest, Installed};
use super::manifest::{Kind, Manifest};
use super::precheck::{choose, Mode, Prompt, Quit};
use juiz_core::anyhow::{anyhow, Result};
use juiz_sdk::serde_json::{json, Value};
use semver::Version;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 基礎を起動してから、つながるまで待つ長さ(§15.4)
pub(crate) const START_WAIT: Duration = Duration::from_secs(10);
/// 基礎を起動してから、つながるかを試す間隔(§15.4)
pub(crate) const START_POLL: Duration = Duration::from_millis(500);
/// 覚えたつなぎ先のファイル名(§15.6)
pub(crate) const DEPENDS_FILE: &str = "depends.yaml";

/// つなぎ先の profile を読んだ結果
#[derive(Debug, Clone)]
pub(crate) enum Probe {
    /// つながった。値は `system/profile`
    Profile(Value),
    /// つなげなかった
    Unreachable,
    /// この juiz では起動前に確かめられない種類のブローカー(`System::setup` の確認に任せる)
    Unsupported,
}

/// 依存の確認が外の世界に触る所。テストでは差し替える
pub(crate) trait DepEnv: Prompt {
    /// subsystem の conf(`type_name`・`name`)の相手の profile を読む
    fn probe(&mut self, conf: &Value) -> Probe;
    /// この機械で IPC で待ち受けている juiz の名前(§15.5。登録を持たない古い juiz の基礎のため)
    fn ipc_names(&mut self) -> Vec<String>;
    /// この機械の名前の登録(naming.md §8)
    fn names(&self) -> &dyn NameList;
    /// 基礎を別窓で起動する(§15.7)。端末を開けず切り離して起動したときは、ログのパスを返す
    fn launch_base(&mut self, name: &str, version: &Version) -> Result<Option<String>>;
    fn sleep(&mut self, d: Duration);
}

/// 確認の結果
#[derive(Debug)]
pub(crate) struct Checked {
    /// 依存のつなぎ先を入れた conf
    pub conf: Value,
    pub depends: Vec<Depend>,
    /// §15.6 に覚える (依存の名前, URL)
    pub remember: Vec<(String, String)>,
}

/// 確認の失敗
enum Failure {
    /// つなげない
    Unreachable,
    /// つながったが §10.3 の確認に通らない
    Mismatch(DependError),
}

fn verify(env: &mut impl DepEnv, app: &str, dep: &Depend) -> Result<(), Failure> {
    match env.probe(&dep.conf) {
        Probe::Unsupported => Ok(()),
        Probe::Unreachable => Err(Failure::Unreachable),
        Probe::Profile(p) => depends::check_profile(app, dep, &p).map_err(Failure::Mismatch),
    }
}

/// `using robot_base at … (remembered; …)` の文(§15.6)
fn remembered_line(name: &str, url: &str) -> String {
    format!("using {name} at {url} (remembered; --forget-depends to reset)")
}

/// `using robot_base at … (found in the name list)` の文(naming.md §8)
fn found_line(name: &str, url: &str) -> String {
    format!("using {name} at {url} (found in the name list)")
}

/// 登録で見つけたつなぎ先で、既定(`ipc://<名前>`)と違うときだけ文を出す(naming.md §8)
fn found_url(dep: &Depend) -> Option<String> {
    let url = dep.url();
    (dep.found && url != format!("ipc://{}", dep.name)).then_some(url)
}

/// 依存のつなぎ先を決め、つながるかを確かめる(§10.2、§10.3、§15.4〜§15.6)
///
/// - `original` は依存を足す前の conf。選び直すたびに、ここから [`depends::resolve`] をやり直す
/// - `cli` は `--depend`、`remembered` は §15.6 で覚えた (依存の名前 → URL)。`--depend` のある依存には覚えた先を使わない
/// - 対話できない(`--yes` も)ときは §10.3 のエラーで止まる
pub(crate) fn resolve_and_check(
    m: &Manifest,
    original: &Value,
    cli: &[(String, String)],
    remembered: &BTreeMap<String, String>,
    installed: &[Installed],
    mode: Mode,
    env: &mut impl DepEnv,
) -> Result<Checked> {
    let mut overrides: Vec<(String, String)> = Vec::new();
    // 覚えた先は --depend より前に置く(同じ名前は後のものが勝つ)
    let mut from_memory: BTreeMap<String, String> = BTreeMap::new();
    for (name, url) in remembered {
        if m.depends.contains_key(name) && !cli.iter().any(|(n, _)| n == name) {
            overrides.push((name.clone(), url.clone()));
            from_memory.insert(name.clone(), url.clone());
        }
    }
    overrides.extend(cli.iter().cloned());
    let installed_bases: BTreeSet<&str> = installed
        .iter()
        .filter(|i| i.manifest.kind == Kind::Base)
        .map(|i| i.manifest.name.as_str())
        .collect();
    let mut checked: BTreeSet<String> = BTreeSet::new();
    // 2) 3) で選び、まだ確かめていない先
    let mut chosen: BTreeMap<String, String> = BTreeMap::new();
    let mut remember = Vec::new();
    loop {
        let mut conf = original.clone();
        let deps = depends::resolve(m, &mut conf, &overrides, installed, env.names())?;
        let Some(dep) = deps.iter().find(|d| !checked.contains(&d.name)).cloned() else {
            return Ok(Checked {
                conf,
                depends: deps,
                remember,
            });
        };
        let remembered_url = from_memory
            .get(&dep.name)
            .filter(|_| !chosen.contains_key(&dep.name));
        let failure = match verify(env, &m.name, &dep) {
            Ok(()) => {
                if let Some(url) = remembered_url {
                    env.say(&remembered_line(&dep.name, url));
                } else if let Some(url) = found_url(&dep) {
                    env.say(&found_line(&dep.name, &url));
                }
                // 覚えるのは補った後の URL(http のポートを省いた入力でも、次回は同じ先へ)
                if chosen.remove(&dep.name).is_some() {
                    remember.push((dep.name.clone(), dep.url()));
                }
                checked.insert(dep.name.clone());
                continue;
            }
            Err(f) => f,
        };
        let error = match &failure {
            Failure::Unreachable => {
                depends::connect_error(&m.name, &dep, installed_bases.contains(dep.name.as_str()))
            }
            Failure::Mismatch(e) => DependError(e.0.clone()),
        };
        if mode != Mode::Interactive {
            let mut msg = error.0;
            if let Some(url) = remembered_url {
                msg.push('\n');
                msg.push_str(&remembered_line(&dep.name, url));
            } else if let Some(url) = found_url(&dep) {
                msg.push('\n');
                msg.push_str(&found_line(&dep.name, &url));
            }
            return Err(DependError(msg).into());
        }
        chosen.remove(&dep.name);
        match ask(
            env,
            m,
            &dep,
            &failure,
            remembered_url.map(String::as_str),
            &overrides,
            original,
            installed,
        )? {
            Next::Retry => {}
            Next::Started => {
                checked.insert(dep.name.clone());
            }
            Next::Use(url) => {
                overrides.retain(|(n, _)| n != &dep.name);
                overrides.push((dep.name.clone(), url.clone()));
                from_memory.remove(&dep.name);
                chosen.insert(dep.name.clone(), url);
            }
        }
    }
}

/// 選択の後にすること
enum Next {
    /// 同じつなぎ先でもう一度確かめる
    Retry,
    /// 基礎を起動し、今のつなぎ先でつながった
    Started,
    /// このつなぎ先に変えて確かめる(つながったら覚える)
    Use(String),
}

/// 選択肢
enum Choice {
    Start(Version),
    Running(Vec<(String, Version)>),
    Enter,
    Retry,
    Quit,
}

/// §15.4 の選択を出す
#[allow(clippy::too_many_arguments)]
fn ask(
    env: &mut impl DepEnv,
    m: &Manifest,
    dep: &Depend,
    failure: &Failure,
    remembered_url: Option<&str>,
    overrides: &[(String, String)],
    original: &Value,
    installed: &[Installed],
) -> Result<Next> {
    let url = dep.url();
    let question = match failure {
        Failure::Unreachable => format!(
            "{} depends on {} {}, but it is not running at {url}.",
            m.name, dep.name, dep.range
        ),
        Failure::Mismatch(e) => {
            env.say(&e.0);
            format!("{} depends on {} {}.", m.name, dep.name, dep.range)
        }
    };
    if let Some(u) = remembered_url {
        env.say(&remembered_line(&dep.name, u));
    }
    loop {
        let installed_base = find_newest(installed, &dep.name, Some(&dep.req))
            .filter(|i| i.manifest.kind == Kind::Base)
            .map(|i| i.manifest.version.clone());
        let running = running_bases(env, dep, &url);
        let mut choices = Vec::new();
        let mut labels = Vec::new();
        if let Some(v) = installed_base {
            labels.push(format!("start {} {v} now", dep.name));
            choices.push(Choice::Start(v));
        }
        if !running.is_empty() {
            labels.push(format!("choose a running {}", dep.name));
            choices.push(Choice::Running(running));
        }
        labels.push(format!(
            "enter where {} is (ex., http://192.168.0.10:8100)",
            dep.name
        ));
        choices.push(Choice::Enter);
        labels.push("retry".to_owned());
        choices.push(Choice::Retry);
        labels.push("quit".to_owned());
        choices.push(Choice::Quit);
        let Some(i) = choose(env, &question, &labels, 0) else {
            return Err(Quit.into());
        };
        match &choices[i] {
            Choice::Quit => return Err(Quit.into()),
            Choice::Retry => return Ok(Next::Retry),
            Choice::Start(v) => {
                if start_and_wait(env, m, dep, v) {
                    return Ok(Next::Started);
                }
            }
            Choice::Running(list) => {
                let labels: Vec<String> = list
                    .iter()
                    .map(|(u, v)| format!("{} {v} at {u}", dep.name))
                    .collect();
                let Some(j) = choose(env, &format!("running {}:", dep.name), &labels, 0) else {
                    return Err(Quit.into());
                };
                return Ok(Next::Use(list[j].0.clone()));
            }
            Choice::Enter => {
                let Some(line) = env.ask(&format!("where is {}: ", dep.name)) else {
                    return Err(Quit.into());
                };
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                // 書式とポートの補い(§10.2)をここで確かめ、だめなら同じ選択に戻る
                let mut trial = overrides.to_vec();
                trial.retain(|(n, _)| n != &dep.name);
                trial.push((dep.name.clone(), line.to_owned()));
                match depends::resolve(m, &mut original.clone(), &trial, installed, env.names()) {
                    Ok(_) if line != depends::DEPEND_NONE => return Ok(Next::Use(line.to_owned())),
                    Ok(_) => env.say(&format!("'{line}' is not a broker URL (ex., ipc://robot_base, http://192.168.0.10:8100)")),
                    Err(e) => env.say(&format!("{e}")),
                }
            }
        }
    }
}

/// §15.5: この機械で動いている、名前と版の合う基礎(今のつなぎ先は除く)
///
/// この機械の登録(naming.md §8)の一覧と、登録を持たない古い juiz の基礎のための IPC の待ち受けの列挙を合わせる。
/// 同じ System が両方で見つかったら `uuid` で 1 つにまとめる。
fn running_bases(env: &mut impl DepEnv, dep: &Depend, current: &str) -> Vec<(String, Version)> {
    let mut out = Vec::new();
    let mut uuids: BTreeSet<String> = BTreeSet::new();
    let kinds = env.names().kinds();
    for reg in env.names().bases(&dep.name, &dep.req) {
        let Some(v) = reg.package_version().and_then(|v| Version::parse(v).ok()) else {
            continue;
        };
        let uuid = reg.uuid.clone();
        let candidate = juiz_core::naming::Candidate { registration: reg };
        let Ok((_, url)) = candidate.select_broker(None, &kinds) else {
            continue;
        };
        uuids.insert(uuid);
        if url == current {
            continue;
        }
        out.push((url, v));
    }
    for name in env.ipc_names() {
        let url = format!("ipc://{name}");
        if url == current {
            continue;
        }
        let Probe::Profile(p) = env.probe(&json!({ "type_name": "ipc", "name": name })) else {
            continue;
        };
        if let Some(u) = p
            .get("system_store")
            .and_then(|s| s.get("uuid"))
            .and_then(Value::as_str)
        {
            if uuids.contains(u) {
                continue;
            }
        }
        let Some(pkg) = p.get("package") else {
            continue;
        };
        if pkg.get("kind").and_then(Value::as_str) != Some("base")
            || pkg.get("name").and_then(Value::as_str) != Some(dep.name.as_str())
        {
            continue;
        }
        let Some(v) = pkg
            .get("version")
            .and_then(Value::as_str)
            .and_then(|v| Version::parse(v).ok())
            .filter(|v| dep.req.matches(v))
        else {
            continue;
        };
        out.push((url, v));
    }
    out
}

/// 1) 基礎を別窓で起動し、今のつなぎ先でつながるまで待つ(§15.4、§15.7)
fn start_and_wait(env: &mut impl DepEnv, m: &Manifest, dep: &Depend, v: &Version) -> bool {
    match env.launch_base(&dep.name, v) {
        Ok(None) => env.say(&format!("starting {} {v} in another window.", dep.name)),
        Ok(Some(log)) => env.say(&format!(
            "starting {} {v} in the background; its output goes to {log}",
            dep.name
        )),
        Err(e) => {
            env.say(&format!("cannot start {} {v}: {e:#}", dep.name));
            return false;
        }
    }
    let tries = (START_WAIT.as_millis() / START_POLL.as_millis()) as usize;
    for _ in 0..tries {
        env.sleep(START_POLL);
        match verify(env, &m.name, dep) {
            Ok(()) => return true,
            Err(Failure::Mismatch(e)) => {
                env.say(&e.0);
                return false;
            }
            Err(Failure::Unreachable) => {}
        }
    }
    env.say(&format!(
        "{} did not start in {} s.",
        dep.name,
        START_WAIT.as_secs()
    ));
    false
}

// ---- §15.6 覚えたつなぎ先 ----

/// `<データ>/depends.yaml` のパス
pub(crate) fn depends_file(data: &Path) -> PathBuf {
    data.join(DEPENDS_FILE)
}

/// ファイル全体を (アプリ名 → (依存の名前 → URL)) で読む。無ければ空。壊れていたら警告を出して空
fn load_all(file: &Path, warn: &mut dyn FnMut(&str)) -> BTreeMap<String, BTreeMap<String, String>> {
    let Ok(text) = std::fs::read_to_string(file) else {
        return BTreeMap::new();
    };
    let broken = |warn: &mut dyn FnMut(&str)| {
        warn(&format!(
            "warning: {} is broken; ignoring the remembered connections",
            file.display()
        ));
        BTreeMap::new()
    };
    let v = match super::yaml::parse(&text) {
        Ok(v) => v,
        Err(_) => return broken(warn),
    };
    if v.is_null() {
        return BTreeMap::new();
    }
    let Some(apps) = v.as_object() else {
        return broken(warn);
    };
    let mut out = BTreeMap::new();
    for (app, deps) in apps {
        if deps.is_null() {
            continue;
        }
        let Some(deps) = deps.as_object() else {
            return broken(warn);
        };
        let mut m = BTreeMap::new();
        for (name, url) in deps {
            let Some(url) = url.as_str() else {
                return broken(warn);
            };
            m.insert(name.clone(), url.to_owned());
        }
        out.insert(app.clone(), m);
    }
    out
}

fn save_all(file: &Path, all: &BTreeMap<String, BTreeMap<String, String>>) -> Result<()> {
    let q = |s: &str| juiz_sdk::serde_json::to_string(s).unwrap();
    let mut text = String::from(
        "# juiz run writes this file (docs/spec/app-package.md 15.6). You may edit it.\n",
    );
    for (app, deps) in all {
        if deps.is_empty() {
            continue;
        }
        text.push_str(&format!("{}:\n", q(app)));
        for (name, url) in deps {
            text.push_str(&format!("  {}: {}\n", q(name), q(url)));
        }
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| anyhow!("cannot create {}: {e}", dir.display()))?;
    }
    // 途中で止まっても壊れたファイルを残さないよう、隣に書いてから置き換える
    let tmp = file.with_extension(format!("yaml.tmp-{}", std::process::id()));
    std::fs::write(&tmp, text).map_err(|e| anyhow!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, file).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        anyhow!("cannot write {}: {e}", file.display())
    })
}

/// アプリの覚えたつなぎ先(依存の名前 → URL)
pub(crate) fn load_remembered(data: &Path, app: &str) -> BTreeMap<String, String> {
    load_all(&depends_file(data), &mut |w| eprintln!("{w}"))
        .remove(app)
        .unwrap_or_default()
}

/// つなぎ先を覚える(同じ依存の前の値は置き換える)
pub(crate) fn remember(data: &Path, app: &str, entries: &[(String, String)]) -> Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    let file = depends_file(data);
    let mut all = load_all(&file, &mut |_| {});
    let deps = all.entry(app.to_owned()).or_default();
    for (name, url) in entries {
        deps.insert(name.clone(), url.clone());
    }
    save_all(&file, &all)
}

/// `--forget-depends`: アプリの項目を消す。消したら true
pub(crate) fn forget(data: &Path, app: &str) -> Result<bool> {
    let file = depends_file(data);
    let mut all = load_all(&file, &mut |w| eprintln!("{w}"));
    if all.remove(app).is_none() {
        return Ok(false);
    }
    save_all(&file, &all)?;
    Ok(true)
}

// ---- 本物の Env ----

/// 本物の端末・ブローカー・OS を使う [`DepEnv`]
pub(crate) struct RealDepEnv {
    /// 端末から切り離して起動したときのログの置き場所(§15.7)
    pub log_dir: PathBuf,
    /// この機械の名前の登録(naming.md §8)
    pub names: depends::RegistryNames,
}

impl Prompt for RealDepEnv {
    fn say(&mut self, line: &str) {
        super::precheck::RealEnv.say(line)
    }
    fn ask(&mut self, prompt: &str) -> Option<String> {
        super::precheck::RealEnv.ask(prompt)
    }
}

impl DepEnv for RealDepEnv {
    fn probe(&mut self, conf: &Value) -> Probe {
        probe(conf)
    }

    fn ipc_names(&mut self) -> Vec<String> {
        ipc_names()
    }

    fn names(&self) -> &dyn NameList {
        &self.names
    }

    fn launch_base(&mut self, name: &str, version: &Version) -> Result<Option<String>> {
        launch_base(name, version, &self.log_dir)
    }

    fn sleep(&mut self, d: Duration) {
        std::thread::sleep(d)
    }
}

/// `system/profile` の値の `__value__` の包みを外す
fn unwrap_value(v: Value) -> Value {
    match v {
        Value::Object(mut m) if m.contains_key("__value__") && m.len() == 1 => {
            m.remove("__value__").unwrap()
        }
        v => v,
    }
}

fn probe(conf: &Value) -> Probe {
    let t = conf.get("type_name").and_then(Value::as_str).unwrap_or("");
    let n = conf.get("name").and_then(Value::as_str).unwrap_or("");
    match t {
        #[cfg(feature = "ipc-broker")]
        "ipc" => {
            use juiz_core::prelude::SystemBrokerProxy;
            let result =
                juiz_core::ipc::IpcBrokerProxy::new(&json!({ "name": n, "timeout_ms": 1000 }))
                    .and_then(|p| {
                        juiz_core::CRUDBrokerProxyHolder::new(
                            "IpcBrokerProxy",
                            "ipc",
                            n,
                            Box::new(p),
                        )
                    })
                    .and_then(|h| h.read().map_err(|_| anyhow!("lock"))?.system_profile());
            match result {
                Ok(v) => Probe::Profile(unwrap_value(v)),
                Err(_) => Probe::Unreachable,
            }
        }
        "http" | "qmp" => {
            let Some((host, port)) = n
                .rsplit_once(':')
                .and_then(|(h, p)| p.parse::<i64>().ok().map(|p| (h, p)))
            else {
                return Probe::Unsupported;
            };
            use super::precheck::{Env, Transport};
            let transport = if t == "http" {
                Transport::Http
            } else {
                Transport::Qmp
            };
            match super::precheck::RealEnv.profile(transport, host, port) {
                Some(v) => Probe::Profile(v),
                None => Probe::Unreachable,
            }
        }
        _ => Probe::Unsupported,
    }
}

/// §15.5: IPC の待ち受けの名前を列挙する
fn ipc_names() -> Vec<String> {
    #[cfg(all(feature = "ipc-broker", unix))]
    {
        let Ok(dir) = juiz_core::ipc::ipc_name::ipc_socket_dir() else {
            return Vec::new();
        };
        let Ok(rd) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut names: Vec<String> = rd
            .flatten()
            .filter_map(|e| {
                let n = e.file_name().into_string().ok()?;
                n.strip_suffix(".sock").map(str::to_owned)
            })
            .collect();
        names.sort();
        names
    }
    #[cfg(all(feature = "ipc-broker", windows))]
    {
        let Ok(rd) = std::fs::read_dir(r"\\.\pipe\") else {
            return Vec::new();
        };
        let mut names: Vec<String> = rd
            .flatten()
            .filter_map(|e| {
                let n = e.file_name().into_string().ok()?;
                n.strip_prefix("juiz-").map(str::to_owned)
            })
            .collect();
        names.sort();
        names.dedup();
        names
    }
    #[cfg(not(feature = "ipc-broker"))]
    Vec::new()
}

/// 関連付けや別窓の起動に使う juiz の実行ファイル(§17.2)。pip 版はランチャが `JUIZ_LAUNCHER` で渡す
pub(crate) fn juiz_executable() -> Result<PathBuf> {
    if let Some(p) = std::env::var_os("JUIZ_LAUNCHER").filter(|p| !p.is_empty()) {
        return Ok(PathBuf::from(p));
    }
    std::env::current_exe().map_err(|e| anyhow!("cannot find the juiz executable: {e}"))
}

/// §15.7: 基礎を別窓で起動する。アプリの子プロセスとしては待たない
fn launch_base(name: &str, version: &Version, log_dir: &Path) -> Result<Option<String>> {
    let exe = juiz_executable()?;
    let spec = format!("{name}@{version}");
    let args = ["run".to_owned(), "--launched".to_owned(), spec];
    launch_in_window(&exe, &args, name, log_dir)
}

#[cfg(windows)]
fn launch_in_window(
    exe: &Path,
    args: &[String],
    _name: &str,
    _log_dir: &Path,
) -> Result<Option<String>> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    std::process::Command::new(exe)
        .args(args)
        .creation_flags(CREATE_NEW_CONSOLE)
        .spawn()
        .map_err(|e| anyhow!("{}: {e}", exe.display()))?;
    Ok(None)
}

#[cfg(target_os = "macos")]
fn launch_in_window(
    exe: &Path,
    args: &[String],
    _name: &str,
    _log_dir: &Path,
) -> Result<Option<String>> {
    let command = std::iter::once(exe.display().to_string())
        .chain(args.iter().cloned())
        .map(|s| shell_quote(&s))
        .collect::<Vec<_>>()
        .join(" ");
    let script = format!(
        "tell application \"Terminal\"\n  do script {}\n  activate\nend tell",
        applescript_string(&command)
    );
    let status = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .stdout(std::process::Stdio::null())
        .status()
        .map_err(|e| anyhow!("osascript: {e}"))?;
    if !status.success() {
        return Err(anyhow!("osascript failed ({status})"));
    }
    Ok(None)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn launch_in_window(
    exe: &Path,
    args: &[String],
    name: &str,
    log_dir: &Path,
) -> Result<Option<String>> {
    let has_display = ["DISPLAY", "WAYLAND_DISPLAY"]
        .iter()
        .any(|v| std::env::var_os(v).is_some_and(|s| !s.is_empty()));
    if has_display {
        if let Some(term) = find_terminal() {
            let mut c = std::process::Command::new(&term);
            // gnome-terminal の -e は 1 つの文字列しか受けないので、-- の後に並べる
            if Path::new(&term)
                .file_name()
                .is_some_and(|f| f == "gnome-terminal")
            {
                c.arg("--");
            } else {
                c.arg("-e");
            }
            c.arg(exe).args(args);
            detach(&mut c);
            c.stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(|e| anyhow!("{term}: {e}"))?;
            return Ok(None);
        }
    }
    // 端末を開けなければ、切り離して起動し、出力をログに書く
    std::fs::create_dir_all(log_dir)
        .map_err(|e| anyhow!("cannot create {}: {e}", log_dir.display()))?;
    let log = log_dir.join(format!("{name}.log"));
    let out =
        std::fs::File::create(&log).map_err(|e| anyhow!("cannot create {}: {e}", log.display()))?;
    let err = out.try_clone()?;
    let mut c = std::process::Command::new(exe);
    c.args(args)
        .stdin(std::process::Stdio::null())
        .stdout(out)
        .stderr(err);
    detach(&mut c);
    c.spawn().map_err(|e| anyhow!("{}: {e}", exe.display()))?;
    Ok(Some(log.display().to_string()))
}

/// 端末の Ctrl+C がアプリと一緒に届かないよう、別のプロセスグループにする
#[cfg(all(unix, not(target_os = "macos")))]
fn detach(c: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    c.process_group(0);
}

/// `$TERMINAL`、`x-terminal-emulator`、`gnome-terminal`、`konsole`、`xterm` の順で見つかったもの(§15.7)
#[cfg(all(unix, not(target_os = "macos")))]
fn find_terminal() -> Option<String> {
    let from_env = std::env::var("TERMINAL").ok().filter(|t| !t.is_empty());
    from_env
        .into_iter()
        .chain(
            ["x-terminal-emulator", "gnome-terminal", "konsole", "xterm"]
                .iter()
                .map(|s| s.to_string()),
        )
        .find_map(|t| which(&t))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn which(cmd: &str) -> Option<String> {
    if cmd.contains('/') {
        return Path::new(cmd).is_file().then(|| cmd.to_owned());
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(cmd))
        .find(|p| p.is_file())
        .map(|p| p.display().to_string())
}

#[cfg(target_os = "macos")]
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(target_os = "macos")]
fn applescript_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    fn manifest(text: &str) -> Manifest {
        Manifest::parse_str(text, "t").unwrap()
    }

    fn app() -> Manifest {
        manifest(
            "format: 1\nname: my_patrol\nversion: 0.1.0\ndepends:\n  robot_base: \">=1.2, <2\"\n",
        )
    }

    fn base(version: &str) -> Installed {
        Installed {
            manifest: manifest(&format!(
                "format: 1\nname: robot_base\nversion: {version}\nkind: base\nbase:\n  http_port: 8100\n"
            )),
            dir: PathBuf::from("/x"),
        }
    }

    fn profile(name: &str, version: &str, kind: &str) -> Value {
        json!({ "system_store": { "uuid": "u" }, "package": { "name": name, "version": version, "kind": kind } })
    }

    /// 決めた相手だけ動いている偽の Env
    #[derive(Default)]
    struct Fake {
        /// URL(`ipc://robot_base`)→ profile
        running: BTreeMap<String, Value>,
        /// launch_base で動き出すもの
        on_launch: Option<(String, Value)>,
        launched: Vec<String>,
        answers: VecDeque<&'static str>,
        said: Vec<String>,
        slept: u32,
        /// この機械の名前の登録
        names: depends::tests::FakeNames,
    }

    impl Fake {
        fn answers(mut self, a: &[&'static str]) -> Self {
            self.answers = a.iter().copied().collect();
            self
        }
        fn run(mut self, url: &str, p: Value) -> Self {
            self.running.insert(url.to_owned(), p);
            self
        }
        fn text(&self) -> String {
            self.said.join("\n")
        }
    }

    impl Prompt for Fake {
        fn say(&mut self, line: &str) {
            self.said.push(line.to_owned());
        }
        fn ask(&mut self, prompt: &str) -> Option<String> {
            self.said.push(prompt.to_owned());
            self.answers.pop_front().map(str::to_owned)
        }
    }

    impl DepEnv for Fake {
        fn probe(&mut self, conf: &Value) -> Probe {
            let url = format!(
                "{}://{}",
                conf["type_name"].as_str().unwrap(),
                conf["name"].as_str().unwrap()
            );
            match self.running.get(&url) {
                Some(p) => Probe::Profile(p.clone()),
                None => Probe::Unreachable,
            }
        }
        fn ipc_names(&mut self) -> Vec<String> {
            self.running
                .keys()
                .filter_map(|u| u.strip_prefix("ipc://").map(str::to_owned))
                .collect()
        }
        fn names(&self) -> &dyn NameList {
            &self.names
        }
        fn launch_base(&mut self, name: &str, version: &Version) -> Result<Option<String>> {
            self.launched.push(format!("{name}@{version}"));
            if let Some((u, p)) = self.on_launch.take() {
                self.running.insert(u, p);
            }
            Ok(None)
        }
        fn sleep(&mut self, _d: Duration) {
            self.slept += 1;
        }
    }

    fn run(
        env: &mut Fake,
        cli: &[(&str, &str)],
        remembered: &[(&str, &str)],
        installed: &[Installed],
        mode: Mode,
    ) -> Result<Checked> {
        let cli: Vec<(String, String)> = cli
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let rem: BTreeMap<String, String> = remembered
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        resolve_and_check(
            &app(),
            &json!({ "name": "a" }),
            &cli,
            &rem,
            installed,
            mode,
            env,
        )
    }

    fn first_url(c: &Checked) -> String {
        c.depends[0].url()
    }

    #[test]
    fn running_base_passes_without_question() {
        let mut env =
            Fake::default().run("ipc://robot_base", profile("robot_base", "1.3.0", "base"));
        let c = run(&mut env, &[], &[], &[], Mode::Interactive).unwrap();
        assert_eq!(first_url(&c), "ipc://robot_base");
        assert!(c.remember.is_empty());
        assert!(env.said.is_empty(), "{}", env.text());
    }

    #[test]
    fn non_interactive_keeps_the_old_errors() {
        for mode in [Mode::NoPrompt, Mode::Yes] {
            let mut env = Fake::default();
            let e = run(&mut env, &[], &[], &[base("1.3.0")], mode)
                .unwrap_err()
                .to_string();
            assert_eq!(e, "my_patrol depends on robot_base >=1.2, <2, but robot_base is not running at ipc://robot_base. Start it first with: juiz run robot_base");
            let e = run(&mut env, &[], &[], &[], mode).unwrap_err().to_string();
            assert!(e.contains("robot_base is not installed"), "{e}");
            let mut env =
                Fake::default().run("ipc://robot_base", profile("robot_base", "2.0.1", "base"));
            let e = run(&mut env, &[], &[], &[], mode).unwrap_err().to_string();
            assert_eq!(e, "my_patrol needs robot_base >=1.2, <2, but robot_base 2.0.1 is running at ipc://robot_base.");
            assert!(env.said.is_empty());
        }
    }

    #[test]
    fn start_the_base_and_wait() {
        let mut env = Fake::default().answers(&[""]);
        env.on_launch = Some((
            "ipc://robot_base".into(),
            profile("robot_base", "1.3.0", "base"),
        ));
        let c = run(
            &mut env,
            &[],
            &[],
            &[base("1.1.0"), base("1.3.0")],
            Mode::Interactive,
        )
        .unwrap();
        assert_eq!(env.launched, ["robot_base@1.3.0"]);
        assert_eq!(first_url(&c), "ipc://robot_base");
        assert!(c.remember.is_empty(), "starting the base is not remembered");
        let t = env.text();
        assert!(t.contains("my_patrol depends on robot_base >=1.2, <2, but it is not running at ipc://robot_base."), "{t}");
        assert!(t.contains("  1) start robot_base 1.3.0 now"), "{t}");
        assert!(t.contains("  2) enter where robot_base is"), "{t}");
        assert!(t.contains("  3) retry\n  4) quit"), "{t}");
        assert_eq!(env.slept, 1);
    }

    #[test]
    fn start_that_does_not_come_up_returns_to_the_choice() {
        let mut env = Fake::default().answers(&["1", "4"]);
        let e = run(&mut env, &[], &[], &[base("1.3.0")], Mode::Interactive).unwrap_err();
        assert!(e.downcast_ref::<Quit>().is_some());
        assert_eq!(env.slept, 20);
        assert!(env.text().contains("robot_base did not start in 10 s."));
    }

    #[test]
    fn choose_a_running_base_and_remember_it() {
        let mut env = Fake::default()
            .run("ipc://robot_base2", profile("robot_base", "1.4.0", "base"))
            .run("ipc://other", profile("robot_base", "1.4.0", "app"))
            .run("ipc://old", profile("robot_base", "1.0.0", "base"))
            .answers(&["1", ""]);
        let c = run(&mut env, &[], &[], &[], Mode::Interactive).unwrap();
        assert_eq!(first_url(&c), "ipc://robot_base2");
        assert_eq!(c.conf["subsystems"][0]["name"], "robot_base2");
        assert_eq!(
            c.remember,
            [("robot_base".to_owned(), "ipc://robot_base2".to_owned())]
        );
        let t = env.text();
        assert!(t.contains("  1) choose a running robot_base"), "{t}");
        assert!(
            t.contains("  1) robot_base 1.4.0 at ipc://robot_base2"),
            "{t}"
        );
        assert!(
            !t.contains("ipc://other") && !t.contains("ipc://old"),
            "{t}"
        );
    }

    /// 名前の一覧で見つけた先は、既定と違えば文を出す(naming.md §8)
    #[test]
    fn found_in_the_name_list_is_announced() {
        use depends::tests::{registration, FakeNames};
        let names = |kinds: Vec<&'static str>| FakeNames {
            regs: vec![registration(
                "robot_base",
                "1.3.0",
                "base",
                &[("ipc", "robot_base"), ("http", "0.0.0.0:8101")],
            )],
            kinds,
        };
        // ipc で見つかれば既定と同じなので何も出さない
        let mut env =
            Fake::default().run("ipc://robot_base", profile("robot_base", "1.3.0", "base"));
        env.names = names(vec!["ipc", "http"]);
        let c = run(&mut env, &[], &[], &[], Mode::NoPrompt).unwrap();
        assert_eq!(first_url(&c), "ipc://robot_base");
        assert_eq!(env.text(), "");
        // http で見つかれば出す
        let mut env = Fake::default().run(
            "http://127.0.0.1:8101",
            profile("robot_base", "1.3.0", "base"),
        );
        env.names = names(vec!["http"]);
        let c = run(&mut env, &[], &[], &[], Mode::NoPrompt).unwrap();
        assert_eq!(first_url(&c), "http://127.0.0.1:8101");
        assert_eq!(
            env.text(),
            "using robot_base at http://127.0.0.1:8101 (found in the name list)"
        );
        // つながらなければ、対話できないときのエラーに添える
        let mut env = Fake::default();
        env.names = names(vec!["http"]);
        let e = run(&mut env, &[], &[], &[], Mode::NoPrompt)
            .unwrap_err()
            .to_string();
        assert!(
            e.ends_with("\nusing robot_base at http://127.0.0.1:8101 (found in the name list)"),
            "{e}"
        );
    }

    /// 動いている基礎は名前の一覧と IPC の列挙から集め、同じ System は 1 つにまとめる(§15.5)
    #[test]
    fn running_bases_from_the_name_list() {
        use depends::tests::{registration, FakeNames};
        let mut reg = registration(
            "robot_base",
            "1.4.0",
            "base",
            &[("ipc", "robot_base9"), ("http", "0.0.0.0:8105")],
        );
        reg.uuid = "u".to_owned();
        let mut env = Fake::default()
            // 登録と同じ System(profile の uuid が "u")
            .run("ipc://robot_base9", profile("robot_base", "1.4.0", "base"))
            .answers(&["1"]);
        // ipc をつなげない juiz にして、登録で見つけた先は http にする
        env.names = FakeNames {
            regs: vec![reg],
            kinds: vec!["http"],
        };
        // --depend で別の先を書くと、そこにつながらず選択が出る(並べた後の問いは EOF でやめる)
        let c = run(
            &mut env,
            &[("robot_base", "http://127.0.0.1:8106")],
            &[],
            &[],
            Mode::Interactive,
        );
        assert!(c.is_err());
        let t = env.text();
        assert!(
            t.contains("  1) robot_base 1.4.0 at http://127.0.0.1:8105"),
            "{t}"
        );
        assert!(!t.contains("at ipc://robot_base9"), "{t}");
    }

    #[test]
    fn enter_a_url() {
        // ポートを省いた http は、インストール済みの基礎の base.http_port で補う
        let mut env = Fake::default()
            .run(
                "http://192.168.0.10:8100",
                profile("robot_base", "1.3.0", "base"),
            )
            .answers(&["2", "http://192.168.0.10"]);
        let c = run(&mut env, &[], &[], &[base("1.3.0")], Mode::Interactive).unwrap();
        assert_eq!(first_url(&c), "http://192.168.0.10:8100");
        assert_eq!(
            c.remember,
            [(
                "robot_base".to_owned(),
                "http://192.168.0.10:8100".to_owned()
            )]
        );
        // 書式が違えば同じ選択に戻る
        let mut env = Fake::default()
            .run(
                "http://192.168.0.10:8100",
                profile("robot_base", "1.3.0", "base"),
            )
            .answers(&[
                "1",
                "not a url",
                "1",
                "http://192.168.0.10",
                "1",
                "http://192.168.0.10:8100",
            ]);
        let c = run(&mut env, &[], &[], &[], Mode::Interactive).unwrap();
        assert_eq!(first_url(&c), "http://192.168.0.10:8100");
        assert_eq!(
            c.remember,
            [(
                "robot_base".to_owned(),
                "http://192.168.0.10:8100".to_owned()
            )]
        );
        let t = env.text();
        assert!(t.contains("'not a url' is not a broker URL"), "{t}");
        assert!(t.contains("cannot decide the port for robot_base"), "{t}");
    }

    #[test]
    fn entered_url_that_fails_asks_again() {
        let mut env = Fake::default()
            .run("ipc://wrong", profile("other_base", "1.3.0", "base"))
            .answers(&["1", "ipc://wrong", "3"]);
        let e = run(&mut env, &[], &[], &[], Mode::Interactive).unwrap_err();
        assert!(e.downcast_ref::<Quit>().is_some());
        let t = env.text();
        assert!(
            t.contains("ipc://wrong is other_base, not robot_base."),
            "{t}"
        );
        assert!(
            t.contains("my_patrol depends on robot_base >=1.2, <2.\n"),
            "{t}"
        );
    }

    #[test]
    fn retry_and_eof() {
        let mut env = Fake::default().answers(&["2"]);
        let e = run(&mut env, &[], &[], &[], Mode::Interactive).unwrap_err();
        // retry の後、入力が閉じたらやめる
        assert!(e.downcast_ref::<Quit>().is_some());
        assert_eq!(env.text().matches("but it is not running at").count(), 2);
    }

    #[test]
    fn remembered_url_is_used_and_announced() {
        let mut env =
            Fake::default().run("ipc://robot_base2", profile("robot_base", "1.3.0", "base"));
        let c = run(
            &mut env,
            &[],
            &[
                ("robot_base", "ipc://robot_base2"),
                ("unrelated", "ipc://x"),
            ],
            &[],
            Mode::NoPrompt,
        )
        .unwrap();
        assert_eq!(first_url(&c), "ipc://robot_base2");
        assert!(c.remember.is_empty());
        assert_eq!(
            env.text(),
            "using robot_base at ipc://robot_base2 (remembered; --forget-depends to reset)"
        );
        // --depend が勝つ
        let mut env =
            Fake::default().run("ipc://robot_base", profile("robot_base", "1.3.0", "base"));
        let c = run(
            &mut env,
            &[("robot_base", "ipc://robot_base")],
            &[("robot_base", "ipc://robot_base2")],
            &[],
            Mode::NoPrompt,
        )
        .unwrap();
        assert_eq!(first_url(&c), "ipc://robot_base");
        assert!(env.said.is_empty());
        // つながらなければ、対話できないときはエラーに添える
        let mut env = Fake::default();
        let e = run(
            &mut env,
            &[],
            &[("robot_base", "ipc://robot_base2")],
            &[],
            Mode::NoPrompt,
        )
        .unwrap_err()
        .to_string();
        assert!(
            e.ends_with(
                "\nusing robot_base at ipc://robot_base2 (remembered; --forget-depends to reset)"
            ),
            "{e}"
        );
    }

    #[test]
    fn depend_none_is_not_checked() {
        let mut env = Fake::default();
        let c = run(
            &mut env,
            &[("robot_base", "none")],
            &[],
            &[],
            Mode::NoPrompt,
        )
        .unwrap();
        assert!(c.depends.is_empty());
    }

    #[test]
    fn depends_file_roundtrip() {
        let dir = std::env::temp_dir().join(format!("juiz-depcheck-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(load_remembered(&dir, "my_patrol").is_empty());
        remember(
            &dir,
            "my_patrol",
            &[("robot_base".into(), "http://192.168.0.10:8100".into())],
        )
        .unwrap();
        remember(&dir, "other", &[("arm".into(), "ipc://arm2".into())]).unwrap();
        let r = load_remembered(&dir, "my_patrol");
        assert_eq!(
            r.get("robot_base").map(String::as_str),
            Some("http://192.168.0.10:8100")
        );
        assert!(forget(&dir, "my_patrol").unwrap());
        assert!(!forget(&dir, "my_patrol").unwrap());
        assert!(load_remembered(&dir, "my_patrol").is_empty());
        assert_eq!(load_remembered(&dir, "other").len(), 1);
        // 壊れたファイルは無いものとして扱う
        std::fs::write(depends_file(&dir), "- a\n- b\n").unwrap();
        let mut warned = Vec::new();
        assert!(load_all(&depends_file(&dir), &mut |w| warned.push(w.to_owned())).is_empty());
        assert_eq!(warned.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
