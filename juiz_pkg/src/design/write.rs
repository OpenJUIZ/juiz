//! 文書を書く: [`Design`] → YAML (docs/spec/design.md §9)
//!
//! juiz が書く文書は 1 つの形にそろえる。juiz が書いた文書を読んで書き直すと同じ文字列になる。
//! `x-` のキーと YAML のコメントは残らない (利用者の文書を書き直すことは無い)。

use serde_json::Value;
use yaml_rust2::Yaml;

use super::{ArgumentDesign, Design, ProcessDesign, StateDesign, FILE_NAME, FORMAT};
use crate::JUIZ_VERSION;

/// そのまま (引用符なしで) 書いても同じ文字列として読める字の並びか
fn plain_ok(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphanumeric() || c == '_')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '<' | '>' | '/' | '-'))
        && matches!(Yaml::from_str(s), Yaml::String(_))
}

fn quoted(s: &str) -> String {
    serde_json::to_string(s).expect("a string is JSON")
}

/// 名前・型の式・パス: 書けるならそのまま、書けなければ "…" で囲む
fn word(s: &str) -> String {
    if plain_ok(s) {
        s.to_owned()
    } else {
        quoted(s)
    }
}

/// 値を 1 行の flow 形式で書く (文字列はいつも "…" で囲む)
fn value(v: &Value) -> String {
    match v {
        Value::Null => "null".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => quoted(s),
        Value::Array(a) => format!("[{}]", a.iter().map(value).collect::<Vec<_>>().join(", ")),
        Value::Object(m) if m.is_empty() => "{}".to_owned(),
        Value::Object(m) => format!(
            "{{ {} }}",
            m.iter()
                .map(|(k, v)| format!("{}: {}", word(k), value(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn flow(items: &[(&str, String)]) -> String {
    format!(
        "{{ {} }}",
        items
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn argument(a: &ArgumentDesign) -> String {
    let mut items = vec![("name", word(&a.name)), ("type", word(&a.type_expr))];
    if let Some(d) = &a.default {
        items.push(("default", value(d)));
    }
    if let Some(d) = &a.description {
        items.push(("description", quoted(d)));
    }
    flow(&items)
}

fn state(s: &StateDesign) -> String {
    let mut items = vec![("name", word(&s.name)), ("type", word(&s.type_expr))];
    if let Some(i) = &s.init {
        items.push(("init", word(i)));
    }
    if let Some(d) = &s.default {
        items.push(("default", value(d)));
    }
    if let Some(d) = &s.description {
        items.push(("description", quoted(d)));
    }
    flow(&items)
}

fn list(out: &mut String, pad: &str, key: &str, lines: Vec<String>) {
    if lines.is_empty() {
        return;
    }
    out.push_str(&format!("{pad}{key}:\n"));
    for l in lines {
        out.push_str(&format!("{pad}  - {l}\n"));
    }
}

fn process(out: &mut String, pad: &str, p: &ProcessDesign) {
    out.push_str(&format!("{pad}- name: {}\n", word(&p.name)));
    let pad = format!("{pad}  ");
    if let Some(d) = &p.description {
        out.push_str(&format!("{pad}description: {}\n", quoted(d)));
    }
    list(out, &pad, "arguments", p.arguments.iter().map(argument).collect());
    if let Some(r) = &p.returns {
        out.push_str(&format!("{pad}returns: {}\n", word(r)));
    }
    if p.readonly {
        out.push_str(&format!("{pad}readonly: true\n"));
    }
}

/// 先頭のコメント (§9)。1 行目は JSON Schema の場所
pub fn header(name: &str) -> String {
    format!(
        "# yaml-language-server: $schema=https://raw.githubusercontent.com/OpenJUIZ/juiz/v{JUIZ_VERSION}/juiz_pkg/schema/juizdesign.schema.json\n\
         # {name}: design of the juiz component {name} (juiz docs/spec/design.md)\n\
         #   check:    juiz pkg design check {FILE_NAME}\n\
         #   scaffold: juiz pkg create -d {FILE_NAME} -b <rust|python|cpp|haskell|zig>\n"
    )
}

/// 文書を §9 の形で書く
pub fn to_yaml(d: &Design) -> String {
    let mut out = header(&d.component.name);
    out.push_str(&format!("format: {FORMAT}\ncomponent:\n  name: {}\n", word(&d.component.name)));
    if let Some(desc) = &d.component.description {
        out.push_str(&format!("  description: {}\n", quoted(desc)));
    }
    if let Some(l) = d.component.language {
        out.push_str(&format!("  language: {}\n", l.dir()));
    }
    list(&mut out, "", "idl", d.idl.iter().map(|p| word(p)).collect());
    if !d.processes.is_empty() {
        out.push_str("processes:\n");
        for p in &d.processes {
            process(&mut out, "  ", p);
        }
    }
    if !d.containers.is_empty() {
        out.push_str("containers:\n");
        for c in &d.containers {
            out.push_str(&format!("  - name: {}\n", word(&c.name)));
            if let Some(desc) = &c.description {
                out.push_str(&format!("    description: {}\n", quoted(desc)));
            }
            list(&mut out, "    ", "arguments", c.arguments.iter().map(argument).collect());
            list(&mut out, "    ", "state", c.state.iter().map(state).collect());
            if !c.processes.is_empty() {
                out.push_str("    processes:\n");
                for p in &c.processes {
                    process(&mut out, "      ", p);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn words_and_values() {
        assert_eq!(word("long"), "long");
        assert_eq!(word("array<double>"), "array<double>");
        assert_eq!(word("robot.types.Pose"), "robot.types.Pose");
        assert_eq!(word("long?"), "\"long?\"");
        assert_eq!(word("union { null, string }"), "\"union { null, string }\"");
        // そのまま書くと別の値に読めるものは囲む
        assert_eq!(word("null"), "\"null\"");
        assert_eq!(word("true"), "\"true\"");
        assert_eq!(word("10"), "\"10\"");
        assert_eq!(value(&json!({"x": 1.0, "y": [1, "a"], "z": null})), "{ x: 1.0, y: [1, \"a\"], z: null }");
        assert_eq!(value(&json!({})), "{}");
    }
}
