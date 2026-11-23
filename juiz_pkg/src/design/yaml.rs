//! 位置つきの YAML の木 (docs/spec/design.md §4.2・§5)
//!
//! yaml-rust2 の Yaml には位置が無いので、イベントから行・桁つきの木を作る。
//! スカラーの読み方は yaml-rust2 の YamlLoader と同じ (juiz の conf の読み方と同じ)。
//! JSON にできない値 (文字列でない辞書の鍵、.nan・.inf、タグ) はエラーにする。

use std::collections::HashMap;

use serde_json::{Map, Number, Value};
use yaml_rust2::parser::{Event, MarkedEventReceiver, Parser, Tag};
use yaml_rust2::scanner::{Marker, TScalarStyle};
use yaml_rust2::Yaml;

/// 文書の中の位置 (行・桁とも 1 から数える)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pos {
    pub line: usize,
    pub column: usize,
}

impl From<Marker> for Pos {
    fn from(m: Marker) -> Self {
        Pos {
            line: m.line(),
            column: m.col() + 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// 鍵の並びは書いた順。鍵の位置も持つ
    Map(Vec<(String, Pos, Node)>),
    Seq(Vec<Node>),
    Scalar(Value),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub kind: Kind,
    pub pos: Pos,
}

impl Node {
    /// JSON の値にする (既定値・初期値に使う)
    pub fn to_json(&self) -> Value {
        match &self.kind {
            Kind::Scalar(v) => v.clone(),
            Kind::Seq(items) => Value::Array(items.iter().map(Node::to_json).collect()),
            Kind::Map(entries) => {
                let mut m = Map::new();
                for (k, _, v) in entries {
                    m.insert(k.clone(), v.to_json());
                }
                Value::Object(m)
            }
        }
    }

    pub fn type_label(&self) -> &'static str {
        match &self.kind {
            Kind::Map(_) => "a mapping",
            Kind::Seq(_) => "a sequence",
            Kind::Scalar(Value::Null) => "null",
            Kind::Scalar(Value::Bool(_)) => "a boolean",
            Kind::Scalar(Value::Number(_)) => "a number",
            Kind::Scalar(Value::String(_)) => "a string",
            Kind::Scalar(_) => "a value",
        }
    }
}

/// 読めなかったときのエラー (位置と文)
#[derive(Clone, Debug, PartialEq)]
pub struct YamlError {
    pub pos: Option<Pos>,
    pub message: String,
}

enum Building {
    Map {
        entries: Vec<(String, Pos, Node)>,
        key: Option<(String, Pos)>,
        anchor: usize,
        pos: Pos,
    },
    Seq {
        items: Vec<Node>,
        anchor: usize,
        pos: Pos,
    },
}

#[derive(Default)]
struct Builder {
    stack: Vec<Building>,
    docs: Vec<Option<Node>>,
    anchors: HashMap<usize, Node>,
    error: Option<YamlError>,
}

fn tag_error(tag: &Tag, pos: Pos) -> YamlError {
    YamlError {
        pos: Some(pos),
        message: format!(
            "the tag {}{} cannot be used (the document is read as JSON values)",
            if tag.handle == "tag:yaml.org,2002:" { "!!" } else { &tag.handle },
            tag.suffix
        ),
    }
}

/// yaml-rust2 の YamlLoader と同じ読み方で、プレーンのスカラーを値にする
fn plain_scalar(v: &str, pos: Pos) -> Result<Value, YamlError> {
    Ok(match Yaml::from_str(v) {
        Yaml::Null => Value::Null,
        Yaml::Boolean(b) => Value::Bool(b),
        Yaml::Integer(i) => Value::Number(i.into()),
        Yaml::Real(r) => {
            let f = Yaml::Real(r.clone()).as_f64().unwrap_or(f64::NAN);
            match Number::from_f64(f) {
                Some(n) => Value::Number(n),
                None => {
                    return Err(YamlError {
                        pos: Some(pos),
                        message: format!("'{r}' cannot be used (NaN and infinity are not JSON values)"),
                    })
                }
            }
        }
        Yaml::String(s) => Value::String(s),
        _ => Value::String(v.to_owned()),
    })
}

impl Builder {
    fn fail(&mut self, e: YamlError) {
        if self.error.is_none() {
            self.error = Some(e);
        }
    }

    fn insert(&mut self, node: Node, anchor: usize) {
        if anchor > 0 {
            self.anchors.insert(anchor, node.clone());
        }
        match self.stack.last_mut() {
            None => self.docs.push(Some(node)),
            Some(Building::Seq { items, .. }) => items.push(node),
            Some(Building::Map { entries, key, .. }) => match key.take() {
                Some((k, kpos)) => {
                    if entries.iter().any(|(e, _, _)| *e == k) {
                        let e = YamlError {
                            pos: Some(kpos),
                            message: format!("the key '{k}' is written twice"),
                        };
                        self.fail(e);
                        return;
                    }
                    entries.push((k, kpos, node));
                }
                None => {
                    let k = match &node.kind {
                        Kind::Scalar(Value::String(s)) => s.clone(),
                        _ => {
                            let e = YamlError {
                                pos: Some(node.pos),
                                message: format!(
                                    "a key of a mapping must be a string, not {}",
                                    node.type_label()
                                ),
                            };
                            self.fail(e);
                            return;
                        }
                    };
                    *key = Some((k, node.pos));
                }
            },
        }
    }

    fn event(&mut self, ev: Event, mark: Marker) {
        if self.error.is_some() {
            return;
        }
        let pos = Pos::from(mark);
        match ev {
            Event::Nothing | Event::StreamStart | Event::StreamEnd | Event::DocumentStart => {}
            Event::DocumentEnd => {
                if self.docs.is_empty() && self.stack.is_empty() {
                    // 空の文書
                    self.docs.push(None);
                }
            }
            Event::Alias(id) => match self.anchors.get(&id).cloned() {
                Some(mut n) => {
                    n.pos = pos;
                    self.insert(n, 0);
                }
                None => self.fail(YamlError {
                    pos: Some(pos),
                    message: "unknown alias".to_owned(),
                }),
            },
            Event::Scalar(v, style, anchor, tag) => {
                if let Some(t) = &tag {
                    // !!str だけは文字列として受ける
                    if !(t.handle == "tag:yaml.org,2002:" && t.suffix == "str") {
                        self.fail(tag_error(t, pos));
                        return;
                    }
                }
                let value = if style != TScalarStyle::Plain || tag.is_some() {
                    Value::String(v)
                } else {
                    match plain_scalar(&v, pos) {
                        Ok(v) => v,
                        Err(e) => {
                            self.fail(e);
                            return;
                        }
                    }
                };
                self.insert(
                    Node {
                        kind: Kind::Scalar(value),
                        pos,
                    },
                    anchor,
                );
            }
            Event::SequenceStart(anchor, tag) => {
                if let Some(t) = &tag {
                    self.fail(tag_error(t, pos));
                    return;
                }
                self.stack.push(Building::Seq {
                    items: Vec::new(),
                    anchor,
                    pos,
                });
            }
            Event::MappingStart(anchor, tag) => {
                if let Some(t) = &tag {
                    self.fail(tag_error(t, pos));
                    return;
                }
                self.stack.push(Building::Map {
                    entries: Vec::new(),
                    key: None,
                    anchor,
                    pos,
                });
            }
            Event::SequenceEnd => {
                if let Some(Building::Seq { items, anchor, pos }) = self.stack.pop() {
                    self.insert(
                        Node {
                            kind: Kind::Seq(items),
                            pos,
                        },
                        anchor,
                    );
                }
            }
            Event::MappingEnd => {
                if let Some(Building::Map {
                    entries, anchor, pos, ..
                }) = self.stack.pop()
                {
                    self.insert(
                        Node {
                            kind: Kind::Map(entries),
                            pos,
                        },
                        anchor,
                    );
                }
            }
        }
    }
}

impl MarkedEventReceiver for Builder {
    fn on_event(&mut self, ev: Event, mark: Marker) {
        self.event(ev, mark);
    }
}

/// YAML の文字列を読む。文書は 1 つでなければならない。空の文書は None
pub fn parse(text: &str) -> Result<Option<Node>, YamlError> {
    let mut builder = Builder::default();
    let mut parser = Parser::new_from_str(text);
    parser.load(&mut builder, true).map_err(|e| YamlError {
        pos: Some(Pos::from(*e.marker())),
        message: e.info().to_owned(),
    })?;
    if let Some(e) = builder.error {
        return Err(e);
    }
    match builder.docs.len() {
        0 => Ok(None),
        1 => Ok(builder.docs.pop().unwrap()),
        _ => Err(YamlError {
            pos: None,
            message: "a design document must be one YAML document (remove '---' between documents)".to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn values_and_positions() {
        let n = parse("a: 1\nb:\n  - { x: \"s\", y: 0x10 }\n  - 1.5\nc: ~\n").unwrap().unwrap();
        assert_eq!(
            n.to_json(),
            json!({"a": 1, "b": [{"x": "s", "y": 16}, 1.5], "c": null})
        );
        let Kind::Map(entries) = &n.kind else { panic!() };
        assert_eq!(entries[1].1, Pos { line: 2, column: 1 });
        let Kind::Seq(items) = &entries[1].2.kind else { panic!() };
        assert_eq!(items[1].pos, Pos { line: 4, column: 5 });
    }

    #[test]
    fn errors() {
        let e = parse("a: .nan\n").unwrap_err();
        assert_eq!(e.pos, Some(Pos { line: 1, column: 4 }));
        assert!(parse("a: !!binary aGVsbG8=\n").unwrap_err().message.contains("!!binary"));
        assert!(parse("1: a\n").unwrap_err().message.contains("key"));
        assert!(parse("a: 1\na: 2\n").unwrap_err().message.contains("twice"));
        assert!(parse("a: [\n").is_err());
        assert!(parse("a: 1\n---\nb: 2\n").is_err());
        assert_eq!(parse("").unwrap(), None);
        assert_eq!(parse("# only a comment\n").unwrap(), None);
        // !!str は文字列
        assert_eq!(parse("a: !!str 1\n").unwrap().unwrap().to_json(), json!({"a": "1"}));
        // 別名はその値になる
        assert_eq!(
            parse("a: &x [1, 2]\nb: *x\n").unwrap().unwrap().to_json(),
            json!({"a": [1, 2], "b": [1, 2]})
        );
    }
}
