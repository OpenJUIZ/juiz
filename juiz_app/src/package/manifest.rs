//! マニフェスト `juizapp.yaml` の項目の検査と書き出し(docs/spec/app-package.md §4)

use super::names::check_package_name;
use super::{warn, yaml, FORMAT_VERSION, JUIZ_VERSION};
use juiz_core::anyhow::{anyhow, bail, Result};
use juiz_sdk::serde_json::{Map, Value};
use semver::{Op, Version, VersionReq};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    App,
    Base,
}

impl Kind {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Kind::App => "app",
            Kind::Base => "base",
        }
    }
}

/// 読んだマニフェスト。`raw` は書き出すときに知らない項目を残すために持つ
#[derive(Debug, Clone)]
pub(crate) struct Manifest {
    pub name: String,
    pub version: Version,
    pub kind: Kind,
    pub juiz: Option<String>,
    pub entry: String,
    pub depends: BTreeMap<String, String>,
    pub python_requires: Vec<String>,
    pub python_version: Option<String>,
    pub base_http_port: Option<u16>,
    pub exclude: Vec<String>,
    pub languages: Option<Vec<String>>,
    pub targets: Option<Vec<String>>,
    pub rust_juiz_sdk: Option<String>,
    pub rust_rustc: Option<String>,
    pub files: Option<BTreeMap<String, String>>,
    raw: Map<String, Value>,
}

/// 作る人が書く項目と pack が書く項目(§4.1)。知らない項目の警告と、書き出す順に使う
const KNOWN_KEYS: &[&str] = &[
    "format",
    "name",
    "version",
    "kind",
    "description",
    "license",
    "authors",
    "homepage",
    "juiz",
    "entry",
    "depends",
    "python",
    "base",
    "exclude",
    "languages",
    "targets",
    "rust",
    "files",
];

/// pack が書く項目(書き出すときは最後に置く)
const PACK_KEYS: &[&str] = &["languages", "targets", "rust", "files"];

/// 言語の名前(§4.1 languages)
pub(crate) const LANGUAGES: &[&str] = &["rust", "python", "c++", "haskell"];

fn opt_str<'a>(m: &'a Map<String, Value>, key: &str) -> Result<Option<&'a str>> {
    match m.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(v) => bail!("'{key}' in juizapp.yaml must be a string, but {v}"),
    }
}

fn opt_str_list(m: &Map<String, Value>, key: &str) -> Result<Option<Vec<String>>> {
    match m.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| {
                v.as_str().map(str::to_owned).ok_or_else(|| {
                    anyhow!("'{key}' in juizapp.yaml must be a list of strings, but has {v}")
                })
            })
            .collect::<Result<Vec<_>>>()
            .map(Some),
        Some(v) => bail!("'{key}' in juizapp.yaml must be a list of strings, but {v}"),
    }
}

fn opt_map<'a>(m: &'a Map<String, Value>, key: &str) -> Result<Option<&'a Map<String, Value>>> {
    match m.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(o)) => Ok(Some(o)),
        Some(v) => bail!("'{key}' in juizapp.yaml must be a map, but {v}"),
    }
}

fn str_map(m: &Map<String, Value>, key: &str) -> Result<BTreeMap<String, String>> {
    m.iter()
        .map(|(k, v)| match v {
            Value::String(s) => Ok((k.clone(), s.clone())),
            _ => bail!("'{key}.{k}' in juizapp.yaml must be a string, but {v}"),
        })
        .collect()
}

/// 版の範囲(Cargo の書き方)を読む
pub(crate) fn parse_req(what: &str, s: &str) -> Result<VersionReq> {
    VersionReq::parse(s).map_err(|e| anyhow!("{what} '{s}' is not a version range: {e}"))
}

/// `=x.y.z` の形ならその版を返す
pub(crate) fn exact_version(req: &VersionReq) -> Option<Version> {
    match req.comparators.as_slice() {
        [c] if c.op == Op::Exact => Some(Version {
            major: c.major,
            minor: c.minor?,
            patch: c.patch?,
            pre: c.pre.clone(),
            build: Default::default(),
        }),
        _ => None,
    }
}

/// マニフェストの `format` を確かめる(§9 の段 2)。`file` は文の中で使う名前
pub(crate) fn check_format(v: &Value, file: &str) -> Result<()> {
    let format = v
        .get("format")
        .ok_or_else(|| anyhow!("'format' is missing in juizapp.yaml of {file}"))?;
    match format.as_i64() {
        Some(FORMAT_VERSION) => Ok(()),
        Some(n) if n > FORMAT_VERSION => bail!(
            "{file} uses package format {n}, but this juiz ({JUIZ_VERSION}) supports up to {FORMAT_VERSION}. Update juiz."
        ),
        _ => bail!("'format' in juizapp.yaml of {file} must be {FORMAT_VERSION}, but {format}"),
    }
}

impl Manifest {
    pub(crate) fn parse_str(text: &str, file: &str) -> Result<Manifest> {
        let v = yaml::parse(text).map_err(|e| anyhow!("juizapp.yaml of {file}: {e}"))?;
        Manifest::from_value(v, file)
    }

    /// 型と値を検査して読む(§4.1、§6.3 の 1、8)
    pub(crate) fn from_value(v: Value, file: &str) -> Result<Manifest> {
        let Value::Object(m) = v else {
            bail!("juizapp.yaml of {file} must be a map");
        };
        check_format(&Value::Object(m.clone()), file)?;
        for k in m.keys() {
            if !KNOWN_KEYS.contains(&k.as_str()) {
                warn(&format!(
                    "unknown field '{k}' in juizapp.yaml of {file} is ignored"
                ));
            }
        }
        let name = opt_str(&m, "name")?
            .ok_or_else(|| anyhow!("'name' is missing in juizapp.yaml of {file}"))?
            .to_owned();
        check_package_name(&name).map_err(|e| anyhow!(e))?;
        let version_str = opt_str(&m, "version")?
            .ok_or_else(|| anyhow!("'version' is missing in juizapp.yaml of {file}"))?;
        let version = Version::parse(version_str).map_err(|e| {
            anyhow!("'version' '{version_str}' in juizapp.yaml is not a semantic version: {e}")
        })?;
        let kind = match opt_str(&m, "kind")? {
            None | Some("app") => Kind::App,
            Some("base") => Kind::Base,
            Some(k) => bail!("'kind' in juizapp.yaml must be 'app' or 'base', but '{k}'"),
        };
        for key in ["description", "license", "homepage"] {
            opt_str(&m, key)?;
        }
        opt_str_list(&m, "authors")?;
        let juiz = opt_str(&m, "juiz")?.map(str::to_owned);
        if let Some(j) = &juiz {
            parse_req("'juiz'", j)?;
        }
        let entry = opt_str(&m, "entry")?.unwrap_or("app.conf").to_owned();
        let depends = match opt_map(&m, "depends")? {
            Some(d) => str_map(d, "depends")?,
            None => BTreeMap::new(),
        };
        for (dep, range) in &depends {
            check_package_name(dep).map_err(|e| anyhow!("depends: {e}"))?;
            parse_req(&format!("the range of depends.{dep}"), range)?;
            if dep == &name {
                bail!("'{name}' must not depend on itself");
            }
        }
        let (python_requires, python_version) = match opt_map(&m, "python")? {
            Some(p) => (
                opt_str_list(p, "requires")
                    .map_err(|e| anyhow!("python.{e}"))?
                    .unwrap_or_default(),
                opt_str(p, "version")?.map(str::to_owned),
            ),
            None => (Vec::new(), None),
        };
        let base_http_port = match opt_map(&m, "base")? {
            Some(b) => match b.get("http_port") {
                None | Some(Value::Null) => None,
                Some(p) => Some(
                    p.as_u64()
                        .and_then(|p| u16::try_from(p).ok())
                        .filter(|p| *p > 0)
                        .ok_or_else(|| {
                            anyhow!(
                                "'base.http_port' in juizapp.yaml must be a port number, but {p}"
                            )
                        })?,
                ),
            },
            None => None,
        };
        if kind == Kind::App && m.get("base").is_some_and(|b| !b.is_null()) {
            bail!("'base' in juizapp.yaml is only for 'kind: base'");
        }
        let exclude = opt_str_list(&m, "exclude")?.unwrap_or_default();
        let languages = opt_str_list(&m, "languages")?;
        if let Some(langs) = &languages {
            for l in langs {
                if !LANGUAGES.contains(&l.as_str()) {
                    bail!("unknown language '{l}' in 'languages' of juizapp.yaml (one of {LANGUAGES:?})");
                }
            }
        }
        let targets = opt_str_list(&m, "targets")?;
        let (rust_juiz_sdk, rust_rustc) = match opt_map(&m, "rust")? {
            Some(r) => (
                opt_str(r, "juiz_sdk")?.map(str::to_owned),
                opt_str(r, "rustc")?.map(str::to_owned),
            ),
            None => (None, None),
        };
        let files = match opt_map(&m, "files")? {
            Some(f) => {
                let f = str_map(f, "files")?;
                for (path, hash) in &f {
                    let ok = hash.strip_prefix("sha256:").is_some_and(|h| {
                        h.len() == 64 && h.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
                    });
                    if !ok {
                        bail!("files.{path} in juizapp.yaml must be 'sha256:<64 lowercase hex digits>', but '{hash}'");
                    }
                }
                Some(f)
            }
            None => None,
        };
        Ok(Manifest {
            name,
            version,
            kind,
            juiz,
            entry,
            depends,
            python_requires,
            python_version,
            base_http_port,
            exclude,
            languages,
            targets,
            rust_juiz_sdk,
            rust_rustc,
            files,
            raw: m,
        })
    }

    /// パッケージの中のマニフェストに必須の項目(§4.1 の「○(パッケージ内)」)を確かめる
    pub(crate) fn check_packed(&self, file: &str) -> Result<()> {
        for (key, missing) in [
            ("juiz", self.juiz.is_none()),
            ("languages", self.languages.is_none()),
            ("targets", self.targets.is_none()),
            ("files", self.files.is_none()),
        ] {
            if missing {
                bail!("'{key}' is missing in juizapp.yaml of {file}. Make the package with 'juiz pack'.");
            }
        }
        if self.has_language("rust") && (self.rust_juiz_sdk.is_none() || self.rust_rustc.is_none())
        {
            bail!("'rust.juiz_sdk' and 'rust.rustc' are missing in juizapp.yaml of {file}. Make the package with 'juiz pack'.");
        }
        Ok(())
    }

    pub(crate) fn has_language(&self, lang: &str) -> bool {
        self.languages
            .as_ref()
            .is_some_and(|l| l.iter().any(|x| x == lang))
    }

    /// pack が書き足した結果を YAML で書く。作る人の項目は書いてあった値のまま(既定値は書き足す)
    pub(crate) fn to_yaml(&self) -> String {
        let mut items: Vec<(String, Value)> = Vec::new();
        let author_keys = KNOWN_KEYS.iter().filter(|k| !PACK_KEYS.contains(k));
        for key in author_keys {
            let v = match *key {
                "format" => Some(Value::from(FORMAT_VERSION)),
                "kind" => Some(Value::from(self.kind.as_str())),
                "juiz" => self.juiz.clone().map(Value::from),
                "entry" => Some(Value::from(self.entry.clone())),
                k => self.raw.get(k).filter(|v| !v.is_null()).cloned(),
            };
            if let Some(v) = v {
                items.push((key.to_string(), v));
            }
        }
        for (k, v) in &self.raw {
            if !KNOWN_KEYS.contains(&k.as_str()) {
                items.push((k.clone(), v.clone()));
            }
        }
        if let Some(l) = &self.languages {
            items.push(("languages".into(), Value::from(l.clone())));
        }
        if let Some(t) = &self.targets {
            items.push(("targets".into(), Value::from(t.clone())));
        }
        if let (Some(sdk), Some(rustc)) = (&self.rust_juiz_sdk, &self.rust_rustc) {
            let mut r = Map::new();
            r.insert("juiz_sdk".into(), Value::from(sdk.clone()));
            r.insert("rustc".into(), Value::from(rustc.clone()));
            items.push(("rust".into(), Value::Object(r)));
        }
        if let Some(f) = &self.files {
            let m: Map<String, Value> = f
                .iter()
                .map(|(k, v)| (k.clone(), Value::from(v.clone())))
                .collect();
            items.push(("files".into(), Value::Object(m)));
        }
        yaml::emit_top(&items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
format: 1
name: talker_listener
version: 0.1.0
description: "talker と listener"
juiz: "=0.0.28"
depends:
  robot_base: ">=1.2, <2"
python:
  requires: ["numpy>=1.26"]
  version: ">=3.10"
exclude: ["*.log"]
"#;

    #[test]
    fn reads_author_fields() {
        let m = Manifest::parse_str(SAMPLE, "t").unwrap();
        assert_eq!(m.name, "talker_listener");
        assert_eq!(m.version.to_string(), "0.1.0");
        assert_eq!(m.kind, Kind::App);
        assert_eq!(m.entry, "app.conf");
        assert_eq!(m.depends["robot_base"], ">=1.2, <2");
        assert_eq!(m.python_requires, vec!["numpy>=1.26"]);
        assert_eq!(m.python_version.as_deref(), Some(">=3.10"));
        assert_eq!(m.exclude, vec!["*.log"]);
        assert!(m.files.is_none());
        assert!(m.check_packed("t").is_err());
    }

    #[test]
    fn to_yaml_round_trips() {
        let mut m = Manifest::parse_str(SAMPLE, "t").unwrap();
        m.languages = Some(vec!["rust".into()]);
        m.targets = Some(vec!["x86_64-unknown-linux-gnu".into()]);
        m.rust_juiz_sdk = Some("0.0.28".into());
        m.rust_rustc = Some("rustc 1.91.0 (f8297e351 2025-10-28)".into());
        m.files = Some(BTreeMap::from([(
            "app.conf".to_owned(),
            format!("sha256:{}", "0".repeat(64)),
        )]));
        let text = m.to_yaml();
        assert!(
            text.starts_with("format: 1\nname: talker_listener\n"),
            "{text}"
        );
        let back = Manifest::parse_str(&text, "t").unwrap();
        back.check_packed("t").unwrap();
        assert_eq!(back.rust_rustc, m.rust_rustc);
        assert_eq!(back.files, m.files);
        assert_eq!(back.depends, m.depends);
        assert_eq!(back.kind, Kind::App);
        assert_eq!(back.juiz.as_deref(), Some("=0.0.28"));
    }

    #[test]
    fn rejects_invalid_fields() {
        let cases = [
            ("format: 2\nname: a\nversion: 0.1.0", "supports up to 1"),
            ("format: 0\nname: a\nversion: 0.1.0", "must be 1"),
            ("name: a\nversion: 0.1.0", "'format' is missing"),
            ("format: 1\nname: A\nversion: 0.1.0", "lowercase"),
            ("format: 1\nname: a\nversion: '1.2'", "semantic version"),
            (
                "format: 1\nname: a\nversion: 0.1.0\nkind: lib",
                "'app' or 'base'",
            ),
            (
                "format: 1\nname: a\nversion: 0.1.0\njuiz: abc",
                "version range",
            ),
            (
                "format: 1\nname: a\nversion: 0.1.0\ndepends: {a: '1'}",
                "itself",
            ),
            (
                "format: 1\nname: a\nversion: 0.1.0\ndepends: {b: '>>1'}",
                "version range",
            ),
            (
                "format: 1\nname: a\nversion: 0.1.0\nbase: {http_port: 8100}",
                "only for 'kind: base'",
            ),
            (
                "format: 1\nname: a\nversion: 0.1.0\nkind: base\nbase: {http_port: 70000}",
                "port number",
            ),
            (
                "format: 1\nname: a\nversion: 0.1.0\nlanguages: [go]",
                "unknown language",
            ),
            (
                "format: 1\nname: a\nversion: 0.1.0\nfiles: {a: 'md5:00'}",
                "sha256",
            ),
            (
                "format: 1\nname: a\nversion: 0.1.0\nexclude: x",
                "list of strings",
            ),
            ("format: 1\nname: 1\nversion: 0.1.0", "must be a string"),
        ];
        for (text, expect) in cases {
            let e = Manifest::parse_str(text, "t").unwrap_err().to_string();
            assert!(e.contains(expect), "{text}: {e}");
        }
    }

    #[test]
    fn exact_versions() {
        let r = VersionReq::parse("=0.0.28").unwrap();
        assert_eq!(exact_version(&r).unwrap().to_string(), "0.0.28");
        assert!(exact_version(&VersionReq::parse(">=0.0.28").unwrap()).is_none());
        assert!(exact_version(&VersionReq::parse("=0.0").unwrap()).is_none());
        assert!(exact_version(&VersionReq::parse("=1.0.0, <2").unwrap()).is_none());
    }
}
