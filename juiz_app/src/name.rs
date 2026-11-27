//! この機械の名前の一覧 (docs/spec/naming.md §9.3、issue #616)
//!
//! ```text
//! juiz name list [--check] [--lan] [--peer <URL>]... [--json]
//! juiz name resolve <登録名> [--all] [--lan] [--peer <URL>]... [--json]
//! ```
//!
//! どちらもブローカーを立てない (System を作らない)。登録ファイルを直接読み、死活を確かめる。
//! feature `naming-mdns` の juiz は LAN (mDNS、§11) も引く。`--peer`・`JUIZ_NAMING_PEERS` の juiz にも聞く (§12)。
//! `resolve` はこの機械で見つからなければ LAN と peers を見る。

use std::io::{BufRead, IsTerminal, Write};

use clap::Subcommand;
use juiz_core::anyhow::{anyhow, Result};
use juiz_core::naming::peers::{env_peers, parse_peers, Peer};
use juiz_core::naming::{
    ambiguous_message, core_kinds, find_candidates, list_all, not_registered_message, Candidate,
    CoreProbe, LanLookup, ListedRegistration, Looked, NameQuery, NameSpec, Place, Probe,
    ProfileProbe, RegisteredBroker, Resolved,
};
use juiz_core::prelude::*;

#[derive(Debug, Subcommand, Clone)]
pub(crate) enum NameSubCommands {
    /// List the juiz running on this machine with their brokers (docs/spec/naming.md §9.3)
    List {
        /// Ask each juiz for its profile and mark the ones that do not answer
        #[arg(long = "check")]
        check: bool,

        /// Also list the juiz advertised on the LAN (mDNS; needs a juiz built with the feature naming-mdns) and the ones the peers know
        #[arg(long = "lan")]
        lan: bool,

        /// Also list the juiz this juiz knows (ex., http://192.168.0.10:8000). Can be written more than once
        #[arg(long = "peer", value_name = "URL")]
        peer: Vec<String>,

        /// Print JSON
        #[arg(long = "json")]
        json: bool,
    },

    /// Find a juiz by its registered name and print the URL of the broker to connect to
    Resolve {
        /// Registered name: name, namespace/name, /name (root namespace) or name@machine
        name: String,

        /// Print all the candidates with their brokers, without choosing one
        #[arg(long = "all")]
        all: bool,

        /// Look at the LAN (mDNS) and the peers too, even when the name is found on this machine
        #[arg(long = "lan")]
        lan: bool,

        /// A juiz to ask when the name is not found on this machine (ex., http://192.168.0.10:8000). Can be written more than once
        #[arg(long = "peer", value_name = "URL")]
        peer: Vec<String>,

        /// Print JSON
        #[arg(long = "json")]
        json: bool,
    },
}

/// juiz コマンドが問い合わせられる種類 (juiz_core の ipc・http に、組み込んだ qmp を足す)
pub(crate) struct AppProbe;

impl ProfileProbe for AppProbe {
    #[allow(unused_variables)]
    fn probe(&self, broker: &RegisteredBroker, url: &str) -> Probe {
        #[cfg(feature = "qmp-broker")]
        if broker.type_name == "qmp" {
            let Some((host, port)) = url
                .strip_prefix("qmp://")
                .and_then(|r| r.split('/').next())
                .and_then(|a| a.rsplit_once(':'))
                .and_then(|(h, p)| p.parse::<i64>().ok().map(|p| (h.to_owned(), p)))
            else {
                return Probe::Unsupported;
            };
            return match qmp_broker::fetch_profile_blocking(&host, port)
                .as_ref()
                .and_then(|v| {
                    v.get("system_store")?
                        .get("uuid")?
                        .as_str()
                        .map(str::to_owned)
                }) {
                Some(u) => Probe::Uuid(u),
                None => Probe::NoResponse,
            };
        }
        CoreProbe.probe(broker, url)
    }
}

/// juiz コマンドがつなげる種類
pub(crate) fn app_kinds() -> Vec<String> {
    #[allow(unused_mut)]
    let mut v = core_kinds();
    #[cfg(feature = "qmp-broker")]
    v.push("qmp".to_owned());
    // 置き場所に入れたブローカー (`juiz broker install zenoh` など、broker-plugin-abi.md §10) にもつなげる (#640)
    if let Some(home) = juiz_core::installed::broker_home() {
        for b in juiz_core::installed::list_installed(&home) {
            if b.status == juiz_core::installed::InstalledStatus::Used && !v.contains(&b.type_name) {
                v.push(b.type_name);
            }
        }
    }
    v
}

/// `juiz name` を実行して終了コードを返す。`global_peers` は `juiz --peer <URL> name …` の分
pub(crate) fn run_name_command(cmd: &NameSubCommands, global_peers: &[String]) -> i32 {
    match on_name(cmd, global_peers) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e:#}");
            1
        }
    }
}

/// `--lan` を feature の無い juiz で書いたときの警告
fn warn_no_lan(lan: bool) {
    if lan && !juiz_core::naming::lan::available() {
        eprintln!("warning: --lan is ignored: this juiz is built without the feature naming-mdns");
    }
}

/// `--peer` で書いた peers を読む。読めないものはエラー (引数の誤りなので)
fn cli_peers(written: &[String]) -> Result<Vec<Peer>> {
    for w in written {
        juiz_core::naming::peers::parse_peer(w)?;
    }
    Ok(parse_peers(written.iter().map(String::as_str)))
}

/// `--peer` と `JUIZ_NAMING_PEERS` の peers (§12.1)
fn all_peers(written: &[String]) -> Result<Vec<Peer>> {
    let mut w = written.to_vec();
    w.extend(env_peers());
    cli_peers(&w)
}

fn on_name(cmd: &NameSubCommands, global_peers: &[String]) -> Result<()> {
    let kinds = app_kinds();
    match cmd {
        NameSubCommands::List {
            check,
            lan,
            peer,
            json,
        } => {
            warn_no_lan(*lan);
            let written: Vec<String> = global_peers.iter().chain(peer).cloned().collect();
            // 一覧は既定でこの機械だけ。--lan なら LAN と peers (JUIZ_NAMING_PEERS を含む)、--peer なら書いた peers
            let peers = if *lan {
                all_peers(&written)?
            } else {
                cli_peers(&written)?
            };
            let listed = list_all(*check, *lan, &peers, &AppProbe, &kinds)?;
            if *json {
                let v: Vec<Value> = listed.iter().map(ListedRegistration::to_json).collect();
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                print!(
                    "{}",
                    format_list(
                        &listed,
                        Looked {
                            lan: *lan && juiz_core::naming::lan::available(),
                            peers: !peers.is_empty(),
                        }
                    )
                );
            }
            Ok(())
        }
        NameSubCommands::Resolve {
            name,
            all,
            lan,
            peer,
            json,
        } => {
            warn_no_lan(*lan);
            let query = NameQuery::parse(name)?;
            let written: Vec<String> = global_peers.iter().chain(peer).cloned().collect();
            let peers = all_peers(&written)?;
            let (candidates, looked) = find_candidates(
                &query,
                &AppProbe,
                &kinds,
                LanLookup::from_flag(*lan),
                &peers,
            )?;
            if *all {
                if *json {
                    let v: Vec<Value> = candidates.iter().map(candidate_json).collect();
                    println!("{}", serde_json::to_string_pretty(&v)?);
                } else {
                    for c in &candidates {
                        println!("{}", c.describe());
                        for b in &c.registration.brokers {
                            if c.place.is_remote() && b.type_name == "ipc" {
                                continue;
                            }
                            println!("    {}", b.local_url());
                        }
                    }
                }
                return Ok(());
            }
            let chosen = choose(&query, candidates, looked)?;
            let (_, url) = chosen.select_broker(None, &kinds)?;
            if *json {
                let mut v = candidate_json(&chosen);
                if let Some(m) = v.as_object_mut() {
                    m.insert("url".to_owned(), jvalue!(url));
                }
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                println!("{url}");
            }
            Ok(())
        }
    }
}

/// CLI の `-s` の値が名前(`name://…` か、`://` も `:` も無いもの)なら、この機械の登録で引いて
/// つなぐブローカーの URL を返す(docs/spec/naming.md §9.2)。名前でなければ `None`。
/// この機械に無ければ LAN と peers(`--peer`・`JUIZ_NAMING_PEERS`、§12)を見る
pub(crate) fn resolve_server(server: &str, peers: &[String]) -> Result<Option<String>> {
    let Some(spec) = NameSpec::parse_server(server)? else {
        return Ok(None);
    };
    let kinds = app_kinds();
    let peers = all_peers(peers)?;
    let (candidates, looked) =
        find_candidates(&spec.query, &AppProbe, &kinds, LanLookup::Fallback, &peers)?;
    let chosen = choose(&spec.query, candidates, looked)?;
    let resolved = Resolved::new(chosen, spec.kind.as_deref(), &kinds)?;
    log::info!("{server} is resolved to {}", resolved.url);
    Ok(Some(resolved.url))
}

fn candidate_json(c: &Candidate) -> Value {
    ListedRegistration {
        registration: c.registration.clone(),
        alive: Some(true),
        place: c.place,
    }
    .to_json()
}

/// 候補を 1 つに決める (§7.2)。端末なら選ばせ、そうでなければ複数はエラー。`looked` はほかに見た所
fn choose(query: &NameQuery, mut candidates: Vec<Candidate>, looked: Looked) -> Result<Candidate> {
    match candidates.len() {
        0 => Err(anyhow!(not_registered_message(query, looked))),
        1 => Ok(candidates.remove(0)),
        _ => {
            let tty = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
            if !tty {
                return Err(anyhow!(ambiguous_message(query, &candidates)));
            }
            let stdin = std::io::stdin();
            let i = ask_choice(
                query,
                &candidates,
                &mut stdin.lock(),
                &mut std::io::stderr(),
            )?;
            Ok(candidates.remove(i))
        }
    }
}

/// 候補を並べて番号を読む。空行は 1 番
fn ask_choice(
    query: &NameQuery,
    candidates: &[Candidate],
    input: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Result<usize> {
    writeln!(out, "{} juiz are registered as {query}:", candidates.len())?;
    for (i, c) in candidates.iter().enumerate() {
        writeln!(out, "  {}) {}", i + 1, c.describe())?;
    }
    loop {
        write!(out, "choose [1]: ")?;
        out.flush()?;
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Err(anyhow!(ambiguous_message(query, candidates)));
        }
        let t = line.trim();
        if t.is_empty() {
            return Ok(0);
        }
        match t.parse::<usize>() {
            Ok(n) if (1..=candidates.len()).contains(&n) => return Ok(n - 1),
            _ => writeln!(out, "type a number from 1 to {}", candidates.len())?,
        }
    }
}

/// `juiz name list` の表。`lan` は LAN も見たとき (MACHINE の列を足す)
fn format_list(listed: &[ListedRegistration], looked: Looked) -> String {
    if listed.is_empty() {
        let q = NameQuery {
            namespace: None,
            name: "x".to_owned(),
            host: None,
        };
        // 「No juiz is registered on …」は §7.2 の文と同じ所の並べ方にする
        let m = not_registered_message(&q, looked);
        return format!("{}\n", m.replacen("x is not registered", "No juiz is registered", 1));
    }
    let rows: Vec<[String; 6]> = listed
        .iter()
        .map(|l| {
            let r = &l.registration;
            let package = match (r.package_name(), r.package_version()) {
                (Some(n), Some(v)) => format!("{n} {v}"),
                (Some(n), None) => n.to_owned(),
                _ => "-".to_owned(),
            };
            let mut brokers = r
                .brokers
                .iter()
                .filter(|b| l.place == Place::ThisMachine || b.type_name != "ipc")
                .map(|b| b.local_url())
                .collect::<Vec<_>>()
                .join(" ");
            if l.alive == Some(false) {
                brokers.push_str("  (not answering)");
            }
            let machine = match l.place {
                Place::ThisMachine => "(this)".to_owned(),
                Place::Lan | Place::Peer => r.host.clone(),
            };
            [
                r.registered_name(),
                machine,
                package,
                r.pid.to_string(),
                r.started_at.clone(),
                brokers,
            ]
        })
        .collect();
    let header = ["NAME", "MACHINE", "PACKAGE", "PID", "STARTED", "BROKERS"];
    // LAN を見ないときは MACHINE の列を出さない (今までの表のまま)
    let skip = if looked.any() { usize::MAX } else { 1 };
    let mut widths = header.map(str::len);
    for row in &rows {
        for (i, c) in row.iter().enumerate() {
            widths[i] = widths[i].max(c.chars().count());
        }
    }
    let line = |cols: [&str; 6]| -> String {
        let mut s = String::new();
        for (i, c) in cols.iter().enumerate() {
            if i == skip {
                continue;
            }
            if i == cols.len() - 1 {
                s.push_str(c);
            } else {
                s.push_str(&format!("{c:<w$}  ", w = widths[i]));
            }
        }
        s.trim_end().to_owned() + "\n"
    };
    let mut out = line(header);
    for r in &rows {
        out.push_str(&line([&r[0], &r[1], &r[2], &r[3], &r[4], &r[5]]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use juiz_core::naming::{Registration, FORMAT};

    fn reg(ns: &str, name: &str) -> Registration {
        Registration {
            format: FORMAT,
            name: name.into(),
            namespace: ns.into(),
            uuid: "u".into(),
            pid: 12345,
            host: "pc".into(),
            juiz_version: "0".into(),
            started_at: "2026-11-25T06:00:00Z".into(),
            package: Some(jvalue!({"name": name, "version": "0.4.0", "kind": "base"})),
            brokers: vec![
                RegisteredBroker::new("ipc", name),
                RegisteredBroker::new("http", "0.0.0.0:8001"),
            ],
        }
    }

    #[test]
    fn list_table() {
        let listed = vec![
            ListedRegistration {
                registration: reg("robot1", "mobile_sim"),
                alive: None,
                place: Place::ThisMachine,
            },
            ListedRegistration {
                registration: reg("", "teleop"),
                alive: Some(false),
                place: Place::ThisMachine,
            },
        ];
        let t = format_list(&listed, Looked::default());
        let lines: Vec<&str> = t.lines().collect();
        assert!(lines[0].starts_with("NAME"), "{t}");
        assert!(
            lines[1].starts_with("robot1/mobile_sim  mobile_sim 0.4.0  12345"),
            "{t}"
        );
        assert!(
            lines[1].ends_with("ipc://mobile_sim http://127.0.0.1:8001"),
            "{t}"
        );
        assert!(lines[2].ends_with("(not answering)"), "{t}");
        assert_eq!(
            format_list(&[], Looked::default()),
            "No juiz is registered on this machine.\n"
        );
        assert_eq!(
            format_list(
                &[],
                Looked {
                    lan: true,
                    peers: false
                }
            ),
            "No juiz is registered on this machine or the LAN.\n"
        );
        assert_eq!(
            format_list(
                &[],
                Looked {
                    lan: false,
                    peers: true
                }
            ),
            "No juiz is registered on this machine or the peers.\n"
        );
    }

    #[test]
    fn list_table_with_lan() {
        let mut remote = reg("robot2", "mobile_sim");
        remote.host = "robot2-pc".into();
        remote.brokers = vec![RegisteredBroker::new("http", "192.168.0.12:8001")];
        let listed = vec![
            ListedRegistration {
                registration: reg("robot1", "mobile_sim"),
                alive: None,
                place: Place::ThisMachine,
            },
            ListedRegistration {
                registration: remote,
                alive: None,
                place: Place::Lan,
            },
        ];
        let t = format_list(
            &listed,
            Looked {
                lan: true,
                peers: false,
            },
        );
        let lines: Vec<&str> = t.lines().collect();
        assert!(lines[0].starts_with("NAME               MACHINE"), "{t}");
        assert!(lines[1].starts_with("robot1/mobile_sim  (this)"), "{t}");
        assert!(lines[2].starts_with("robot2/mobile_sim  robot2-pc"), "{t}");
        assert!(lines[2].ends_with("http://192.168.0.12:8001"), "{t}");
    }

    #[test]
    fn choose_by_number() {
        let q = NameQuery::parse("mobile_sim").unwrap();
        let c = vec![
            Candidate::local(reg("robot1", "mobile_sim")),
            Candidate::lan({
                let mut r = reg("robot2", "mobile_sim");
                r.host = "robot2-pc".into();
                r
            }),
        ];
        let mut out = Vec::new();
        let i = ask_choice(&q, &c, &mut "x\n2\n".as_bytes(), &mut out).unwrap();
        assert_eq!(i, 1);
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("2 juiz are registered as mobile_sim:\n  1) robot1/mobile_sim  0.4.0  pid 12345  (this machine)\n  2) robot2/mobile_sim  0.4.0  at robot2-pc\n"), "{text}");
        assert!(text.contains("type a number from 1 to 2"), "{text}");
        let i = ask_choice(&q, &c, &mut "\n".as_bytes(), &mut Vec::new()).unwrap();
        assert_eq!(i, 0);
        // 入力が閉じたら、対話できないときと同じエラー
        let e = ask_choice(&q, &c, &mut "".as_bytes(), &mut Vec::new()).unwrap_err();
        assert_eq!(e.to_string(), "mobile_sim is ambiguous: robot1/mobile_sim (this machine), robot2/mobile_sim@robot2-pc. Write the namespace or the machine (ex., robot1/mobile_sim).");
    }
}
