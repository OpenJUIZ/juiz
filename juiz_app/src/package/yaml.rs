//! マニフェストの YAML の読み書き。
//!
//! juiz_sdk の `yaml_conf_load` は読めないファイルを `{}` として扱い、null も `{}` にするので、
//! マニフェストと pack の entry の conf は、誤りをそのまま返すこちらで読む。

use juiz_core::anyhow::{anyhow, Result};
use juiz_sdk::serde_json::{self, Map, Value};
use yaml_rust2::{Yaml, YamlLoader};

/// YAML の文字列を読む。文書が無ければ null
pub(crate) fn parse(text: &str) -> Result<Value> {
    let docs = YamlLoader::load_from_str(text).map_err(|e| anyhow!("invalid YAML: {e}"))?;
    match docs.len() {
        0 => Ok(Value::Null),
        1 => to_value(&docs[0]),
        _ => Err(anyhow!("invalid YAML: more than one document")),
    }
}

fn to_value(y: &Yaml) -> Result<Value> {
    Ok(match y {
        Yaml::Real(s) => {
            let f: f64 = s.parse().map_err(|_| anyhow!("invalid YAML number: {s}"))?;
            serde_json::Number::from_f64(f)
                .map(Value::Number)
                .ok_or_else(|| anyhow!("invalid YAML number: {s}"))?
        }
        Yaml::Integer(i) => Value::from(*i),
        Yaml::String(s) => Value::String(s.clone()),
        Yaml::Boolean(b) => Value::Bool(*b),
        Yaml::Null => Value::Null,
        Yaml::Array(a) => Value::Array(a.iter().map(to_value).collect::<Result<_>>()?),
        Yaml::Hash(h) => {
            let mut m = Map::new();
            for (k, v) in h {
                let key = match k {
                    Yaml::String(s) => s.clone(),
                    _ => return Err(anyhow!("invalid YAML: keys must be strings ({k:?})")),
                };
                m.insert(key, to_value(v)?);
            }
            Value::Object(m)
        }
        Yaml::Alias(_) | Yaml::BadValue => return Err(anyhow!("invalid YAML value")),
    })
}

/// 並びを決めた (キー, 値) の列を YAML のブロック形式で書く
pub(crate) fn emit_top(items: &[(String, Value)]) -> String {
    let mut out = String::new();
    for (k, v) in items {
        emit_entry(&mut out, 0, k, v);
    }
    out
}

fn emit_entry(out: &mut String, indent: usize, key: &str, v: &Value) {
    let pad = " ".repeat(indent);
    match v {
        Value::Object(m) if !m.is_empty() => {
            out.push_str(&format!("{pad}{}:\n", key_str(key)));
            for (k, c) in m {
                emit_entry(out, indent + 2, k, c);
            }
        }
        Value::Array(a) if !a.is_empty() => {
            out.push_str(&format!("{pad}{}:\n", key_str(key)));
            for c in a {
                out.push_str(&format!("{pad}  - {}\n", flow(c)));
            }
        }
        _ => out.push_str(&format!("{pad}{}: {}\n", key_str(key), flow(v))),
    }
}

/// 1 行で書く値。入れ子は JSON(YAML の flow 形式として読める)
fn flow(v: &Value) -> String {
    match v {
        Value::String(s) => scalar_str(s),
        Value::Object(m) if m.is_empty() => "{}".to_owned(),
        Value::Array(a) if a.is_empty() => "[]".to_owned(),
        other => other.to_string(),
    }
}

fn key_str(k: &str) -> String {
    if is_plain(k) {
        k.to_owned()
    } else {
        Value::String(k.to_owned()).to_string()
    }
}

fn scalar_str(s: &str) -> String {
    if is_plain(s) {
        s.to_owned()
    } else {
        // JSON の文字列は YAML の二重引用符の文字列としてそのまま読める
        Value::String(s.to_owned()).to_string()
    }
}

/// 引用符なしで書いても同じ文字列として読み戻せるか
fn is_plain(s: &str) -> bool {
    let first_ok = s
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
    first_ok
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | '+'))
        && s.parse::<f64>().is_err()
        && !matches!(
            s.to_ascii_lowercase().as_str(),
            "true" | "false" | "null" | "yes" | "no" | "on" | "off" | "y" | "n" | "inf" | "nan"
        )
        && !s.starts_with("0x")
        && !s.starts_with("0o")
}

#[cfg(test)]
mod tests {
    use super::*;
    use juiz_sdk::serde_json::json;

    #[test]
    fn parse_keeps_null_and_reports_errors() {
        assert_eq!(
            parse("a: ~\nb: 1\nc: x").unwrap(),
            json!({"a": null, "b": 1, "c": "x"})
        );
        assert_eq!(parse("").unwrap(), Value::Null);
        assert!(parse("a: [").is_err());
        assert!(parse("1: a").is_err());
    }

    #[test]
    fn emit_round_trips() {
        let items: Vec<(String, Value)> = vec![
            ("format".into(), json!(1)),
            ("name".into(), json!("talker_listener")),
            ("version".into(), json!("0.1.0")),
            ("v2".into(), json!("1.2")),
            ("b".into(), json!("true")),
            ("juiz".into(), json!("=0.0.28")),
            ("description".into(), json!("a: \"quoted\"\nline")),
            ("depends".into(), json!({"robot_base": ">=1.2, <2"})),
            (
                "python".into(),
                json!({"requires": ["numpy>=1.26"], "version": ">=3.10"}),
            ),
            ("targets".into(), json!(["x86_64-unknown-linux-gnu", "any"])),
            ("empty".into(), json!([])),
            ("emptymap".into(), json!({})),
            (
                "rust".into(),
                json!({"rustc": "rustc 1.91.0 (f8297e351 2025-10-28)"}),
            ),
            (
                "files".into(),
                json!({"plugins/x/libtalker.so": "sha256:00", "app.conf": "sha256:11"}),
            ),
            ("nested".into(), json!([{"a": 1}, [1, 2]])),
            ("port".into(), json!(8100)),
            ("flag".into(), json!(false)),
        ];
        let text = emit_top(&items);
        let back = parse(&text).unwrap();
        for (k, v) in &items {
            assert_eq!(&back[k], v, "{k} in\n{text}");
        }
        assert!(text.contains("name: talker_listener\n"), "{text}");
    }
}
