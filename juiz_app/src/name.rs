//! この機械の名前の一覧 (docs/spec/naming.md §9.3、issue #616)
//!
//! ```text
//! juiz name list [--check] [--json]
//! juiz name resolve <登録名> [--all] [--json]
//! ```
//!
//! どちらもブローカーを立てない (System を作らない)。登録ファイルを直接読み、死活を確かめる。

use std::io::{BufRead, IsTerminal, Write};

use clap::Subcommand;
use juiz_core::anyhow::{anyhow, Result};
use juiz_core::naming::{
    ambiguous_message, core_kinds, not_registered_message, Candidate, CoreProbe,
    ListedRegistration, NameQuery, NameRegistry, Probe, ProfileProbe, RegisteredBroker,
};
use juiz_core::prelude::*;

#[derive(Debug, Subcommand, Clone)]
pub(crate) enum NameSubCommands {
    /// List the juiz running on this machine with their brokers (docs/spec/naming.md §9.3)
    List {
        /// Ask each juiz for its profile and mark the ones that do not answer
        #[arg(long = "check")]
        check: bool,

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
    #[cfg(feature = "zenoh-broker")]
    v.push("zenoh".to_owned());
    v
}

/// `juiz name` を実行して終了コードを返す
pub(crate) fn run_name_command(cmd: &NameSubCommands) -> i32 {
    match on_name(cmd) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e:#}");
            1
        }
    }
}

fn on_name(cmd: &NameSubCommands) -> Result<()> {
    let registry = NameRegistry::open()?;
    let kinds = app_kinds();
    match cmd {
        NameSubCommands::List { check, json } => {
            let listed = registry.list(*check, &AppProbe, &kinds);
            if *json {
                let v: Vec<Value> = listed.iter().map(ListedRegistration::to_json).collect();
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                print!("{}", format_list(&listed));
            }
            Ok(())
        }
        NameSubCommands::Resolve { name, all, json } => {
            let query = NameQuery::parse(name)?;
            let candidates = registry.candidates(&query, &AppProbe, &kinds);
            if *all {
                if *json {
                    let v: Vec<Value> = candidates.iter().map(candidate_json).collect();
                    println!("{}", serde_json::to_string_pretty(&v)?);
                } else {
                    for c in &candidates {
                        println!("{}", c.describe());
                        for b in &c.registration.brokers {
                            println!("    {}", b.local_url());
                        }
                    }
                }
                return Ok(());
            }
            let chosen = choose(&query, candidates)?;
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

fn candidate_json(c: &Candidate) -> Value {
    ListedRegistration {
        registration: c.registration.clone(),
        alive: Some(true),
    }
    .to_json()
}

/// 候補を 1 つに決める (§7.2)。端末なら選ばせ、そうでなければ複数はエラー
fn choose(query: &NameQuery, mut candidates: Vec<Candidate>) -> Result<Candidate> {
    match candidates.len() {
        0 => Err(anyhow!(not_registered_message(query))),
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

/// `juiz name list` の表
fn format_list(listed: &[ListedRegistration]) -> String {
    if listed.is_empty() {
        return "No juiz is registered on this machine.\n".to_owned();
    }
    let rows: Vec<[String; 5]> = listed
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
                .map(|b| b.local_url())
                .collect::<Vec<_>>()
                .join(" ");
            if l.alive == Some(false) {
                brokers.push_str("  (not answering)");
            }
            [
                r.registered_name(),
                package,
                r.pid.to_string(),
                r.started_at.clone(),
                brokers,
            ]
        })
        .collect();
    let header = ["NAME", "PACKAGE", "PID", "STARTED", "BROKERS"];
    let mut widths = header.map(str::len);
    for row in &rows {
        for (i, c) in row.iter().enumerate() {
            widths[i] = widths[i].max(c.chars().count());
        }
    }
    let line = |cols: [&str; 5]| -> String {
        let mut s = String::new();
        for (i, c) in cols.iter().enumerate() {
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
        out.push_str(&line([&r[0], &r[1], &r[2], &r[3], &r[4]]));
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
            },
            ListedRegistration {
                registration: reg("", "teleop"),
                alive: Some(false),
            },
        ];
        let t = format_list(&listed);
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
        assert_eq!(format_list(&[]), "No juiz is registered on this machine.\n");
    }

    #[test]
    fn choose_by_number() {
        let q = NameQuery::parse("mobile_sim").unwrap();
        let c = vec![
            Candidate {
                registration: reg("robot1", "mobile_sim"),
            },
            Candidate {
                registration: reg("robot2", "mobile_sim"),
            },
        ];
        let mut out = Vec::new();
        let i = ask_choice(&q, &c, &mut "x\n2\n".as_bytes(), &mut out).unwrap();
        assert_eq!(i, 1);
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("2 juiz are registered as mobile_sim:\n  1) robot1/mobile_sim  0.4.0  pid 12345  (this machine)\n"), "{text}");
        assert!(text.contains("type a number from 1 to 2"), "{text}");
        let i = ask_choice(&q, &c, &mut "\n".as_bytes(), &mut Vec::new()).unwrap();
        assert_eq!(i, 0);
        // 入力が閉じたら、対話できないときと同じエラー
        let e = ask_choice(&q, &c, &mut "".as_bytes(), &mut Vec::new()).unwrap_err();
        assert!(e.to_string().starts_with("mobile_sim is ambiguous"), "{e}");
    }
}
