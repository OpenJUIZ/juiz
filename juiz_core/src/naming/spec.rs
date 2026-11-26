//! 名前で指す書き方 `name://…` と、CLI の `-s` の名前 (docs/spec/naming.md §9.1・§9.2、issue #618)

use juiz_sdk::anyhow::anyhow;

use crate::prelude::*;

use super::probe::ProfileProbe;
use super::record::{namespace_segments, RegisteredBroker};
use super::registry::{Candidate, NameQuery};

/// 名前で指すときのブローカーの種類名 (`name://…`)
pub const NAME_SCHEME: &str = "name";

/// 名前で指したつなぎ先 (`name://mobile_sim?ns=robot1&type=http` を読んだもの)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameSpec {
    pub query: NameQuery,
    /// `type=<種類>` (§7.3 の種類の指定)
    pub kind: Option<String>,
}

impl NameSpec {
    /// `name://` の後ろ (ブローカーの名前の部分) を読む: `<名前>[@<機械>][?ns=<ネームスペース>&type=<種類>]`
    pub fn parse_broker_name(s: &str) -> JuizResult<NameSpec> {
        let (body, query) = match s.split_once('?') {
            Some((b, q)) => (b, Some(q)),
            None => (s, None),
        };
        if body.contains('/') {
            return Err(anyhow!(
                "invalid name 'name://{s}'. Write the namespace as a query (ex., name://mobile_sim?ns=robot1)"
            ));
        }
        let mut q = NameQuery::parse(body)?;
        let mut kind = None;
        for pair in query.unwrap_or("").split('&').filter(|p| !p.is_empty()) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            match k {
                "ns" => {
                    let ns = v.trim_matches('/');
                    namespace_segments(ns)
                        .map_err(|e| anyhow!("invalid namespace in 'name://{s}': {e}"))?;
                    q.namespace = Some(ns.to_owned());
                }
                "type" if !v.is_empty() => kind = Some(v.to_owned()),
                _ => {
                    return Err(anyhow!(
                        "unknown query '{pair}' in 'name://{s}'. Only ns=<namespace> and type=<broker type> can be written"
                    ))
                }
            }
        }
        Ok(NameSpec { query: q, kind })
    }

    /// `name://…` の URL を読む。`name://` で始まらなければ `None`
    pub fn parse_url(s: &str) -> JuizResult<Option<NameSpec>> {
        match s.strip_prefix("name://") {
            Some(rest) => Self::parse_broker_name(rest.trim_end_matches('/')).map(Some),
            None => Ok(None),
        }
    }

    /// CLI の `-s`・pyjuiz のつなぎ先 (§9.2)。`name://…` か、`://` も `:` も無いものを名前として読む。
    /// それ以外 (今までどおりの URL) は `None`
    pub fn parse_server(s: &str) -> JuizResult<Option<NameSpec>> {
        if s.starts_with("name://") {
            return Self::parse_url(s);
        }
        if s.contains("://") || s.contains(':') {
            return Ok(None);
        }
        Ok(Some(NameSpec {
            query: NameQuery::parse(s)?,
            kind: None,
        }))
    }

    /// この機械の登録 (見つからなければ LAN と peers、§7.1) から 1 つに決め (§7.2、対話しない)、
    /// つなぐブローカーを選ぶ (§7.3)
    pub fn resolve(
        &self,
        probe: &dyn ProfileProbe,
        kinds: &[String],
        peers: &[super::peers::Peer],
    ) -> JuizResult<Resolved> {
        let candidate = super::resolve_query(&self.query, probe, kinds, peers)?;
        Resolved::new(candidate, self.kind.as_deref(), kinds)
    }
}

impl std::fmt::Display for NameSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.query)?;
        if let Some(k) = &self.kind {
            write!(f, " ({k})")?;
        }
        Ok(())
    }
}

/// 名前を解決した結果
#[derive(Debug, Clone)]
pub struct Resolved {
    pub candidate: Candidate,
    pub broker: RegisteredBroker,
    /// つなぐ URL (`ipc://mobile_sim`・`http://127.0.0.1:8001`)
    pub url: String,
}

impl Resolved {
    /// 決めた候補から、つなぐブローカーを選ぶ (§7.3)
    pub fn new(candidate: Candidate, kind: Option<&str>, kinds: &[String]) -> JuizResult<Resolved> {
        let (broker, url) = candidate.select_broker(kind, kinds)?;
        Ok(Resolved {
            candidate,
            broker,
            url,
        })
    }

    /// つなぐブローカーの種類と名前 (`("http", "127.0.0.1:8001")`)
    pub fn type_and_name(&self) -> (String, String) {
        split_broker_url(&self.url)
    }
}

/// `<種類>://<名前>` を種類と名前に分ける
pub fn split_broker_url(url: &str) -> (String, String) {
    match url.split_once("://") {
        Some((t, n)) => (t.to_owned(), n.trim_end_matches('/').to_owned()),
        None => (String::new(), url.to_owned()),
    }
}
