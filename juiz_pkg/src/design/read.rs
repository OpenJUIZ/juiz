//! 文書を読む: YAML → [`Design`] (docs/spec/design.md §3.2、§5.1 の F1〜F3)

use clap::ValueEnum;
use serde_json::Value;

use super::yaml::{self, Kind, Node, Pos};
use super::{
    ArgumentDesign, ComponentDesign, ContainerDesign, Design, Diag, ProcessDesign, Report,
    StateDesign, FORMAT,
};
use crate::Lang;

struct Reader {
    errors: Vec<Diag>,
    positions: Vec<(String, Pos)>,
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

/// 辞書の項目 (鍵の位置つき)
struct Entries<'a> {
    path: String,
    pos: Pos,
    entries: Vec<(&'a str, Pos, &'a Node)>,
}

impl<'a> Entries<'a> {
    fn get(&self, key: &str) -> Option<(Pos, &'a Node)> {
        self.entries
            .iter()
            .find(|(k, _, _)| *k == key)
            .map(|(_, p, n)| (*p, *n))
    }
}

impl Reader {
    fn error(&mut self, path: &str, pos: Pos, message: impl Into<String>) {
        self.errors.push(Diag {
            path: path.to_owned(),
            pos: Some(pos),
            message: message.into(),
        });
    }

    /// 辞書として読む。表に無いキー (`x-` で始まるものを除く) はエラー (F3)
    fn map<'a>(&mut self, node: &'a Node, path: &str, allowed: &[&str]) -> Option<Entries<'a>> {
        let Kind::Map(entries) = &node.kind else {
            self.error(path, node.pos, format!("must be a mapping, not {}", node.type_label()));
            return None;
        };
        let mut out = Vec::new();
        for (k, kpos, v) in entries {
            if k.starts_with("x-") {
                continue;
            }
            if !allowed.contains(&k.as_str()) {
                self.error(
                    &join(path, k),
                    *kpos,
                    format!("unknown key '{k}' (allowed: {}; keys starting with x- are ignored)", allowed.join(", ")),
                );
                continue;
            }
            self.positions.push((join(path, k), *kpos));
            out.push((k.as_str(), *kpos, v));
        }
        Some(Entries {
            path: path.to_owned(),
            pos: node.pos,
            entries: out,
        })
    }

    fn string(&mut self, e: &Entries, key: &str, required: bool) -> Option<String> {
        let path = join(&e.path, key);
        match e.get(key) {
            None => {
                if required {
                    self.error(&path, e.pos, format!("'{key}' is required"));
                }
                None
            }
            Some((pos, n)) => match &n.kind {
                Kind::Scalar(Value::String(s)) => Some(s.clone()),
                // 説明に null を書いたときは省いたのと同じにする
                Kind::Scalar(Value::Null) if !required => None,
                _ => {
                    self.error(&path, pos, format!("must be a string, not {}", n.type_label()));
                    None
                }
            },
        }
    }

    /// 型の式 (空はエラー)
    fn type_expr(&mut self, e: &Entries, key: &str, required: bool) -> Option<String> {
        let s = self.string(e, key, required)?;
        if s.trim().is_empty() {
            let (pos, _) = e.get(key).unwrap();
            self.error(&join(&e.path, key), pos, "must not be empty");
            return None;
        }
        Some(s)
    }

    /// 配列。省いた、null は空
    fn seq<'a>(&mut self, e: &Entries<'a>, key: &str) -> Vec<(String, &'a Node)> {
        let path = join(&e.path, key);
        match e.get(key) {
            None => Vec::new(),
            Some((pos, n)) => match &n.kind {
                Kind::Scalar(Value::Null) => Vec::new(),
                Kind::Seq(items) => items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| {
                        let p = format!("{path}[{i}]");
                        self.positions.push((p.clone(), item.pos));
                        (p, item)
                    })
                    .collect(),
                _ => {
                    self.error(&path, pos, format!("must be a sequence, not {}", n.type_label()));
                    Vec::new()
                }
            },
        }
    }

    fn component(&mut self, node: &Node, path: &str) -> Option<ComponentDesign> {
        let e = self.map(node, path, &["name", "description", "language"])?;
        let name = self.string(&e, "name", true);
        let description = self.string(&e, "description", false);
        let language = match self.string(&e, "language", false) {
            None => None,
            Some(l) => match Lang::from_str(&l, true) {
                Ok(lang) => Some(lang),
                Err(_) => {
                    let (pos, _) = e.get("language").unwrap();
                    self.error(
                        &join(path, "language"),
                        pos,
                        format!("unknown language '{l}' (rust, python, cpp, haskell or zig)"),
                    );
                    None
                }
            },
        };
        Some(ComponentDesign {
            name: name?,
            description,
            language,
        })
    }

    fn argument(&mut self, node: &Node, path: &str) -> Option<ArgumentDesign> {
        let e = self.map(node, path, &["name", "type", "default", "description"])?;
        let name = self.string(&e, "name", true);
        let type_expr = self.type_expr(&e, "type", true);
        let description = self.string(&e, "description", false);
        let default = e.get("default").map(|(_, n)| n.to_json());
        Some(ArgumentDesign {
            name: name?,
            type_expr: type_expr?,
            default,
            description,
        })
    }

    fn arguments(&mut self, e: &Entries, key: &str) -> Vec<ArgumentDesign> {
        let items = self.seq(e, key);
        items
            .into_iter()
            .filter_map(|(p, n)| self.argument(n, &p))
            .collect()
    }

    fn state(&mut self, node: &Node, path: &str) -> Option<StateDesign> {
        let e = self.map(node, path, &["name", "type", "init", "default", "description"])?;
        let name = self.string(&e, "name", true);
        let type_expr = self.type_expr(&e, "type", true);
        let init = self.string(&e, "init", false);
        let description = self.string(&e, "description", false);
        let default = e.get("default").map(|(_, n)| n.to_json());
        Some(StateDesign {
            name: name?,
            type_expr: type_expr?,
            init,
            default,
            description,
        })
    }

    fn process(&mut self, node: &Node, path: &str, container_process: bool) -> Option<ProcessDesign> {
        let e = self.map(node, path, &["name", "description", "arguments", "returns", "readonly"])?;
        let name = self.string(&e, "name", true);
        let description = self.string(&e, "description", false);
        let arguments = self.arguments(&e, "arguments");
        let returns = self.type_expr(&e, "returns", false);
        let readonly = match e.get("readonly") {
            None => false,
            Some((pos, _)) if !container_process => {
                self.error(
                    &join(path, "readonly"),
                    pos,
                    "readonly can be written only for a container process (containers[].processes[])",
                );
                false
            }
            Some((pos, n)) => match &n.kind {
                Kind::Scalar(Value::Bool(b)) => *b,
                Kind::Scalar(Value::Null) => false,
                _ => {
                    self.error(
                        &join(path, "readonly"),
                        pos,
                        format!("must be true or false, not {}", n.type_label()),
                    );
                    false
                }
            },
        };
        Some(ProcessDesign {
            name: name?,
            description,
            arguments,
            returns,
            readonly,
        })
    }

    fn container(&mut self, node: &Node, path: &str) -> Option<ContainerDesign> {
        let e = self.map(node, path, &["name", "description", "arguments", "state", "processes"])?;
        let name = self.string(&e, "name", true);
        let description = self.string(&e, "description", false);
        let arguments = self.arguments(&e, "arguments");
        let state = self
            .seq(&e, "state")
            .into_iter()
            .filter_map(|(p, n)| self.state(n, &p))
            .collect();
        let processes = self
            .seq(&e, "processes")
            .into_iter()
            .filter_map(|(p, n)| self.process(n, &p, true))
            .collect();
        Some(ContainerDesign {
            name: name?,
            description,
            arguments,
            state,
            processes,
        })
    }
}

fn stop(path: &str, pos: Option<Pos>, message: impl Into<String>) -> Report {
    Report {
        errors: vec![Diag {
            path: path.to_owned(),
            pos,
            message: message.into(),
        }],
        warnings: Vec::new(),
    }
}

/// 文書を読む。§5.1 の F1〜F3 に反すればエラー (F1・F2 はそこで止める、F3 はすべて並べる)
pub fn parse(text: &str) -> Result<Design, Report> {
    // F1
    let root = match yaml::parse(text) {
        Ok(Some(n)) => n,
        Ok(None) => return Err(stop("", None, "the document is empty")),
        Err(e) => return Err(stop("", e.pos, format!("not a YAML document: {}", e.message))),
    };
    let Kind::Map(entries) = &root.kind else {
        return Err(stop(
            "",
            Some(root.pos),
            format!("the top level must be a mapping, not {}", root.type_label()),
        ));
    };
    // F2
    match entries.iter().find(|(k, _, _)| k == "format") {
        None => return Err(stop("format", Some(root.pos), format!("'format' is required (format: {FORMAT})"))),
        Some((_, kpos, v)) => match &v.kind {
            Kind::Scalar(Value::Number(n)) if n.as_i64() == Some(FORMAT) => {}
            Kind::Scalar(Value::Number(n)) if n.as_i64().is_some_and(|f| f > FORMAT) => {
                return Err(stop(
                    "format",
                    Some(*kpos),
                    format!("format {n} is newer than this juiz reads (format {FORMAT}); the document was made by a newer juiz, so use a newer juiz"),
                ))
            }
            _ => {
                return Err(stop(
                    "format",
                    Some(*kpos),
                    format!("format must be {FORMAT}, not {}", serde_json::to_string(&v.to_json()).unwrap_or_default()),
                ))
            }
        },
    }
    // F3
    let mut r = Reader {
        errors: Vec::new(),
        positions: Vec::new(),
    };
    let e = r
        .map(&root, "", &["format", "component", "idl", "processes", "containers"])
        .expect("the root is a mapping");
    let component = match e.get("component") {
        None => {
            r.error("component", root.pos, "'component' is required");
            None
        }
        Some((_, n)) => r.component(n, "component"),
    };
    let mut idl = Vec::new();
    for (p, n) in r.seq(&e, "idl") {
        match &n.kind {
            Kind::Scalar(Value::String(s)) => idl.push(s.clone()),
            _ => r.error(&p, n.pos, format!("must be a string (a path of a .jidl), not {}", n.type_label())),
        }
    }
    let processes: Vec<ProcessDesign> = r
        .seq(&e, "processes")
        .into_iter()
        .filter_map(|(p, n)| r.process(n, &p, false))
        .collect();
    let containers: Vec<ContainerDesign> = r
        .seq(&e, "containers")
        .into_iter()
        .filter_map(|(p, n)| r.container(n, &p))
        .collect();
    if !r.errors.is_empty() {
        return Err(Report {
            errors: r.errors,
            warnings: Vec::new(),
        });
    }
    Ok(Design {
        component: component.expect("no error means the component was read"),
        idl,
        processes,
        containers,
        positions: r.positions,
    })
}
