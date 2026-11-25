//! 基礎パッケージを起動するときの conf の足し方と、アプリの depends の解決・確認
//! (docs/spec/app-package.md §10、#286)

use super::install::{find_newest, Installed};
use super::manifest::{parse_req, Kind, Manifest};
use juiz_core::anyhow::{anyhow, bail, Result};
use juiz_core::naming::{Candidate, Registration};
use juiz_sdk::serde_json::{json, Map, Value};
use semver::{Version, VersionReq};
use std::cell::RefCell;
use std::collections::BTreeMap;

/// 依存を確かめられなかったときのエラー。`System::setup` の文脈を外して、この文だけを利用者に見せる
#[derive(Debug)]
pub(crate) struct DependError(pub String);

impl std::fmt::Display for DependError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DependError {}

/// depends の 1 つと、そのつなぎ先
#[derive(Debug, Clone)]
pub(crate) struct Depend {
    pub name: String,
    /// depends に書いてあった範囲(文に使う)
    pub range: String,
    pub req: VersionReq,
    /// subsystems に足す conf(`package: <name>` 付き)
    pub conf: Value,
    /// この機械の登録(naming.md §8 の 4)で見つけたつなぎ先か
    pub found: bool,
}

impl Depend {
    /// つなぎ先を `ipc://robot_base` の形で
    pub(crate) fn url(&self) -> String {
        conf_url(&self.conf)
    }

    pub(crate) fn type_name(&self) -> &str {
        self.conf
            .get("type_name")
            .and_then(Value::as_str)
            .unwrap_or("")
    }
}

fn conf_url(conf: &Value) -> String {
    let t = conf.get("type_name").and_then(Value::as_str).unwrap_or("?");
    let n = conf.get("name").and_then(Value::as_str).unwrap_or("?");
    format!("{t}://{n}")
}

/// `--depend <名前>=<種類>` に書ける種類(naming.md §8)
pub(crate) const DEPEND_KINDS: [&str; 4] = ["ipc", "http", "qmp", "zenoh"];

/// `--depend` の値が URL でなく種類の指定(`http` など)なら、その種類
pub(crate) fn depend_kind(v: &str) -> Option<&str> {
    DEPEND_KINDS.iter().copied().find(|k| *k == v)
}

/// この機械の名前の登録(naming.md §3)を引く所。テストでは差し替える
pub(crate) trait NameList {
    /// `package.name` が `name`、`package.kind` が `base`、版が `req` に入る、生きている登録(naming.md §8 の 4)
    fn bases(&self, name: &str, req: &VersionReq) -> Vec<Registration>;
    /// この juiz がつなげるブローカーの種類
    fn kinds(&self) -> Vec<String>;
}

/// 登録を引かない [`NameList`](登録の無い古い動作。テスト用)
#[allow(dead_code)]
pub(crate) struct NoNames;

impl NameList for NoNames {
    fn bases(&self, _name: &str, _req: &VersionReq) -> Vec<Registration> {
        Vec::new()
    }
    fn kinds(&self) -> Vec<String> {
        DEPEND_KINDS.iter().map(|k| k.to_string()).collect()
    }
}

/// この機械の登録ファイルを読む [`NameList`]
///
/// 死活の確かめ(naming.md §6)に時間がかかることがあるので、見つかった結果だけを覚えて使い回す。
/// 見つからなかった結果は覚えない(「もう一度試す」の間に基礎を起動したときに見つけられるように)。
pub(crate) struct RegistryNames {
    found: RefCell<BTreeMap<String, Vec<Registration>>>,
}

impl RegistryNames {
    pub(crate) fn new() -> Self {
        RegistryNames {
            found: RefCell::new(BTreeMap::new()),
        }
    }
}

impl NameList for RegistryNames {
    fn bases(&self, name: &str, req: &VersionReq) -> Vec<Registration> {
        let key = format!("{name} {req}");
        if let Some(v) = self.found.borrow().get(&key) {
            return v.clone();
        }
        let Ok(registry) = juiz_core::naming::NameRegistry::open() else {
            return Vec::new();
        };
        let kinds = self.kinds();
        let out: Vec<Registration> = registry
            .candidates_by(|r| is_base_of(r, name, req), &crate::name::AppProbe, &kinds)
            .into_iter()
            .map(|c| c.registration)
            .collect();
        if !out.is_empty() {
            self.found.borrow_mut().insert(key, out.clone());
        }
        out
    }
    fn kinds(&self) -> Vec<String> {
        crate::name::app_kinds()
    }
}

/// 登録が依存 `name` の範囲 `req` に合う基礎か(naming.md §8 の 4)
pub(crate) fn is_base_of(r: &Registration, name: &str, req: &VersionReq) -> bool {
    r.package_name() == Some(name)
        && r.package_kind() == Some("base")
        && r.package_version()
            .and_then(|v| Version::parse(v).ok())
            .is_some_and(|v| req.matches(&v))
}

/// 登録から依存のつなぎ先を引く(naming.md §7.3・§8)。`kind` は種類の指定
///
/// 見つからなければ None。種類を指定しないで、つなげる種類のブローカーが無いときも None(既定の `ipc://<名前>` に任せる)。
fn from_name_list(
    name: &str,
    req: &VersionReq,
    kind: Option<&str>,
    names: &dyn NameList,
) -> Result<Option<Value>> {
    let mut regs = names.bases(name, req);
    let reg = match regs.len() {
        0 => return Ok(None),
        1 => regs.remove(0),
        _ => {
            // 基礎は同じ機械に 1 つしか動かない(IPC 名が同じ)ので、ふつうはここに来ない
            let list = regs
                .iter()
                .map(|r| format!("{} (pid {})", r.registered_name(), r.pid))
                .collect::<Vec<_>>()
                .join(", ");
            bail!("{name} is registered more than once on this machine: {list}. Choose one with --depend {name}=<URL>.");
        }
    };
    let candidate = Candidate { registration: reg };
    match candidate.select_broker(kind, &names.kinds()) {
        Ok((_, url)) => Ok(Some(url_to_conf(&url)?)),
        Err(_) if kind.is_none() => Ok(None),
        Err(e) => Err(anyhow!("{e}")),
    }
}

/// 種類だけを指定した依存(`--depend <名前>=<種類>`、conf の `subsystems` で `name` を省いたもの)のつなぎ先(naming.md §8)
///
/// 戻り値の bool は登録で見つけたか。見つからなければ ipc は `ipc://<名前>`、http は `127.0.0.1`(ポートは
/// 呼んだ側が `base.http_port` で補う)、qmp・zenoh はエラー。
fn by_kind(
    name: &str,
    req: &VersionReq,
    kind: &str,
    names: &dyn NameList,
) -> Result<(Value, bool)> {
    if let Some(conf) = from_name_list(name, req, Some(kind), names)? {
        return Ok((conf, true));
    }
    match kind {
        "ipc" => Ok((json!({ "type_name": "ipc", "name": name }), false)),
        "http" => Ok((json!({ "type_name": "http", "name": "127.0.0.1" }), false)),
        _ => bail!(
            "{name} is not registered on this machine, so its {kind} broker cannot be found. Write where it is (ex., --depend {name}={kind}://192.168.0.10:<port>)."
        ),
    }
}

/// host がこの機械を指すか(naming.md §8 の http のポートの補い)
fn is_local_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]")
}

/// `--depend <名前>=none` の `none`。その依存につながない(§10.2、#452)
pub(crate) const DEPEND_NONE: &str = "none";

/// `--depend <名前>=<URL>` を分ける
pub(crate) fn parse_depend_arg(s: &str) -> Result<(String, String)> {
    let (name, url) = s.split_once('=').ok_or_else(|| {
        anyhow!("--depend must be <name>=<URL> or <name>=none (ex., robot_base=ipc://robot_base2), but '{s}'")
    })?;
    if name.is_empty() || url.is_empty() {
        bail!("--depend must be <name>=<URL> or <name>=none (ex., robot_base=ipc://robot_base2), but '{s}'");
    }
    Ok((name.to_owned(), url.to_owned()))
}

/// `--remap <旧>=<新>` を分ける(§7.3、#452)
pub(crate) fn parse_remap_arg(s: &str) -> Result<(String, String)> {
    match s.split_once('=') {
        Some((from, to)) if !from.is_empty() && !to.is_empty() => {
            Ok((from.to_owned(), to.to_owned()))
        }
        _ => bail!("--remap must be <old topic>=<new topic> (ex., cmd_vel=cmd_vel_dwa), but '{s}'"),
    }
}

/// conf の Process の `subscribes` の値と `publishes` の要素のトピック名を付け替える(§7.3、#452)
///
/// 対象はトップレベルの `processes`、`containers` と `composite_containers` の `processes`。
/// 名前は conf に書いたとおりの文字列で比べる(ネームスペースで解決する前)。1 つの名前は 1 回だけ付け替える
/// (`a=b` と `b=c` を両方渡しても `a` は `b` になる)。同じ旧い名前が複数あれば後のものを使う。
pub(crate) fn remap_topics(conf: &mut Value, remaps: &[(String, String)]) -> Result<()> {
    if remaps.is_empty() {
        return Ok(());
    }
    let map_name = |v: &mut Value| {
        let target = match v {
            Value::String(_) => Some(v),
            Value::Object(o) => o.get_mut("name").filter(|n| n.is_string()),
            _ => None,
        };
        if let Some(t) = target {
            let name = t.as_str().unwrap();
            if let Some((_, to)) = remaps.iter().rev().find(|(from, _)| from == name) {
                *t = Value::String(to.clone());
            }
        }
    };
    let fix_process = |p: &mut Value| {
        if let Some(Value::Object(subs)) = p.get_mut("subscribes") {
            subs.values_mut().for_each(map_name);
        }
        if let Some(Value::Array(pubs)) = p.get_mut("publishes") {
            pubs.iter_mut().for_each(map_name);
        }
    };
    let obj = conf_object(conf)?;
    if let Some(Value::Array(ps)) = obj.get_mut("processes") {
        ps.iter_mut().for_each(fix_process);
    }
    for key in ["containers", "composite_containers"] {
        if let Some(Value::Array(cs)) = obj.get_mut(key) {
            for c in cs.iter_mut() {
                if let Some(Value::Array(ps)) = c.get_mut("processes") {
                    ps.iter_mut().for_each(fix_process);
                }
            }
        }
    }
    Ok(())
}

/// `name://` の後ろ(登録名とクエリ)をこの機械の登録で引き、ブローカーの種類と名前にする(naming.md §9.1)
fn resolve_name_url(name: &str) -> Result<(String, String)> {
    let spec = juiz_core::naming::NameSpec::parse_broker_name(name)?;
    let resolved = spec.resolve(&crate::name::AppProbe, &crate::name::app_kinds())?;
    Ok(resolved.type_and_name())
}

/// `ipc://robot_base`、`http://192.168.0.10:8100` を subsystem の conf にする
pub(crate) fn url_to_conf(url: &str) -> Result<Value> {
    let (type_name, name) = url
        .split_once("://")
        .filter(|(t, n)| !t.is_empty() && !n.is_empty())
        .ok_or_else(|| {
            anyhow!("'{url}' is not a broker URL (ex., ipc://robot_base, http://192.168.0.10:8100)")
        })?;
    let name = name.trim_end_matches('/');
    Ok(json!({ "type_name": type_name, "name": name }))
}

/// `kind: base` のパッケージを起動するとき、IPC ブローカー・HTTP ブローカーの設定を conf に足す(§10.1)
///
/// 返す値は `--ipc` の代わりに使う名前(conf にすでに同じ名前の ipc ブローカーがあれば None)。
pub(crate) fn apply_base(
    m: &Manifest,
    conf: &mut Value,
    ipc: Option<&str>,
) -> Result<Option<String>> {
    if let Some(name) = ipc {
        if name != m.name {
            bail!(
                "a base package always uses its package name as the IPC name ('{}'), but --ipc {name} was given",
                m.name
            );
        }
    }
    let obj = conf_object(conf)?;
    let has_ipc = obj
        .get("brokers")
        .and_then(Value::as_array)
        .is_some_and(|b| b.iter().any(|b| is_broker(b, "ipc", Some(&m.name))));
    if let Some(port) = m.base_http_port {
        let option = obj.entry("option").or_insert_with(|| json!({}));
        if option.is_null() {
            *option = json!({});
        }
        let option = option
            .as_object_mut()
            .ok_or_else(|| anyhow!("'option' in the conf must be a map"))?;
        let http = option.entry("http_broker").or_insert_with(|| json!({}));
        if !http.is_object() {
            *http = json!({});
        }
        let http = http.as_object_mut().unwrap();
        http.insert("start".to_owned(), json!(true));
        http.insert("port".to_owned(), json!(port));
    }
    Ok(if has_ipc { None } else { Some(m.name.clone()) })
}

/// system の profile に載せる `package`(§10.1)
pub(crate) fn set_package_profile(m: &Manifest, conf: &mut Value) -> Result<()> {
    conf_object(conf)?.insert(
        "package".to_owned(),
        json!({ "name": m.name, "version": m.version.to_string(), "kind": m.kind.as_str() }),
    );
    Ok(())
}

fn conf_object(conf: &mut Value) -> Result<&mut Map<String, Value>> {
    conf.as_object_mut()
        .ok_or_else(|| anyhow!("the system definition file must be a map"))
}

fn is_broker(b: &Value, type_name: &str, name: Option<&str>) -> bool {
    b.get("type_name").and_then(Value::as_str) == Some(type_name)
        && name.is_none_or(|n| b.get("name").and_then(Value::as_str).unwrap_or("juiz") == n)
}

/// depends の各つなぎ先を決め、conf の `subsystems` を置き換える(§10.2)
///
/// `cli` は `--depend` の (名前, URL)。conf の `subsystems` のうち `package` を持つものは depends のつなぎ先として扱い、
/// `--depend` で上書きされたものは取り除く。URL が `none` の依存にはつながず、返す一覧にも入れない(#452)。
/// どれにも無い依存は、この機械の登録(`names`)で引き、無ければ `ipc://<名前>`(naming.md §8)。
/// `cli` の値と conf の `name` を省いた subsystem は、種類の指定として登録を引く。
pub(crate) fn resolve(
    m: &Manifest,
    conf: &mut Value,
    cli: &[(String, String)],
    installed: &[Installed],
    names: &dyn NameList,
) -> Result<Vec<Depend>> {
    for (name, _) in cli {
        if !m.depends.contains_key(name) {
            bail!("--depend {name}=…: {} does not depend on {name}", m.name);
        }
    }
    let obj = conf_object(conf)?;
    let subsystems = match obj.remove("subsystems") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => a,
        Some(_) => bail!("'subsystems' in the conf must be a list"),
    };
    let mut others = Vec::new();
    let mut from_conf: Vec<(String, Value)> = Vec::new();
    for s in subsystems {
        match s.get("package") {
            None | Some(Value::Null) => others.push(s),
            Some(Value::String(p)) => {
                if !m.depends.contains_key(p) {
                    bail!(
                        "a subsystem in the conf has 'package: {p}', but {} does not depend on {p}. Add it to 'depends' in juizapp.yaml.",
                        m.name
                    );
                }
                if from_conf.iter().any(|(n, _)| n == p) {
                    bail!("two subsystems in the conf have 'package: {p}'");
                }
                from_conf.push((p.clone(), s));
            }
            Some(v) => bail!("'package' of a subsystem in the conf must be a string, but {v}"),
        }
    }
    let mut out = Vec::new();
    for (name, range) in &m.depends {
        let req = parse_req(&format!("the range of depends.{name}"), range)?;
        let cli_url = cli.iter().rev().find(|(n, _)| n == name).map(|(_, u)| u);
        if cli_url.is_some_and(|u| u == DEPEND_NONE) {
            continue;
        }
        // 登録を引いたら true(http のポートを補うときに引き直さない)
        let mut looked_up = false;
        let mut found = false;
        let mut sub = if let Some(url) = cli_url {
            match depend_kind(url) {
                Some(kind) => {
                    looked_up = true;
                    let (c, f) = by_kind(name, &req, kind, names)?;
                    found = f;
                    c
                }
                None => url_to_conf(url)?,
            }
        } else if let Some((_, s)) = from_conf.iter().find(|(n, _)| n == name) {
            let mut s = s.clone();
            if s.get("name").is_none_or(Value::is_null) {
                // name を省いた subsystem は、登録を引いて決める(type_name があればその種類だけ)
                looked_up = true;
                let kind = match s.get("type_name") {
                    None | Some(Value::Null) => None,
                    Some(Value::String(k)) => Some(k.clone()),
                    Some(v) => {
                        bail!("'type_name' of the subsystem for {name} must be a string, but {v}")
                    }
                };
                let (c, f) = match kind.as_deref() {
                    Some(k) => by_kind(name, &req, k, names)?,
                    None => match from_name_list(name, &req, None, names)? {
                        Some(c) => (c, true),
                        None => (json!({ "type_name": "ipc", "name": name }), false),
                    },
                };
                found = f;
                // timeout_ms などの他のキーは残す
                let obj = s.as_object_mut().unwrap();
                for (k, v) in c.as_object().unwrap() {
                    obj.insert(k.clone(), v.clone());
                }
            }
            s
        } else {
            looked_up = true;
            match from_name_list(name, &req, None, names)? {
                Some(c) => {
                    found = true;
                    c
                }
                None => json!({ "type_name": "ipc", "name": name }),
            }
        };
        if !sub.get("type_name").is_some_and(Value::is_string)
            || !sub.get("name").is_some_and(Value::is_string)
        {
            bail!("the subsystem for {name} must have 'type_name' and 'name'");
        }
        // name://<登録名> で書いたつなぎ先は、ここで実際のブローカーにする(naming.md §9.1)。
        // 種類が決まらないと、つなぎ返しのブローカーを足せないため
        if sub["type_name"] == juiz_core::naming::NAME_SCHEME {
            let (t, n) = resolve_name_url(sub["name"].as_str().unwrap())?;
            sub["type_name"] = json!(t);
            sub["name"] = json!(n);
            found = true;
        }
        sub.as_object_mut()
            .unwrap()
            .insert("package".to_owned(), json!(name));
        if sub["type_name"] == "http" && !sub["name"].as_str().unwrap().contains(':') {
            let host = sub["name"].as_str().unwrap().to_owned();
            // host がこの機械なら、まず登録のポートを使う(naming.md §8)
            let registered = if !looked_up && is_local_host(&host) {
                from_name_list(name, &req, Some("http"), names)
                    .ok()
                    .flatten()
                    .and_then(|c| {
                        c["name"]
                            .as_str()
                            .and_then(|n| n.rsplit_once(':'))
                            .and_then(|(_, p)| p.parse::<u16>().ok())
                    })
            } else {
                None
            };
            // 無ければ、インストール済みの基礎の base.http_port を使う
            let port = match registered {
                Some(p) => {
                    found = true;
                    p
                }
                None => find_newest(installed, name, Some(&req))
                    .filter(|i| i.manifest.kind == Kind::Base)
                    .and_then(|i| i.manifest.base_http_port)
                    .ok_or_else(|| {
                        anyhow!("cannot decide the port for {name}. Write the port in the conf or --depend.")
                    })?,
            };
            sub["name"] = json!(format!("{host}:{port}"));
        }
        out.push(Depend {
            name: name.clone(),
            range: range.clone(),
            req,
            conf: sub,
            found,
        });
    }
    let mut all: Vec<Value> = out.iter().map(|d| d.conf.clone()).collect();
    all.extend(others);
    if !all.is_empty() {
        obj.insert("subsystems".to_owned(), Value::Array(all));
    }
    rewrite_ids(m, obj, &out)?;
    Ok(out)
}

/// 依存の名前で書いた ID を、決まったつなぎ先のブローカー付きの ID にする(§10.4、#425)
///
/// - `package://<名前>/<残り>` → `<つなぎ先の URL>/<残り>`
/// - `ipc://<名前>/<残り>`(既定のつなぎ先のまま書いたもの)→ `<つなぎ先の URL>/<残り>`(移行のため)
/// - `--depend <名前>=none` の依存の `package://<名前>/…` はエラー。`ipc://<名前>/…` は変えない
///
/// 対象は `transforms` の `source`・`destination`、`connections` の `source`・`destination`、
/// `ecs` の `bind`(どれも文字列か `identifier` / `id` を持つ辞書)。
fn rewrite_ids(m: &Manifest, obj: &mut Map<String, Value>, deps: &[Depend]) -> Result<()> {
    let rewrite = |s: &str| -> Result<Option<String>> {
        if let Some(rest) = s.strip_prefix("package://") {
            let (name, path) = rest.split_once('/').unwrap_or((rest, ""));
            let Some(d) = deps.iter().find(|d| d.name == name) else {
                if m.depends.contains_key(name) {
                    // --depend <名前>=none でつながない依存(#452)
                    bail!("'{s}' in the conf refers to package {name}, but --depend {name}=none was given");
                }
                bail!(
                    "'{s}' in the conf refers to package {name}, but {} does not depend on {name}. Add it to 'depends' in juizapp.yaml.",
                    m.name
                );
            };
            if path.is_empty() {
                bail!("'{s}' in the conf must be package://{name}/<ID> (ex., package://{name}/container/odom0::static_anchor)");
            }
            return Ok(Some(format!("{}/{path}", d.url())));
        }
        if let Some(rest) = s.strip_prefix("ipc://") {
            if let Some((name, path)) = rest.split_once('/') {
                if let Some(d) = deps.iter().find(|d| d.name == name) {
                    return Ok(Some(format!("{}/{path}", d.url())));
                }
            }
        }
        Ok(None)
    };
    let rewrite_end = |v: &mut Value| -> Result<()> {
        let target = match v {
            Value::String(_) => Some(v),
            Value::Object(map) => ["identifier", "id"]
                .into_iter()
                .find(|k| map.get(*k).is_some_and(Value::is_string))
                .and_then(|k| map.get_mut(k)),
            _ => None,
        };
        if let Some(t) = target {
            if let Some(new) = rewrite(t.as_str().unwrap())? {
                *t = Value::String(new);
            }
        }
        Ok(())
    };
    for key in ["transforms", "connections"] {
        if let Some(Value::Array(items)) = obj.get_mut(key) {
            for item in items.iter_mut() {
                for end in ["source", "destination"] {
                    if let Some(v) = item.get_mut(end) {
                        rewrite_end(v)?;
                    }
                }
            }
        }
    }
    if let Some(Value::Array(ecs)) = obj.get_mut("ecs") {
        for ec in ecs.iter_mut() {
            if let Some(Value::Array(binds)) = ec.get_mut("bind") {
                for b in binds.iter_mut() {
                    rewrite_end(b)?;
                }
            }
        }
    }
    Ok(())
}

/// depends につなぐために、アプリ自身が持たなければならないブローカーを conf に足す
///
/// subsystem としてつなぐと、相手は同じ種類のブローカーでこちらへつなぎ返す(mastersystem)。
/// - ipc: conf に ipc ブローカーが無く `--ipc` も無ければ、`<アプリ名>-<プロセス ID>` の名前で足す。
/// - http: HTTP ブローカーを立てる(戻り値 true)。
/// - qmp・zenoh: conf にその種類のブローカーが無ければ、空いているポート(`port: 0`)で `qmp0`・`zenoh0` の名前で足す(naming.md §8)。
pub(crate) fn ensure_brokers(
    m: &Manifest,
    conf: &mut Value,
    depends: &[Depend],
    ipc: Option<&str>,
) -> Result<(Option<String>, bool)> {
    for kind in ["qmp", "zenoh"] {
        if !depends.iter().any(|d| d.type_name() == kind) {
            continue;
        }
        let obj = conf_object(conf)?;
        let brokers = obj.entry("brokers").or_insert_with(|| json!([]));
        if brokers.is_null() {
            *brokers = json!([]);
        }
        let brokers = brokers
            .as_array_mut()
            .ok_or_else(|| anyhow!("'brokers' in the conf must be a list"))?;
        if !brokers.iter().any(|b| is_broker(b, kind, None)) {
            // 名前は立てた後に実際の住所(`127.0.0.1:<ポート>`)に置き換わる
            brokers.push(json!({ "type_name": kind, "name": format!("{kind}0"), "port": 0 }));
        }
    }
    let needs_http = depends.iter().any(|d| d.type_name() == "http");
    let needs_ipc = depends.iter().any(|d| d.type_name() == "ipc");
    if !needs_ipc || ipc.is_some() {
        return Ok((ipc.map(str::to_owned), needs_http));
    }
    let has_ipc = conf_object(conf)?
        .get("brokers")
        .and_then(Value::as_array)
        .is_some_and(|b| b.iter().any(|b| is_broker(b, "ipc", None)));
    if has_ipc {
        return Ok((None, needs_http));
    }
    let pid = std::process::id().to_string();
    let keep = 64 - 1 - pid.len();
    let base: String = m.name.chars().take(keep).collect();
    Ok((Some(format!("{base}-{pid}")), needs_http))
}

/// つないだ相手の profile の `package` を確かめる(§10.3)
pub(crate) fn check_profile(app: &str, dep: &Depend, profile: &Value) -> Result<(), DependError> {
    let url = dep.url();
    let package = profile.get("package").filter(|p| p.is_object());
    let Some(package) = package else {
        return Err(DependError(format!(
            "{url} is running but is not started from a package. Start it with: juiz run {}",
            dep.name
        )));
    };
    let name = package.get("name").and_then(Value::as_str).unwrap_or("");
    if name != dep.name {
        return Err(DependError(format!("{url} is {name}, not {}.", dep.name)));
    }
    let version_str = package.get("version").and_then(Value::as_str).unwrap_or("");
    let ok = Version::parse(version_str).is_ok_and(|v| dep.req.matches(&v));
    if !ok {
        return Err(DependError(format!(
            "{app} needs {} {}, but {} {version_str} is running at {url}.",
            dep.name, dep.range, dep.name
        )));
    }
    Ok(())
}

/// つなげなかったときの文(§10.3)。インストールされているかで案内を変える
pub(crate) fn connect_error(app: &str, dep: &Depend, installed: bool) -> DependError {
    if installed {
        DependError(format!(
            "{app} depends on {} {}, but {} is not running at {}. Start it first with: juiz run {}",
            dep.name,
            dep.range,
            dep.name,
            dep.url(),
            dep.name
        ))
    } else {
        DependError(format!(
            "{app} depends on {} {}, but {} is not installed. Install it with: juiz install {}-<version>.juizapp",
            dep.name, dep.range, dep.name, dep.name
        ))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::path::PathBuf;

    /// 登録を引かない resolve(Step 2 より前のテストの形)
    fn resolve(
        m: &Manifest,
        conf: &mut Value,
        cli: &[(String, String)],
        installed: &[Installed],
    ) -> Result<Vec<Depend>> {
        super::resolve(m, conf, cli, installed, &NoNames)
    }

    fn manifest(text: &str) -> Manifest {
        Manifest::parse_str(text, "t").unwrap()
    }

    fn app() -> Manifest {
        manifest(
            "format: 1\nname: my_patrol\nversion: 0.1.0\ndepends:\n  robot_base: \">=1.2, <2\"\n",
        )
    }

    fn base(port: Option<u16>) -> Manifest {
        let port = port
            .map(|p| format!("base:\n  http_port: {p}\n"))
            .unwrap_or_default();
        manifest(&format!(
            "format: 1\nname: robot_base\nversion: 1.2.0\nkind: base\n{port}"
        ))
    }

    fn installed(m: Manifest) -> Installed {
        Installed {
            manifest: m,
            dir: PathBuf::from("/x"),
        }
    }

    #[test]
    fn base_adds_ipc_and_http() {
        let m = base(Some(8100));
        let mut conf =
            json!({ "name": "b", "option": { "http_broker": { "start": false, "port": 8000 } } });
        assert_eq!(
            apply_base(&m, &mut conf, None).unwrap().as_deref(),
            Some("robot_base")
        );
        assert_eq!(
            conf["option"]["http_broker"],
            json!({ "start": true, "port": 8100 })
        );
        // 同じ名前の ipc ブローカーが conf にあれば足さない
        let mut conf = json!({ "brokers": [ { "type_name": "ipc", "name": "robot_base" } ] });
        assert_eq!(apply_base(&base(None), &mut conf, None).unwrap(), None);
        assert!(conf.get("option").is_none());
        assert_eq!(
            apply_base(&m, &mut json!({}), Some("robot_base"))
                .unwrap()
                .as_deref(),
            Some("robot_base")
        );
        let e = apply_base(&m, &mut json!({}), Some("other"))
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("a base package always uses its package name as the IPC name"),
            "{e}"
        );
    }

    #[test]
    fn package_profile() {
        let mut conf = json!({ "name": "b" });
        set_package_profile(&base(None), &mut conf).unwrap();
        assert_eq!(
            conf["package"],
            json!({ "name": "robot_base", "version": "1.2.0", "kind": "base" })
        );
    }

    #[test]
    fn resolves_default_conf_and_cli() {
        let m = app();
        // 既定は ipc://<名前>。package を持たない subsystems は残す
        let mut conf =
            json!({ "subsystems": [ { "type_name": "http", "name": "localhost:9000" } ] });
        let d = resolve(&m, &mut conf, &[], &[]).unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].url(), "ipc://robot_base");
        assert_eq!(
            conf["subsystems"],
            json!([
                { "type_name": "ipc", "name": "robot_base", "package": "robot_base" },
                { "type_name": "http", "name": "localhost:9000" },
            ])
        );
        // conf の package 付きの subsystem。ポートを省けばインストール済みの基礎の base.http_port
        let conf_text = json!({ "subsystems": [ { "package": "robot_base", "type_name": "http", "name": "192.168.0.10" } ] });
        let mut conf = conf_text.clone();
        let d = resolve(&m, &mut conf, &[], &[installed(base(Some(8100)))]).unwrap();
        assert_eq!(d[0].url(), "http://192.168.0.10:8100");
        let mut conf = conf_text.clone();
        let e = resolve(&m, &mut conf, &[], &[installed(base(None))])
            .unwrap_err()
            .to_string();
        assert_eq!(
            e,
            "cannot decide the port for robot_base. Write the port in the conf or --depend."
        );
        // --depend が conf より先
        let mut conf = conf_text.clone();
        let cli = vec![("robot_base".to_owned(), "ipc://robot_base2".to_owned())];
        let d = resolve(&m, &mut conf, &cli, &[]).unwrap();
        assert_eq!(d[0].url(), "ipc://robot_base2");
        assert_eq!(conf["subsystems"].as_array().unwrap().len(), 1);
        let cli = vec![("robot_base".to_owned(), "http://10.0.0.1:8100".to_owned())];
        let d = resolve(&m, &mut json!({}), &cli, &[]).unwrap();
        assert_eq!(d[0].url(), "http://10.0.0.1:8100");
    }

    #[test]
    fn rewrites_ids_of_depends() {
        let m = app();
        let conf_text = json!({
            "transforms": [
                { "source": "slam_odom0", "destination": "ipc://robot_base/container/odom0::static_anchor" },
                { "source": "package://robot_base/container/map::periodic_anchor", "destination": "base0" },
                { "source": "ipc://other/container/x::static_anchor", "destination": "y" },
                { "source": "http://h:8000/container/x::static_anchor", "destination": "z" },
            ],
            "connections": [
                { "source": { "identifier": "package://robot_base/process/odom0::odom" }, "destination": "planner0", "arg_name": "odom" },
            ],
            "ecs": [ { "type_name": "main_loop_ec", "name": "ec0", "bind": [ "package://robot_base/process/a::b", { "name": "c" } ] } ],
        });
        // 既定のつなぎ先なら ipc://<名前>/ はそのまま、package:// は ipc://<名前>/ に
        let mut conf = conf_text.clone();
        resolve(&m, &mut conf, &[], &[]).unwrap();
        assert_eq!(conf["transforms"][0]["destination"], "ipc://robot_base/container/odom0::static_anchor");
        assert_eq!(conf["transforms"][1]["source"], "ipc://robot_base/container/map::periodic_anchor");
        assert_eq!(conf["connections"][0]["source"]["identifier"], "ipc://robot_base/process/odom0::odom");
        assert_eq!(conf["ecs"][0]["bind"][0], "ipc://robot_base/process/a::b");
        // --depend で http にすると、どちらの書き方も http のつなぎ先に。依存でない名前・別のブローカーは変えない
        let mut conf = conf_text.clone();
        let cli = vec![("robot_base".to_owned(), "http://192.168.0.10:8100".to_owned())];
        resolve(&m, &mut conf, &cli, &[]).unwrap();
        assert_eq!(conf["transforms"][0]["source"], "slam_odom0");
        assert_eq!(conf["transforms"][0]["destination"], "http://192.168.0.10:8100/container/odom0::static_anchor");
        assert_eq!(conf["transforms"][1]["source"], "http://192.168.0.10:8100/container/map::periodic_anchor");
        assert_eq!(conf["transforms"][2]["source"], "ipc://other/container/x::static_anchor");
        assert_eq!(conf["transforms"][3]["source"], "http://h:8000/container/x::static_anchor");
        assert_eq!(conf["connections"][0]["source"]["identifier"], "http://192.168.0.10:8100/process/odom0::odom");
        assert_eq!(conf["ecs"][0]["bind"][0], "http://192.168.0.10:8100/process/a::b");
        assert_eq!(conf["ecs"][0]["bind"][1], json!({ "name": "c" }));
        // ipc の別名でも
        let mut conf = conf_text.clone();
        let cli = vec![("robot_base".to_owned(), "ipc://robot_base2".to_owned())];
        resolve(&m, &mut conf, &cli, &[]).unwrap();
        assert_eq!(conf["transforms"][0]["destination"], "ipc://robot_base2/container/odom0::static_anchor");
        assert_eq!(conf["transforms"][1]["source"], "ipc://robot_base2/container/map::periodic_anchor");
        // conf の package 付き subsystem(ポートは基礎の base.http_port)
        let mut conf = conf_text.clone();
        conf["subsystems"] = json!([ { "package": "robot_base", "type_name": "http", "name": "10.0.0.2" } ]);
        resolve(&m, &mut conf, &[], &[installed(base(Some(8100)))]).unwrap();
        assert_eq!(conf["transforms"][0]["destination"], "http://10.0.0.2:8100/container/odom0::static_anchor");
        // 依存にない名前の package://、ID のない package:// はエラー
        let mut conf = json!({ "transforms": [ { "source": "package://arm/container/a::b", "destination": "x" } ] });
        let e = resolve(&m, &mut conf, &[], &[]).unwrap_err().to_string();
        assert_eq!(e, "'package://arm/container/a::b' in the conf refers to package arm, but my_patrol does not depend on arm. Add it to 'depends' in juizapp.yaml.");
        let mut conf = json!({ "transforms": [ { "source": "package://robot_base", "destination": "x" } ] });
        let e = resolve(&m, &mut conf, &[], &[]).unwrap_err().to_string();
        assert_eq!(e, "'package://robot_base' in the conf must be package://robot_base/<ID> (ex., package://robot_base/container/odom0::static_anchor)");
    }

    #[test]
    fn depend_none_drops_subsystem() {
        let m = manifest(
            "format: 1\nname: my_patrol\nversion: 0.1.0\ndepends:\n  robot_base: \">=1.2, <2\"\n  arm: \"1\"\n",
        );
        let none = vec![("robot_base".to_owned(), "none".to_owned())];
        // 既定のつなぎ先も、conf の package 付きの subsystem も足さない。package の無いものは残す
        let mut conf = json!({
            "subsystems": [
                { "package": "robot_base", "type_name": "ipc", "name": "robot_base" },
                { "type_name": "http", "name": "localhost:9000" },
            ],
            "transforms": [ { "source": "ipc://robot_base/container/a::b", "destination": "x" } ],
        });
        let d = resolve(&m, &mut conf, &none, &[]).unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].name, "arm");
        assert_eq!(
            conf["subsystems"],
            json!([
                { "type_name": "ipc", "name": "arm", "package": "arm" },
                { "type_name": "http", "name": "localhost:9000" },
            ])
        );
        // ipc://<名前>/ はそのまま
        assert_eq!(conf["transforms"][0]["source"], "ipc://robot_base/container/a::b");
        // 全部 none なら subsystems を書かない(package の無いものも無ければ)
        let all = vec![
            ("robot_base".to_owned(), "none".to_owned()),
            ("arm".to_owned(), "none".to_owned()),
        ];
        let mut conf = json!({ "subsystems": [ { "package": "arm", "type_name": "ipc", "name": "arm" } ] });
        assert!(resolve(&m, &mut conf, &all, &[]).unwrap().is_empty());
        assert!(conf.get("subsystems").is_none());
        let (ipc, http) = ensure_brokers(&m, &mut conf, &[], None).unwrap();
        assert_eq!((ipc, http), (None, false));
        // 後に書いた --depend が勝つ
        let back = vec![
            ("robot_base".to_owned(), "none".to_owned()),
            ("robot_base".to_owned(), "ipc://robot_base2".to_owned()),
        ];
        let d = resolve(&m, &mut json!({}), &back, &[]).unwrap();
        let urls: Vec<String> = d.iter().map(Depend::url).collect();
        assert_eq!(urls, ["ipc://arm", "ipc://robot_base2"]);
        // package://<名前>/ を使っているのに none ならエラー
        let mut conf = json!({ "transforms": [ { "source": "package://robot_base/container/a::b", "destination": "x" } ] });
        let e = resolve(&m, &mut conf, &none, &[]).unwrap_err().to_string();
        assert_eq!(e, "'package://robot_base/container/a::b' in the conf refers to package robot_base, but --depend robot_base=none was given");
        // 依存に無い名前の none はエラー
        let e = resolve(&m, &mut json!({}), &[("other".to_owned(), "none".to_owned())], &[])
            .unwrap_err()
            .to_string();
        assert!(e.contains("my_patrol does not depend on other"), "{e}");
    }

    #[test]
    fn remaps_topics() {
        let mut conf = json!({
            "processes": [
                { "name": "p0", "subscribes": { "a": "amcl_pose", "b": "scan" }, "publishes": [ "cmd_vel", "status" ] },
                { "name": "p1" },
            ],
            "containers": [ {
                "name": "dwa0",
                "processes": [
                    { "name": "c0", "subscribes": { "pose": "amcl_pose", "odom": { "name": "odom" } } },
                    { "name": "c1", "publishes": [ "cmd_vel", { "name": "cmd_vel" } ] },
                ],
            } ],
            "composite_containers": [ { "name": "cc0", "processes": [ { "name": "k0", "subscribes": { "x": "odom" } } ] } ],
            "transforms": [ { "source": "amcl_pose", "destination": "cmd_vel" } ],
        });
        let remaps = vec![
            parse_remap_arg("amcl_pose=nav/amcl_pose").unwrap(),
            parse_remap_arg("odom=nav/odom").unwrap(),
            parse_remap_arg("cmd_vel=cmd_vel_x").unwrap(),
            // 後のものが勝つ。付け替えた名前をもう 1 度付け替えない
            parse_remap_arg("cmd_vel=cmd_vel_dwa").unwrap(),
            parse_remap_arg("cmd_vel_dwa=other").unwrap(),
        ];
        remap_topics(&mut conf, &remaps).unwrap();
        assert_eq!(conf["processes"][0]["subscribes"], json!({ "a": "nav/amcl_pose", "b": "scan" }));
        assert_eq!(conf["processes"][0]["publishes"], json!([ "cmd_vel_dwa", "status" ]));
        assert_eq!(conf["processes"][1], json!({ "name": "p1" }));
        let c = &conf["containers"][0]["processes"];
        assert_eq!(c[0]["subscribes"], json!({ "pose": "nav/amcl_pose", "odom": { "name": "nav/odom" } }));
        assert_eq!(c[1]["publishes"], json!([ "cmd_vel_dwa", { "name": "cmd_vel_dwa" } ]));
        assert_eq!(conf["composite_containers"][0]["processes"][0]["subscribes"]["x"], "nav/odom");
        // トピック以外は変えない
        assert_eq!(conf["transforms"][0], json!({ "source": "amcl_pose", "destination": "cmd_vel" }));
        assert!(parse_remap_arg("cmd_vel").is_err());
        assert!(parse_remap_arg("=x").is_err());
        assert!(parse_remap_arg("x=").is_err());
    }

    #[test]
    fn resolve_rejects() {
        let m = app();
        let cli = vec![("other".to_owned(), "ipc://x".to_owned())];
        let e = resolve(&m, &mut json!({}), &cli, &[])
            .unwrap_err()
            .to_string();
        assert!(e.contains("my_patrol does not depend on other"), "{e}");
        let mut conf =
            json!({ "subsystems": [ { "package": "other", "type_name": "ipc", "name": "x" } ] });
        let e = resolve(&m, &mut conf, &[], &[]).unwrap_err().to_string();
        assert!(e.contains("my_patrol does not depend on other"), "{e}");
        assert!(parse_depend_arg("robot_base").is_err());
        assert!(url_to_conf("robot_base").is_err());
        assert_eq!(
            parse_depend_arg("a=ipc://b").unwrap(),
            ("a".to_owned(), "ipc://b".to_owned())
        );
    }

    #[test]
    fn adds_app_brokers() {
        let m = app();
        let mut conf = json!({});
        let d = resolve(&m, &mut conf, &[], &[]).unwrap();
        let (ipc, http) = ensure_brokers(&m, &mut conf, &d, None).unwrap();
        assert_eq!(ipc, Some(format!("my_patrol-{}", std::process::id())));
        assert!(!http);
        let mut conf = json!({ "brokers": [ { "type_name": "ipc", "name": "mine" } ] });
        assert_eq!(
            ensure_brokers(&m, &mut conf, &d, None).unwrap(),
            (None, false)
        );
        assert_eq!(
            ensure_brokers(&m, &mut conf, &d, Some("x")).unwrap(),
            (Some("x".to_owned()), false)
        );
        let cli = vec![("robot_base".to_owned(), "http://10.0.0.1:8100".to_owned())];
        let d = resolve(&m, &mut json!({}), &cli, &[]).unwrap();
        assert_eq!(
            ensure_brokers(&m, &mut json!({}), &d, None).unwrap(),
            (None, true)
        );
    }

    /// 決めた登録だけがある偽の名前の一覧
    pub(crate) struct FakeNames {
        pub regs: Vec<Registration>,
        pub kinds: Vec<&'static str>,
    }

    impl Default for FakeNames {
        fn default() -> Self {
            FakeNames {
                regs: vec![],
                kinds: vec!["ipc", "http", "qmp", "zenoh"],
            }
        }
    }

    impl NameList for FakeNames {
        fn bases(&self, name: &str, req: &VersionReq) -> Vec<Registration> {
            self.regs
                .iter()
                .filter(|r| is_base_of(r, name, req))
                .cloned()
                .collect()
        }
        fn kinds(&self) -> Vec<String> {
            self.kinds.iter().map(|k| k.to_string()).collect()
        }
    }

    /// 登録 1 つ。`brokers` は (種類, 名前)
    pub(crate) fn registration(
        name: &str,
        version: &str,
        kind: &str,
        brokers: &[(&str, &str)],
    ) -> Registration {
        Registration {
            format: 1,
            name: name.to_owned(),
            namespace: String::new(),
            uuid: format!("uuid-{name}-{version}"),
            pid: 1,
            host: "h".to_owned(),
            juiz_version: "0.3.6".to_owned(),
            started_at: "2026-11-25T00:00:00Z".to_owned(),
            package: Some(json!({ "name": name, "version": version, "kind": kind })),
            brokers: brokers
                .iter()
                .map(|(t, n)| juiz_core::naming::RegisteredBroker::new(t, n))
                .collect(),
        }
    }

    fn names(regs: Vec<Registration>) -> FakeNames {
        FakeNames {
            regs,
            kinds: vec!["ipc", "http", "qmp", "zenoh"],
        }
    }

    /// 基礎がポートを逃げて 8101 で動いている(ipc と http)
    fn moved_base() -> FakeNames {
        names(vec![registration(
            "robot_base",
            "1.3.0",
            "base",
            &[("ipc", "robot_base"), ("http", "0.0.0.0:8101")],
        )])
    }

    fn resolve_n(
        conf: &mut Value,
        cli: &[(&str, &str)],
        installed: &[Installed],
        names: &FakeNames,
    ) -> Result<Vec<Depend>> {
        let cli: Vec<(String, String)> = cli
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        super::resolve(&app(), conf, &cli, installed, names)
    }

    #[test]
    fn name_list_comes_after_written_ones() {
        let n = moved_base();
        // 何も書かなければ登録を引く。同じ機械では ipc が先
        let d = resolve_n(&mut json!({}), &[], &[], &n).unwrap();
        assert_eq!(d[0].url(), "ipc://robot_base");
        assert!(d[0].found);
        // ipc をつなげない juiz なら http(0.0.0.0 は 127.0.0.1 に)
        let no_ipc = FakeNames {
            regs: n.regs.clone(),
            kinds: vec!["http"],
        };
        let d = resolve_n(&mut json!({}), &[], &[], &no_ipc).unwrap();
        assert_eq!(d[0].url(), "http://127.0.0.1:8101");
        // --depend の URL、conf の subsystems は登録より勝つ
        let d = resolve_n(
            &mut json!({}),
            &[("robot_base", "http://10.0.0.1:8100")],
            &[],
            &n,
        )
        .unwrap();
        assert_eq!(d[0].url(), "http://10.0.0.1:8100");
        assert!(!d[0].found);
        let mut conf = json!({ "subsystems": [ { "package": "robot_base", "type_name": "ipc", "name": "rb2" } ] });
        let d = resolve_n(&mut conf, &[], &[], &n).unwrap();
        assert_eq!(d[0].url(), "ipc://rb2");
        assert!(!d[0].found);
        // 登録が無い、版が範囲外、基礎でないものは使わず、今の既定 ipc://<名前>
        for regs in [
            vec![],
            vec![registration(
                "robot_base",
                "2.0.0",
                "base",
                &[("http", "0.0.0.0:8101")],
            )],
            vec![registration(
                "robot_base",
                "1.3.0",
                "app",
                &[("http", "0.0.0.0:8101")],
            )],
        ] {
            let d = resolve_n(&mut json!({}), &[], &[], &names(regs)).unwrap();
            assert_eq!(d[0].url(), "ipc://robot_base");
            assert!(!d[0].found);
        }
        // つなげる種類のブローカーが無い登録も既定に任せる
        let only_zenoh = names(vec![registration(
            "robot_base",
            "1.3.0",
            "base",
            &[("zenoh", "0.0.0.0:7448")],
        )]);
        let no_zenoh = FakeNames {
            regs: only_zenoh.regs.clone(),
            kinds: vec!["ipc", "http"],
        };
        let d = resolve_n(&mut json!({}), &[], &[], &no_zenoh).unwrap();
        assert_eq!(d[0].url(), "ipc://robot_base");
    }

    #[test]
    fn depend_kind_looks_up_the_name_list() {
        let n = moved_base();
        let inst = [installed(base(Some(8100)))];
        // --depend <名前>=http は登録の http(逃げた後のポート)
        let d = resolve_n(&mut json!({}), &[("robot_base", "http")], &inst, &n).unwrap();
        assert_eq!(d[0].url(), "http://127.0.0.1:8101");
        assert!(d[0].found);
        // 登録が無ければ 127.0.0.1 と base.http_port
        let empty = names(vec![]);
        let d = resolve_n(&mut json!({}), &[("robot_base", "http")], &inst, &empty).unwrap();
        assert_eq!(d[0].url(), "http://127.0.0.1:8100");
        assert!(!d[0].found);
        // ipc は登録が無くても ipc://<名前>
        let d = resolve_n(&mut json!({}), &[("robot_base", "ipc")], &[], &empty).unwrap();
        assert_eq!(d[0].url(), "ipc://robot_base");
        // その種類が無い、登録が無い qmp・zenoh はエラー
        let e = resolve_n(&mut json!({}), &[("robot_base", "zenoh")], &[], &n)
            .unwrap_err()
            .to_string();
        assert_eq!(e, "robot_base has no zenoh broker (it has: ipc, http)");
        let e = resolve_n(&mut json!({}), &[("robot_base", "qmp")], &[], &empty)
            .unwrap_err()
            .to_string();
        assert_eq!(
            e,
            "robot_base is not registered on this machine, so its qmp broker cannot be found. Write where it is (ex., --depend robot_base=qmp://192.168.0.10:<port>)."
        );
        // 知らない種類は URL の書式の誤り
        let e = resolve_n(&mut json!({}), &[("robot_base", "tcp")], &[], &n)
            .unwrap_err()
            .to_string();
        assert!(e.starts_with("'tcp' is not a broker URL"), "{e}");
    }

    #[test]
    fn conf_without_name_looks_up_the_name_list() {
        let n = moved_base();
        // type_name だけなら、その種類を登録で引く。他のキーは残す
        let mut conf = json!({ "subsystems": [ { "package": "robot_base", "type_name": "http", "timeout_ms": 500 } ] });
        let d = resolve_n(&mut conf, &[], &[], &n).unwrap();
        assert_eq!(d[0].url(), "http://127.0.0.1:8101");
        assert_eq!(d[0].conf["timeout_ms"], 500);
        assert!(d[0].found);
        // package だけなら種類も登録で決める
        let mut conf = json!({ "subsystems": [ { "package": "robot_base" } ] });
        let d = resolve_n(&mut conf, &[], &[], &n).unwrap();
        assert_eq!(d[0].url(), "ipc://robot_base");
    }

    #[test]
    fn local_http_without_port_uses_the_registered_port() {
        let n = moved_base();
        let inst = [installed(base(Some(8100)))];
        for host in ["localhost", "127.0.0.1"] {
            let mut conf = json!({ "subsystems": [ { "package": "robot_base", "type_name": "http", "name": host } ] });
            let d = resolve_n(&mut conf, &[], &inst, &n).unwrap();
            assert_eq!(d[0].url(), format!("http://{host}:8101"));
            assert!(d[0].found);
        }
        // 別の host は base.http_port のまま
        let mut conf = json!({ "subsystems": [ { "package": "robot_base", "type_name": "http", "name": "192.168.0.10" } ] });
        let d = resolve_n(&mut conf, &[], &inst, &n).unwrap();
        assert_eq!(d[0].url(), "http://192.168.0.10:8100");
        assert!(!d[0].found);
        // --depend の URL でも同じ
        let d = resolve_n(
            &mut json!({}),
            &[("robot_base", "http://127.0.0.1")],
            &inst,
            &n,
        )
        .unwrap();
        assert_eq!(d[0].url(), "http://127.0.0.1:8101");
    }

    #[test]
    fn adds_qmp_and_zenoh_brokers_to_answer_back() {
        let m = app();
        for (kind, url) in [
            ("qmp", "qmp://127.0.0.1:8080"),
            ("zenoh", "zenoh://127.0.0.1:7448"),
        ] {
            let cli = vec![("robot_base".to_owned(), url.to_owned())];
            let d = resolve(&m, &mut json!({}), &cli, &[]).unwrap();
            let mut conf = json!({});
            assert_eq!(
                ensure_brokers(&m, &mut conf, &d, None).unwrap(),
                (None, false)
            );
            assert_eq!(
                conf["brokers"],
                json!([ { "type_name": kind, "name": format!("{kind}0"), "port": 0 } ])
            );
            // conf にあれば足さない
            let mut conf = json!({ "brokers": [ { "type_name": kind, "port": 5000 } ] });
            ensure_brokers(&m, &mut conf, &d, None).unwrap();
            assert_eq!(conf["brokers"].as_array().unwrap().len(), 1);
        }
    }

    #[test]
    fn checks_profile() {
        let m = app();
        let d = resolve(&m, &mut json!({}), &[], &[]).unwrap().remove(0);
        let ok = json!({ "package": { "name": "robot_base", "version": "1.4.0", "kind": "base" } });
        check_profile("my_patrol", &d, &ok).unwrap();
        let e = check_profile("my_patrol", &d, &json!({})).unwrap_err().0;
        assert_eq!(e, "ipc://robot_base is running but is not started from a package. Start it with: juiz run robot_base");
        let e = check_profile(
            "my_patrol",
            &d,
            &json!({ "package": { "name": "arm", "version": "1.2.0" } }),
        )
        .unwrap_err()
        .0;
        assert_eq!(e, "ipc://robot_base is arm, not robot_base.");
        let e = check_profile(
            "my_patrol",
            &d,
            &json!({ "package": { "name": "robot_base", "version": "2.0.1" } }),
        )
        .unwrap_err()
        .0;
        assert_eq!(e, "my_patrol needs robot_base >=1.2, <2, but robot_base 2.0.1 is running at ipc://robot_base.");
        assert_eq!(
            connect_error("my_patrol", &d, false).0,
            "my_patrol depends on robot_base >=1.2, <2, but robot_base is not installed. Install it with: juiz install robot_base-<version>.juizapp"
        );
        assert_eq!(
            connect_error("my_patrol", &d, true).0,
            "my_patrol depends on robot_base >=1.2, <2, but robot_base is not running at ipc://robot_base. Start it first with: juiz run robot_base"
        );
    }
}
