//! 基礎パッケージを起動するときの conf の足し方と、アプリの depends の解決・確認
//! (docs/spec/app-package.md §10、#286)

use super::install::{find_newest, Installed};
use super::manifest::{parse_req, Kind, Manifest};
use juiz_core::anyhow::{anyhow, bail, Result};
use juiz_sdk::serde_json::{json, Map, Value};
use semver::{Version, VersionReq};

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

/// `--depend <名前>=<URL>` を分ける
pub(crate) fn parse_depend_arg(s: &str) -> Result<(String, String)> {
    let (name, url) = s.split_once('=').ok_or_else(|| {
        anyhow!("--depend must be <name>=<URL> (ex., robot_base=ipc://robot_base2), but '{s}'")
    })?;
    if name.is_empty() || url.is_empty() {
        bail!("--depend must be <name>=<URL> (ex., robot_base=ipc://robot_base2), but '{s}'");
    }
    Ok((name.to_owned(), url.to_owned()))
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
/// `--depend` で上書きされたものは取り除く。
pub(crate) fn resolve(
    m: &Manifest,
    conf: &mut Value,
    cli: &[(String, String)],
    installed: &[Installed],
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
        let mut sub = if let Some((_, url)) = cli.iter().rev().find(|(n, _)| n == name) {
            url_to_conf(url)?
        } else if let Some((_, s)) = from_conf.iter().find(|(n, _)| n == name) {
            s.clone()
        } else {
            json!({ "type_name": "ipc", "name": name })
        };
        if !sub.get("type_name").is_some_and(Value::is_string)
            || !sub.get("name").is_some_and(Value::is_string)
        {
            bail!("the subsystem for {name} must have 'type_name' and 'name'");
        }
        sub.as_object_mut()
            .unwrap()
            .insert("package".to_owned(), json!(name));
        if sub["type_name"] == "http" && !sub["name"].as_str().unwrap().contains(':') {
            // ポートを省いたら、インストール済みの基礎の base.http_port を使う
            let port = find_newest(installed, name, Some(&req))
                .filter(|i| i.manifest.kind == Kind::Base)
                .and_then(|i| i.manifest.base_http_port)
                .ok_or_else(|| {
                    anyhow!("cannot decide the port for {name}. Write the port in the conf or --depend.")
                })?;
            let host = sub["name"].as_str().unwrap().to_owned();
            sub["name"] = json!(format!("{host}:{port}"));
        }
        out.push(Depend {
            name: name.clone(),
            range: range.clone(),
            req,
            conf: sub,
        });
    }
    let mut all: Vec<Value> = out.iter().map(|d| d.conf.clone()).collect();
    all.extend(others);
    if !all.is_empty() {
        obj.insert("subsystems".to_owned(), Value::Array(all));
    }
    Ok(out)
}

/// depends につなぐために、アプリ自身が持たなければならないブローカーを conf に足す
///
/// subsystem としてつなぐと、相手は同じ種類のブローカーでこちらへつなぎ返す(mastersystem)。
/// - ipc: conf に ipc ブローカーが無く `--ipc` も無ければ、`<アプリ名>-<プロセス ID>` の名前で足す。
/// - http: HTTP ブローカーを立てる(戻り値 true)。
pub(crate) fn ensure_brokers(
    m: &Manifest,
    conf: &mut Value,
    depends: &[Depend],
    ipc: Option<&str>,
) -> Result<(Option<String>, bool)> {
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
mod tests {
    use super::*;
    use std::path::PathBuf;

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
