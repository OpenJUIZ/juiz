//! `juiz run` の起動前の確認と対話(docs/spec/app-package.md §15.1〜§15.3、issue #393)
//!
//! `System::setup` の前に、立てるブローカーの待ち受けを試しに取ってすぐ放し、使用中かを確かめる。
//! 使用中なら相手を調べ、対話できるときは利用者に選ばせる。選んだ番号は conf に入れて起動する。

use super::manifest::Kind;
use juiz_core::anyhow::{bail, Result};
use juiz_core::port_policy::{
    master_info, probe_host, PortFallback, PortPolicy, PORT_FALLBACK_RANGE,
};
use juiz_core::prelude::*;
use std::io::{BufRead, Write};

/// 利用者に選択を出すかどうか(§15.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// 選択を出して 1 行読む
    Interactive,
    /// 選択を出さず、各選択の既定(推奨)を選ぶ(`--yes`)
    Yes,
    /// 選択を出さず、対話できないときの扱いにする(`--no-prompt`、または端末でないとき)
    NoPrompt,
}

impl Mode {
    /// `--yes`・`--no-prompt` と、標準入力・標準エラーが端末かどうかから決める。両方付けたら `--no-prompt`
    pub(crate) fn from_flags(yes: bool, no_prompt: bool, is_terminal: bool) -> Mode {
        if no_prompt {
            Mode::NoPrompt
        } else if yes {
            Mode::Yes
        } else if is_terminal {
            Mode::Interactive
        } else {
            Mode::NoPrompt
        }
    }

    /// 今の標準入力・標準エラーで決める
    pub(crate) fn current(yes: bool, no_prompt: bool) -> Mode {
        use std::io::IsTerminal;
        let tty = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
        Mode::from_flags(yes, no_prompt, tty)
    }
}

/// 利用者が「やめる」を選んだ、または入力が閉じた。`juiz run` は何も出さずに終了コード 1 で終わる
#[derive(Debug)]
pub(crate) struct Quit;

impl std::fmt::Display for Quit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "stopped")
    }
}

impl std::error::Error for Quit {}

/// 事前確認の結果
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// そのまま起動する
    Start,
    /// 起動せずに終わる(動いている方の管理画面を開いたとき)
    Exit,
}

/// 確かめるブローカーの種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Transport {
    Http,
    Qmp,
}

impl Transport {
    fn type_name(self) -> &'static str {
        match self {
            Transport::Http => "http",
            Transport::Qmp => "qmp",
        }
    }

    /// 選択肢の文の `port`(qmp は `qmp port`、§15.2)
    fn port_word(self) -> &'static str {
        match self {
            Transport::Http => "port",
            Transport::Qmp => "qmp port",
        }
    }
}

/// 利用者とのやりとり(§15.1)。テストでは差し替える
pub(crate) trait Prompt {
    /// 標準エラーに 1 行出す
    fn say(&mut self, line: &str);
    /// prompt を出して 1 行読む。入力が閉じたら None
    fn ask(&mut self, prompt: &str) -> Option<String>;
}

/// 事前確認が外の世界に触る所。テストでは差し替える
pub(crate) trait Env: Prompt {
    /// その番号の待ち受けを試しに取り、使用中なら true(使用中以外の失敗は false にして、ブローカーの起動に任せる)
    fn port_in_use(&mut self, transport: Transport, host: &str, port: i64) -> bool;
    /// 相手の `system/profile`(juiz でなければ None、§16.3)
    fn profile(&mut self, transport: Transport, host: &str, port: i64) -> Option<Value>;
    /// その IPC 名で待ち受けている相手がいるか
    fn ipc_in_use(&mut self, name: &str) -> bool;
    /// URL をブラウザで開く
    fn open_browser(&mut self, url: &str);
}

/// 事前確認に要る、パッケージと起動のしかた
#[derive(Debug, Clone)]
pub(crate) struct Context {
    pub name: String,
    pub kind: Kind,
    /// 基礎の `base.http_port`(§10.1)
    pub base_http_port: Option<u16>,
    /// conf の `option.http_broker` の HTTP ブローカーを立てるか(§7.1)
    pub http_starts: bool,
    pub mode: Mode,
}

/// conf の中の、確かめるブローカーの場所
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    /// `option.http_broker`
    HttpOption,
    /// `brokers[i]`
    Brokers(usize),
}

#[derive(Debug, Clone)]
struct Target {
    place: Place,
    transport: Transport,
    host: String,
    port: i64,
    policy: PortPolicy,
}

/// `option.http_broker` の既定の番号(system_builder の get_http_port と同じ)
const DEFAULT_HTTP_OPTION_PORT: i64 = 8000;
/// `brokers` の http・qmp の既定の番号(各ブローカーの on_start と同じ)
const DEFAULT_BROKERS_PORT: i64 = 8080;

/// conf から確かめるブローカーを集める。知らない port_fallback などは無視する(ブローカーを立てるときにエラーになる)
fn targets(conf: &Value, http_starts: bool) -> Vec<Target> {
    let policy = |v: &Value| PortPolicy::from_manifest(v).unwrap_or_default();
    let mut out = Vec::new();
    if http_starts {
        let http = conf
            .get("option")
            .and_then(|o| o.get("http_broker"))
            .cloned()
            .unwrap_or_else(|| jvalue!({}));
        out.push(Target {
            place: Place::HttpOption,
            transport: Transport::Http,
            // option.http_broker のブローカーは常に 0.0.0.0 で待ち受ける(setup_http_broker)
            host: "0.0.0.0".to_owned(),
            port: http
                .get("port")
                .and_then(Value::as_i64)
                .unwrap_or(DEFAULT_HTTP_OPTION_PORT),
            policy: policy(&http),
        });
    }
    if let Some(brokers) = conf.get("brokers").and_then(Value::as_array) {
        for (i, b) in brokers.iter().enumerate() {
            let transport = match b.get("type_name").and_then(Value::as_str) {
                Some("http") => Transport::Http,
                Some("qmp") => Transport::Qmp,
                _ => continue,
            };
            out.push(Target {
                place: Place::Brokers(i),
                transport,
                host: b
                    .get("host")
                    .and_then(Value::as_str)
                    .unwrap_or("0.0.0.0")
                    .to_owned(),
                port: b
                    .get("port")
                    .and_then(Value::as_i64)
                    .unwrap_or(DEFAULT_BROKERS_PORT),
                policy: policy(b),
            });
        }
    }
    out
}

/// conf の target の場所に番号を書く
fn set_port(conf: &mut Value, place: Place, port: i64) {
    let Some(obj) = conf.as_object_mut() else {
        return;
    };
    let entry = match place {
        Place::HttpOption => {
            let option = obj.entry("option").or_insert_with(|| jvalue!({}));
            if !option.is_object() {
                *option = jvalue!({});
            }
            let http = option
                .as_object_mut()
                .unwrap()
                .entry("http_broker")
                .or_insert_with(|| jvalue!({}));
            if !http.is_object() {
                *http = jvalue!({});
            }
            http
        }
        Place::Brokers(i) => match obj.get_mut("brokers").and_then(|b| b.get_mut(i)) {
            Some(b) => b,
            None => return,
        },
    };
    if let Some(e) = entry.as_object_mut() {
        e.insert("port".to_owned(), port.into());
    }
}

/// 使用中の番号の相手
#[derive(Debug, Clone, PartialEq, Eq)]
enum Occupant {
    /// 同じパッケージの juiz
    Same,
    /// 別の juiz(パッケージから起動していれば `名前 版`)
    Juiz(Option<String>),
    /// juiz 以外
    Other,
}

impl Occupant {
    fn classify(profile: Option<&Value>, name: &str) -> Occupant {
        let Some(profile) = profile else {
            return Occupant::Other;
        };
        let package = profile.get("package").filter(|p| p.is_object());
        let pname = package.and_then(|p| p.get("name")).and_then(Value::as_str);
        if pname == Some(name) {
            return Occupant::Same;
        }
        let label = pname.map(|n| {
            match package
                .and_then(|p| p.get("version"))
                .and_then(Value::as_str)
            {
                Some(v) => format!("{n} {v}"),
                None => n.to_owned(),
            }
        });
        Occupant::Juiz(label)
    }

    fn is_juiz(&self) -> bool {
        !matches!(self, Occupant::Other)
    }

    /// `in use by …` の … の部分
    fn by(&self, name: &str) -> String {
        match self {
            Occupant::Same => format!("another {name}"),
            Occupant::Juiz(Some(l)) => format!("another juiz ({l})"),
            Occupant::Juiz(None) => "another juiz".to_owned(),
            Occupant::Other => "another program".to_owned(),
        }
    }
}

/// 選択肢
#[derive(Debug, Clone, PartialEq, Eq)]
enum Choice {
    /// 空いている番号で起動する(true なら相手を親にする)
    StartOn(i64, bool),
    /// 番号を入力する
    EnterPort,
    /// 動いている方の管理画面を開いて終わる
    OpenUi(String),
    Quit,
}

/// 番号つきの選択を出して 1 つ選ばせる(§15.1)。空行は既定。範囲外・数でない入力は同じ選択をもう一度出す。
/// 入力が閉じたら None
pub(crate) fn choose(env: &mut impl Prompt, question: &str, options: &[String], default: usize) -> Option<usize> {
    loop {
        env.say(question);
        for (i, o) in options.iter().enumerate() {
            env.say(&format!("  {}) {o}", i + 1));
        }
        let line = env.ask(&format!("choose [{}]: ", default + 1))?;
        let line = line.trim();
        if line.is_empty() {
            return Some(default);
        }
        match line.parse::<usize>() {
            Ok(n) if (1..=options.len()).contains(&n) => return Some(n - 1),
            _ => env.say(&format!("'{line}' is not one of 1..{}.", options.len())),
        }
    }
}

/// 番号を入力させる。数でない・範囲外なら聞き直す。入力が閉じたら None
fn ask_port(env: &mut impl Env) -> Option<i64> {
    loop {
        let line = env.ask("port number: ")?;
        let line = line.trim();
        match line.parse::<u16>() {
            Ok(n) if n > 0 => return Some(n as i64),
            _ => env.say(&format!("'{line}' is not a port number (1..65535).")),
        }
    }
}

/// `port` の次から元の番号 + 100 まで、最初に空いている番号(§15.2)
fn find_free(env: &mut impl Env, t: &Target, origin: i64, port: i64) -> Option<i64> {
    let last = (origin + PORT_FALLBACK_RANGE).min(65535);
    (port + 1..=last).find(|p| !env.port_in_use(t.transport, &t.host, *p))
}

/// ポートと IPC 名の事前確認(§15.2)。conf の番号・`mastersystem` を書き換えることがある
pub(crate) fn precheck(conf: &mut Value, ctx: &Context, env: &mut impl Env) -> Result<Outcome> {
    // 基礎の IPC 名が使用中なら、相手を問わず二重起動しない
    if ctx.kind == Kind::Base && env.ipc_in_use(&ctx.name) {
        bail!("{} is already running at ipc://{}.", ctx.name, ctx.name);
    }
    for t in targets(conf, ctx.http_starts) {
        if check_target(conf, ctx, &t, env)? == Outcome::Exit {
            return Ok(Outcome::Exit);
        }
    }
    Ok(Outcome::Start)
}

fn check_target(
    conf: &mut Value,
    ctx: &Context,
    t: &Target,
    env: &mut impl Env,
) -> Result<Outcome> {
    if !(1..=65535).contains(&t.port) {
        return Ok(Outcome::Start);
    }
    let base = ctx.kind == Kind::Base;
    let mut port = t.port;
    // 空いている番号を探す起点。番号を入力したらそこからにする
    let mut origin = t.port;
    loop {
        if !env.port_in_use(t.transport, &t.host, port) {
            return Ok(use_port(conf, ctx, t, port, env));
        }
        let profile = env.profile(t.transport, probe_host(&t.host), port);
        let occupant = Occupant::classify(profile.as_ref(), &ctx.name);
        let url = format!("http://127.0.0.1:{port}/");
        if base && occupant == Occupant::Same {
            bail!("{} is already running at {url}", ctx.name);
        }
        if ctx.mode == Mode::NoPrompt {
            if base {
                let how = match (t.transport, t.place) {
                    (Transport::Http, Place::HttpOption) => "start it with --http-port <n>",
                    _ => "change the port in the conf",
                };
                bail!(
                    "{} {port} for {} is in use by {}. Free the port, or {how}.",
                    t.transport.port_word(),
                    ctx.name,
                    occupant.by(&ctx.name)
                );
            }
            // アプリは §16 の規則(取り直し・親とみなす・port_fallback: error)でブローカーが扱う
            return Ok(Outcome::Start);
        }

        // 選択肢を作る
        let free = if base || t.policy.fallback == PortFallback::Next {
            find_free(env, t, origin, port)
        } else {
            // port_fallback: error なら空いている番号へは移さない(番号の入力はできる)
            None
        };
        // 相手を親にするか(アプリで、相手が juiz で adopt_master が有効、conf に mastersystem が無いとき)
        let join = !base
            && occupant.is_juiz()
            && t.policy.adopt_master
            && conf.get("mastersystem").map(Value::is_null).unwrap_or(true);
        let mut choices = Vec::new();
        let mut labels = Vec::new();
        let question;
        if occupant == Occupant::Same {
            question = format!("{} is already running at {url}", ctx.name);
            if let Some(n) = free {
                choices.push(Choice::StartOn(n, join));
                labels.push(format!(
                    "start another one on {} {n}{}",
                    t.transport.port_word(),
                    if join {
                        " (join it as its subsystem)"
                    } else {
                        ""
                    }
                ));
            }
            if t.transport == Transport::Http {
                let ui = format!("{url}ui/");
                choices.push(Choice::OpenUi(ui));
                labels.push("open the web UI of the running one in a browser, and quit".to_owned());
            }
        } else {
            question = format!(
                "{} {port} is in use by {}.",
                t.transport.port_word(),
                occupant.by(&ctx.name)
            );
            if let Some(n) = free {
                choices.push(Choice::StartOn(n, join));
                labels.push(format!(
                    "start on {} {n} (free){}",
                    t.transport.port_word(),
                    if join {
                        " (join it as its subsystem)"
                    } else {
                        ""
                    }
                ));
            }
            if !base {
                choices.push(Choice::EnterPort);
                labels.push(format!("enter a {} number", t.transport.port_word()));
            }
        }
        choices.push(Choice::Quit);
        labels.push("quit".to_owned());

        let picked = match ctx.mode {
            Mode::Interactive => match choose(env, &question, &labels, 0) {
                Some(i) => choices[i].clone(),
                None => return Err(Quit.into()),
            },
            // --yes は既定(一番上)を選ぶ。入力が要るもの・やめるしか無いときは、対話できないときと同じにする
            _ => match &choices[0] {
                Choice::StartOn(..) => {
                    env.say(&question);
                    choices[0].clone()
                }
                _ => {
                    if base {
                        bail!(
                            "no free {} for {} in {}..{}",
                            t.transport.port_word(),
                            ctx.name,
                            t.port,
                            t.port + PORT_FALLBACK_RANGE
                        );
                    }
                    return Ok(Outcome::Start);
                }
            },
        };
        match picked {
            Choice::StartOn(n, join) => {
                if join {
                    conf.as_object_mut().map(|o| {
                        o.insert(
                            "mastersystem".to_owned(),
                            master_info(t.transport.type_name(), port),
                        )
                    });
                    env.say(&format!(
                        "joining the juiz at 127.0.0.1:{port} as its subsystem"
                    ));
                }
                return Ok(use_port(conf, ctx, t, n, env));
            }
            Choice::EnterPort => match ask_port(env) {
                Some(n) => {
                    port = n;
                    origin = n;
                }
                None => return Err(Quit.into()),
            },
            Choice::OpenUi(ui) => {
                env.say(&format!("opening {ui}"));
                env.open_browser(&ui);
                return Ok(Outcome::Exit);
            }
            Choice::Quit => return Err(Quit.into()),
        }
    }
}

/// 確かめて取れた番号を conf に入れる。基礎の HTTP の番号が base.http_port と違えば添え書きを出す(§15.2)
fn use_port(conf: &mut Value, ctx: &Context, t: &Target, port: i64, env: &mut impl Env) -> Outcome {
    if port != t.port {
        set_port(conf, t.place, port);
    }
    if ctx.kind == Kind::Base && t.place == Place::HttpOption {
        if let Some(b) = ctx.base_http_port {
            if port != b as i64 {
                // 名前の一覧に登録するなら、この機械のアプリは登録のポートで見つける(naming.md §8)
                if juiz_core::naming::naming_options(conf).register {
                    env.say(&format!(
                        "note: apps on this machine find {} on port {port} in the name list, but apps on other machines look for port {b}.",
                        ctx.name
                    ));
                } else {
                    env.say(&format!(
                        "note: apps that connect to {} over http look for port {b} and will not find it.",
                        ctx.name
                    ));
                }
            }
        }
    }
    Outcome::Start
}

/// 本物の端末・ネットワークを使う [`Env`]
pub(crate) struct RealEnv;

impl Prompt for RealEnv {
    fn say(&mut self, line: &str) {
        eprintln!("{line}");
    }

    fn ask(&mut self, prompt: &str) -> Option<String> {
        eprint!("{prompt}");
        let _ = std::io::stderr().flush();
        let mut line = String::new();
        match std::io::stdin().lock().read_line(&mut line) {
            Ok(0) | Err(_) => {
                eprintln!();
                None
            }
            Ok(_) => Some(line),
        }
    }
}

impl Env for RealEnv {
    fn port_in_use(&mut self, transport: Transport, host: &str, port: i64) -> bool {
        let Ok(port) = u16::try_from(port) else {
            return false;
        };
        let result = match transport {
            Transport::Http => std::net::TcpListener::bind((host, port)).map(drop),
            Transport::Qmp => std::net::UdpSocket::bind((host, port)).map(drop),
        };
        matches!(result, Err(e) if e.kind() == std::io::ErrorKind::AddrInUse)
    }

    fn profile(&mut self, transport: Transport, host: &str, port: i64) -> Option<Value> {
        match transport {
            Transport::Http => {
                let host = host.to_owned();
                // 呼ぶ側が tokio の中かどうかによらないよう、別スレッドで小さな runtime を回す
                std::thread::spawn(move || {
                    let rt = juiz_core::tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .ok()?;
                    rt.block_on(juiz_core::port_policy::fetch_http_profile(&host, port))
                })
                .join()
                .ok()
                .flatten()
            }
            #[cfg(feature = "qmp-broker")]
            Transport::Qmp => qmp_broker::fetch_profile_blocking(host, port),
            #[cfg(not(feature = "qmp-broker"))]
            Transport::Qmp => None,
        }
    }

    #[allow(unused_variables)]
    fn ipc_in_use(&mut self, name: &str) -> bool {
        #[cfg(feature = "ipc-broker")]
        return juiz_core::ipc::ipc_name_in_use(name);
        #[cfg(not(feature = "ipc-broker"))]
        return false;
    }

    fn open_browser(&mut self, url: &str) {
        let result = if cfg!(windows) {
            // start の最初の引数は窓の題名なので空にする
            std::process::Command::new("cmd")
                .args(["/C", "start", "", url])
                .status()
        } else if cfg!(target_os = "macos") {
            std::process::Command::new("open").arg(url).status()
        } else {
            std::process::Command::new("xdg-open").arg(url).status()
        };
        if !result.is_ok_and(|s| s.success()) {
            eprintln!("cannot open a browser. Open {url} yourself.");
        }
    }
}

/// 起動できたら、立てたブローカーのつなぎ先を標準エラーに 1 行ずつ出す(§15.3)
pub(crate) fn announce_brokers(profile: &Value) {
    for line in broker_lines(profile) {
        eprintln!("{line}");
    }
}

fn broker_lines(profile: &Value) -> Vec<String> {
    // system/profile は core_store.brokers にブローカーの名前を載せる
    let Some(brokers) = profile
        .get("core_store")
        .and_then(|c| c.get("brokers"))
        .and_then(Value::as_array)
    else {
        return vec![];
    };
    brokers
        .iter()
        .filter_map(|b| {
            let type_name = b.get("type_name").and_then(Value::as_str)?;
            let name = b.get("name").and_then(Value::as_str)?;
            Some(match type_name {
                "http" => {
                    let name = name.replace("0.0.0.0:", "127.0.0.1:");
                    let ui = if webui_served(b) {
                        " (web UI: /ui)"
                    } else {
                        ""
                    };
                    format!("HTTP broker: http://{name}/{ui}")
                }
                "ipc" => format!("IPC broker: ipc://{name}"),
                "local" | "core" => return None,
                t => format!("{t} broker: {t}://{name}"),
            })
        })
        .collect()
}

/// その HTTP ブローカーが管理画面を出しているか(feature webui で、`webui: false` でないとき)
fn webui_served(b: &Value) -> bool {
    cfg!(feature = "webui") && b.get("webui").and_then(Value::as_bool) != Some(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet, VecDeque};

    /// 決めた番号だけ使用中にし、決めた答えを返す偽の Env
    #[derive(Default)]
    struct Fake {
        used: BTreeSet<(&'static str, i64)>,
        profiles: BTreeMap<i64, Value>,
        ipc: BTreeSet<String>,
        answers: VecDeque<&'static str>,
        said: Vec<String>,
        opened: Vec<String>,
    }

    impl Fake {
        fn used(mut self, t: &'static str, ports: &[i64]) -> Self {
            for p in ports {
                self.used.insert((t, *p));
            }
            self
        }
        fn juiz(mut self, port: i64, package: Option<(&str, &str)>) -> Self {
            let mut p = jvalue!({"system_store": {"uuid": "u"}});
            if let Some((n, v)) = package {
                p["package"] = jvalue!({"name": n, "version": v, "kind": "app"});
            }
            self.profiles.insert(port, p);
            self
        }
        fn answers(mut self, a: &[&'static str]) -> Self {
            self.answers = a.iter().copied().collect();
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

    impl Env for Fake {
        fn port_in_use(&mut self, t: Transport, _host: &str, port: i64) -> bool {
            self.used.contains(&(t.type_name(), port))
        }
        fn profile(&mut self, _t: Transport, host: &str, port: i64) -> Option<Value> {
            assert_eq!(host, "127.0.0.1");
            self.profiles.get(&port).cloned()
        }
        fn ipc_in_use(&mut self, name: &str) -> bool {
            self.ipc.contains(name)
        }
        fn open_browser(&mut self, url: &str) {
            self.opened.push(url.to_owned());
        }
    }

    fn ctx(kind: Kind, mode: Mode) -> Context {
        Context {
            name: "my_app".into(),
            kind,
            base_http_port: (kind == Kind::Base).then_some(8100),
            http_starts: true,
            mode,
        }
    }

    fn http_conf(port: i64) -> Value {
        jvalue!({"option": {"http_broker": {"port": port}}})
    }

    #[test]
    fn mode_from_flags() {
        assert_eq!(Mode::from_flags(false, false, true), Mode::Interactive);
        assert_eq!(Mode::from_flags(false, false, false), Mode::NoPrompt);
        assert_eq!(Mode::from_flags(true, false, true), Mode::Yes);
        assert_eq!(Mode::from_flags(true, false, false), Mode::Yes);
        // 両方なら --no-prompt
        assert_eq!(Mode::from_flags(true, true, true), Mode::NoPrompt);
        assert_eq!(Mode::from_flags(false, true, true), Mode::NoPrompt);
    }

    #[test]
    fn free_port_starts_without_asking() {
        let mut conf = http_conf(8080);
        let mut env = Fake::default();
        let r = precheck(&mut conf, &ctx(Kind::App, Mode::Interactive), &mut env).unwrap();
        assert_eq!(r, Outcome::Start);
        assert_eq!(conf, http_conf(8080));
        assert!(env.said.is_empty());
    }

    #[test]
    fn default_port_is_8000_when_conf_has_none() {
        let mut conf = jvalue!({});
        let mut env = Fake::default().used("http", &[8000]).answers(&[""]);
        precheck(&mut conf, &ctx(Kind::App, Mode::Interactive), &mut env).unwrap();
        assert_eq!(conf["option"]["http_broker"]["port"], 8001);
        // HTTP ブローカーを立てないなら見ない
        let mut conf = jvalue!({});
        let mut c = ctx(Kind::App, Mode::Interactive);
        c.http_starts = false;
        let mut env = Fake::default().used("http", &[8000]);
        precheck(&mut conf, &c, &mut env).unwrap();
        assert_eq!(conf, jvalue!({}));
    }

    #[test]
    fn app_other_program_choices() {
        // 1) 空いている番号(既定)
        let mut conf = http_conf(8080);
        let mut env = Fake::default().used("http", &[8080, 8081]).answers(&[""]);
        precheck(&mut conf, &ctx(Kind::App, Mode::Interactive), &mut env).unwrap();
        assert_eq!(conf["option"]["http_broker"]["port"], 8082);
        assert_eq!(
            env.text(),
            "port 8080 is in use by another program.\n  1) start on port 8082 (free)\n  2) enter a port number\n  3) quit\nchoose [1]: "
        );
        assert!(conf.get("mastersystem").is_none());

        // 2) 番号を入力。数でない・範囲外は聞き直し、入れた番号も使用中なら同じ選択を出す
        let mut conf = http_conf(8080);
        let mut env = Fake::default()
            .used("http", &[8080, 9000])
            .answers(&["x", "2", "abc", "70000", "9000", "2", "9100"]);
        precheck(&mut conf, &ctx(Kind::App, Mode::Interactive), &mut env).unwrap();
        assert_eq!(conf["option"]["http_broker"]["port"], 9100);
        let t = env.text();
        assert!(t.contains("'x' is not one of 1..3."), "{t}");
        assert!(
            t.contains("'70000' is not a port number (1..65535)."),
            "{t}"
        );
        assert!(
            t.contains("port 9000 is in use by another program.\n  1) start on port 9001 (free)"),
            "{t}"
        );

        // 3) やめる、入力が閉じたときもやめる
        for answers in [&["3"][..], &[][..]] {
            let mut conf = http_conf(8080);
            let mut env = Fake::default().used("http", &[8080]).answers(answers);
            let e = precheck(&mut conf, &ctx(Kind::App, Mode::Interactive), &mut env).unwrap_err();
            assert!(e.downcast_ref::<Quit>().is_some());
        }
    }

    #[test]
    fn app_other_juiz_joins_when_adopt_master() {
        let mut conf = http_conf(8080);
        let mut env = Fake::default()
            .used("http", &[8080])
            .juiz(8080, Some(("talker_listener", "0.1.0")))
            .answers(&["1"]);
        precheck(&mut conf, &ctx(Kind::App, Mode::Interactive), &mut env).unwrap();
        assert_eq!(conf["option"]["http_broker"]["port"], 8081);
        assert_eq!(conf["mastersystem"], master_info("http", 8080));
        let t = env.text();
        assert!(t.starts_with("port 8080 is in use by another juiz (talker_listener 0.1.0).\n  1) start on port 8081 (free) (join it as its subsystem)\n"), "{t}");
        assert!(
            t.contains("joining the juiz at 127.0.0.1:8080 as its subsystem"),
            "{t}"
        );

        // adopt_master: false、conf に mastersystem があるときは親にしない
        for conf0 in [
            jvalue!({"option": {"http_broker": {"port": 8080, "adopt_master": false}}}),
            jvalue!({"option": {"http_broker": {"port": 8080}}, "mastersystem": {"type_name": "http", "name": "x:1"}}),
        ] {
            let mut conf = conf0.clone();
            let mut env = Fake::default()
                .used("http", &[8080])
                .juiz(8080, None)
                .answers(&[""]);
            precheck(&mut conf, &ctx(Kind::App, Mode::Interactive), &mut env).unwrap();
            assert!(!env.text().contains("join"), "{}", env.text());
            assert_eq!(conf.get("mastersystem"), conf0.get("mastersystem"));
        }
    }

    #[test]
    fn app_same_package_choices() {
        // 2) 管理画面を開いて終わる
        let mut conf = http_conf(8080);
        let mut env = Fake::default()
            .used("http", &[8080])
            .juiz(8080, Some(("my_app", "0.1.0")))
            .answers(&["2"]);
        let r = precheck(&mut conf, &ctx(Kind::App, Mode::Interactive), &mut env).unwrap();
        assert_eq!(r, Outcome::Exit);
        assert_eq!(env.opened, vec!["http://127.0.0.1:8080/ui/".to_owned()]);
        assert!(env.text().starts_with(
            "my_app is already running at http://127.0.0.1:8080/\n  1) start another one on port 8081 (join it as its subsystem)\n  2) open the web UI of the running one in a browser, and quit\n  3) quit\n"
        ), "{}", env.text());

        // 1) もう 1 つ起動
        let mut conf = http_conf(8080);
        let mut env = Fake::default()
            .used("http", &[8080])
            .juiz(8080, Some(("my_app", "0.1.0")))
            .answers(&[""]);
        let r = precheck(&mut conf, &ctx(Kind::App, Mode::Interactive), &mut env).unwrap();
        assert_eq!(r, Outcome::Start);
        assert_eq!(conf["option"]["http_broker"]["port"], 8081);
    }

    #[test]
    fn app_without_prompt_leaves_it_to_the_broker() {
        let mut conf = http_conf(8080);
        let mut env = Fake::default().used("http", &[8080]).juiz(8080, None);
        let r = precheck(&mut conf, &ctx(Kind::App, Mode::NoPrompt), &mut env).unwrap();
        assert_eq!(r, Outcome::Start);
        assert_eq!(conf, http_conf(8080));
        assert!(env.said.is_empty());
    }

    #[test]
    fn app_yes_takes_the_default() {
        let mut conf = http_conf(8080);
        let mut env = Fake::default().used("http", &[8080]).juiz(8080, None);
        precheck(&mut conf, &ctx(Kind::App, Mode::Yes), &mut env).unwrap();
        assert_eq!(conf["option"]["http_broker"]["port"], 8081);
        assert_eq!(conf["mastersystem"], master_info("http", 8080));
        // 同じパッケージでも、既定はもう 1 つ起動
        let mut conf = http_conf(8080);
        let mut env = Fake::default()
            .used("http", &[8080])
            .juiz(8080, Some(("my_app", "0.1.0")));
        let r = precheck(&mut conf, &ctx(Kind::App, Mode::Yes), &mut env).unwrap();
        assert_eq!(r, Outcome::Start);
        assert_eq!(conf["option"]["http_broker"]["port"], 8081);
        assert!(env.opened.is_empty());
    }

    #[test]
    fn port_fallback_error_offers_only_entering_a_port() {
        let conf0 = jvalue!({"option": {"http_broker": {"port": 8080, "port_fallback": "error"}}});
        let mut conf = conf0.clone();
        let mut env = Fake::default().used("http", &[8080]).answers(&["", "9000"]);
        precheck(&mut conf, &ctx(Kind::App, Mode::Interactive), &mut env).unwrap();
        assert!(
            env.text().starts_with(
                "port 8080 is in use by another program.\n  1) enter a port number\n  2) quit\n"
            ),
            "{}",
            env.text()
        );
        assert_eq!(conf["option"]["http_broker"]["port"], 9000);
        // --yes でも入力が要るので、ブローカーに任せる(port_fallback: error のエラーになる)
        let mut conf = conf0.clone();
        let mut env = Fake::default().used("http", &[8080]);
        precheck(&mut conf, &ctx(Kind::App, Mode::Yes), &mut env).unwrap();
        assert_eq!(conf, conf0);
    }

    #[test]
    fn base_checks() {
        // IPC 名が使用中なら止める
        let mut conf = http_conf(8100);
        let mut env = Fake::default();
        env.ipc.insert("my_app".into());
        let e = precheck(&mut conf, &ctx(Kind::Base, Mode::Interactive), &mut env).unwrap_err();
        assert_eq!(e.to_string(), "my_app is already running at ipc://my_app.");

        // 同じパッケージがポートにいれば止める
        let mut env = Fake::default()
            .used("http", &[8100])
            .juiz(8100, Some(("my_app", "1.0.0")));
        let e = precheck(&mut conf, &ctx(Kind::Base, Mode::Yes), &mut env).unwrap_err();
        assert_eq!(
            e.to_string(),
            "my_app is already running at http://127.0.0.1:8100/"
        );

        // 対話できないならエラー
        let mut env = Fake::default()
            .used("http", &[8100])
            .juiz(8100, Some(("other", "2.0.0")));
        let e = precheck(&mut conf, &ctx(Kind::Base, Mode::NoPrompt), &mut env).unwrap_err();
        assert_eq!(
            e.to_string(),
            "port 8100 for my_app is in use by another juiz (other 2.0.0). Free the port, or start it with --http-port <n>."
        );

        // 対話: 1) 空いている番号 2) やめる。親にはしない。番号を変えたら添え書き
        let mut c = conf.clone();
        let mut env = Fake::default()
            .used("http", &[8100])
            .juiz(8100, None)
            .answers(&["1"]);
        precheck(&mut c, &ctx(Kind::Base, Mode::Interactive), &mut env).unwrap();
        assert_eq!(c["option"]["http_broker"]["port"], 8101);
        assert!(c.get("mastersystem").is_none());
        let t = env.text();
        assert!(
            t.starts_with(
                "port 8100 is in use by another juiz.\n  1) start on port 8101 (free)\n  2) quit\n"
            ),
            "{t}"
        );
        assert!(t.ends_with("note: apps on this machine find my_app on port 8101 in the name list, but apps on other machines look for port 8100."), "{t}");

        // 登録しないときは、前の添え書き
        let mut c = conf.clone();
        juiz_core::naming::set_register_option(&mut c, false);
        let mut env = Fake::default()
            .used("http", &[8100])
            .juiz(8100, None)
            .answers(&["1"]);
        precheck(&mut c, &ctx(Kind::Base, Mode::Interactive), &mut env).unwrap();
        let t = env.text();
        assert!(t.ends_with("note: apps that connect to my_app over http look for port 8100 and will not find it."), "{t}");

        // --yes は空いている番号
        let mut c = conf.clone();
        let mut env = Fake::default().used("http", &[8100]);
        precheck(&mut c, &ctx(Kind::Base, Mode::Yes), &mut env).unwrap();
        assert_eq!(c["option"]["http_broker"]["port"], 8101);

        // --http-port で base.http_port と違う番号にしたときも添え書き
        let mut c = http_conf(8200);
        let mut env = Fake::default();
        precheck(&mut c, &ctx(Kind::Base, Mode::NoPrompt), &mut env).unwrap();
        assert!(
            env.text()
                .starts_with("note: apps on this machine find my_app on port 8200"),
            "{}",
            env.text()
        );
    }

    #[test]
    fn qmp_port_is_checked_too() {
        let mut conf = jvalue!({"brokers": [{"type_name": "ipc", "name": "x"}, {"type_name": "qmp", "port": 3000}]});
        let mut c = ctx(Kind::App, Mode::Interactive);
        c.http_starts = false;
        let mut env = Fake::default()
            .used("qmp", &[3000])
            .juiz(3000, Some(("my_app", "0.1.0")))
            .answers(&[""]);
        precheck(&mut conf, &c, &mut env).unwrap();
        assert_eq!(conf["brokers"][1]["port"], 3001);
        // qmp の同じパッケージには管理画面の選択肢が無い
        let t = env.text();
        assert!(
            t.contains(
                "1) start another one on qmp port 3001 (join it as its subsystem)\n  2) quit"
            ),
            "{t}"
        );
        assert_eq!(conf["mastersystem"], master_info("qmp", 3000));

        // 基礎の qmp の番号の案内は conf
        let mut conf = jvalue!({"brokers": [{"type_name": "qmp", "port": 3000}]});
        let mut c = ctx(Kind::Base, Mode::NoPrompt);
        c.http_starts = false;
        let mut env = Fake::default().used("qmp", &[3000]);
        let e = precheck(&mut conf, &c, &mut env).unwrap_err();
        assert_eq!(e.to_string(), "qmp port 3000 for my_app is in use by another program. Free the port, or change the port in the conf.");
    }

    #[test]
    fn only_first_juiz_is_adopted() {
        // HTTP と qmp の両方で juiz に当たっても、親にするのは最初の 1 つ
        let mut conf = jvalue!({"option": {"http_broker": {"port": 8080}}, "brokers": [{"type_name": "qmp", "port": 3000}]});
        let mut env = Fake::default()
            .used("http", &[8080])
            .used("qmp", &[3000])
            .juiz(8080, None)
            .juiz(3000, None)
            .answers(&["", ""]);
        precheck(&mut conf, &ctx(Kind::App, Mode::Interactive), &mut env).unwrap();
        assert_eq!(conf["mastersystem"], master_info("http", 8080));
        assert_eq!(conf["brokers"][0]["port"], 3001);
    }

    #[test]
    fn no_free_port() {
        let ports: Vec<i64> = (8100..=8200).collect();
        let mut conf = http_conf(8100);
        let mut env = Fake::default().used("http", &ports);
        let e = precheck(&mut conf, &ctx(Kind::Base, Mode::Yes), &mut env).unwrap_err();
        assert_eq!(e.to_string(), "no free port for my_app in 8100..8200");
        // 対話ではやめるだけ
        let mut env = Fake::default().used("http", &ports).answers(&[""]);
        let e = precheck(&mut conf, &ctx(Kind::Base, Mode::Interactive), &mut env).unwrap_err();
        assert!(e.downcast_ref::<Quit>().is_some());
        assert!(env.text().contains("  1) quit\n"), "{}", env.text());
    }

    #[test]
    fn broker_lines_from_profile() {
        let p = jvalue!({"core_store": {"brokers": [
            {"type_name": "local", "name": "local"},
            {"type_name": "http", "name": "127.0.0.1:8081"},
            {"type_name": "ipc", "name": "robot_base"},
            {"type_name": "qmp", "name": "0.0.0.0:3000"},
        ]}});
        let ui = if cfg!(feature = "webui") {
            " (web UI: /ui)"
        } else {
            ""
        };
        assert_eq!(
            broker_lines(&p),
            vec![
                format!("HTTP broker: http://127.0.0.1:8081/{ui}"),
                "IPC broker: ipc://robot_base".to_owned(),
                "qmp broker: qmp://0.0.0.0:3000".to_owned(),
            ]
        );
    }
}
