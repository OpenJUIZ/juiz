//! ROS 2 の `.msg` / `.srv` の構文解析 (rosidl_adapter の書き方)

use serde_json::Value;

use super::{ArrayKind, Base, Constant, FieldType, Message, RosField, Service, SubFolder};
use crate::IdlError;

fn syntax(file: &str, line: usize, message: impl Into<String>) -> IdlError {
    IdlError::Syntax {
        file: file.to_owned(),
        line,
        column: 1,
        message: message.into(),
    }
}

/// `#` から行末までを除く (引用符の中の `#` は除かない)。除いたコメントも返す
fn split_comment(line: &str) -> (&str, Option<&str>) {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match (quote, c) {
            (Some(_), '\\') => escaped = true,
            (Some(q), c) if c == q => quote = None,
            (None, '"' | '\'') => quote = Some(c),
            (None, '#') => return (&line[..i], Some(line[i + 1..].trim())),
            _ => {}
        }
    }
    (line, None)
}

/// `.msg` の本文 (`.srv` なら `---` の片側) を読む。`first_line` はファイルの中の最初の行番号
fn parse_message_body(
    lines: &[(usize, &str)],
    package: &str,
    sub: SubFolder,
    name: &str,
    file: &str,
) -> Result<Message, IdlError> {
    let mut message = Message {
        package: package.to_owned(),
        sub,
        name: name.to_owned(),
        doc: None,
        fields: Vec::new(),
        constants: Vec::new(),
    };
    let mut pending: Vec<String> = Vec::new();
    let mut seen_member = false;
    for (n, raw) in lines.iter().copied() {
        let (code, comment) = split_comment(raw);
        let code = code.trim();
        if code.is_empty() {
            match comment {
                Some(c) => pending.push(c.to_owned()),
                None => {
                    // 先頭の、空行で区切ったコメントはメッセージの説明
                    if !seen_member && message.doc.is_none() && !pending.is_empty() {
                        message.doc = Some(pending.join("\n"));
                    }
                    pending.clear();
                }
            }
            continue;
        }
        seen_member = true;
        let mut doc = std::mem::take(&mut pending);
        if let Some(c) = comment.filter(|c| !c.is_empty()) {
            doc.push(c.to_owned());
        }
        let doc = (!doc.is_empty()).then(|| doc.join("\n"));
        let (type_str, rest) = match code.find(char::is_whitespace) {
            Some(i) => (&code[..i], code[i..].trim_start()),
            None => {
                return Err(syntax(
                    file,
                    n,
                    format!("expected a type and a name: '{code}'"),
                ))
            }
        };
        let ty = parse_type(type_str, package).map_err(|m| syntax(file, n, m))?;
        // 定数: `TYPE NAME=VALUE` (= の前後に空白があってもよい)
        let name_end = rest
            .find(|c: char| c.is_whitespace() || c == '=')
            .unwrap_or(rest.len());
        let member_name = &rest[..name_end];
        let after = rest[name_end..].trim_start();
        if let Some(value) = after.strip_prefix('=') {
            let value = parse_literal(value.trim(), &ty, true).map_err(|m| syntax(file, n, m))?;
            message.constants.push(Constant {
                name: member_name.to_owned(),
                ty,
                value,
                doc,
            });
            continue;
        }
        let default = if after.is_empty() {
            None
        } else {
            Some(parse_literal(after, &ty, false).map_err(|m| syntax(file, n, m))?)
        };
        message.fields.push(RosField {
            name: member_name.to_owned(),
            ty,
            default,
            doc,
        });
    }
    if !seen_member && message.doc.is_none() && !pending.is_empty() {
        message.doc = Some(pending.join("\n"));
    }
    Ok(message)
}

/// `.msg` を読む
pub(super) fn parse_msg(
    text: &str,
    package: &str,
    name: &str,
    file: &str,
) -> Result<Message, IdlError> {
    let lines: Vec<(usize, &str)> = text.lines().enumerate().map(|(i, l)| (i + 1, l)).collect();
    parse_message_body(&lines, package, SubFolder::Msg, name, file)
}

/// `.srv` を読む (`---` の前が Request、後が Response)
pub(super) fn parse_srv(
    text: &str,
    package: &str,
    name: &str,
    file: &str,
) -> Result<Service, IdlError> {
    let lines: Vec<(usize, &str)> = text.lines().enumerate().map(|(i, l)| (i + 1, l)).collect();
    let separators: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, (_, l))| split_comment(l).0.trim() == "---")
        .map(|(i, _)| i)
        .collect();
    let [sep] = separators.as_slice() else {
        return Err(syntax(
            file,
            1,
            "a .srv file must have exactly one '---' line between the request and the response",
        ));
    };
    let mut request = parse_message_body(
        &lines[..*sep],
        package,
        SubFolder::Srv,
        &format!("{name}_Request"),
        file,
    )?;
    let response = parse_message_body(
        &lines[sep + 1..],
        package,
        SubFolder::Srv,
        &format!("{name}_Response"),
        file,
    )?;
    let doc = request.doc.take();
    Ok(Service {
        package: package.to_owned(),
        name: name.to_owned(),
        doc,
        request,
        response,
    })
}

/// `int32[<=5]`、`string<=10`、`geometry_msgs/Point[]` などの型
pub(super) fn parse_type(s: &str, package: &str) -> Result<FieldType, String> {
    let (base_str, array) = match s.find('[') {
        Some(i) => {
            let Some(inner) = s[i + 1..].strip_suffix(']') else {
                return Err(format!("invalid array type '{s}'"));
            };
            let array = if inner.is_empty() {
                ArrayKind::Unbounded
            } else if let Some(n) = inner.strip_prefix("<=") {
                ArrayKind::Bounded(parse_bound(n, s)?)
            } else {
                ArrayKind::Fixed(parse_bound(inner, s)?)
            };
            (&s[..i], array)
        }
        None => (s, ArrayKind::None),
    };
    let base = if let Some((kind, bound)) = base_str
        .split_once("<=")
        .filter(|(k, _)| *k == "string" || *k == "wstring")
    {
        Base::String {
            wide: kind == "wstring",
            bound: Some(parse_bound(bound, s)?),
        }
    } else {
        match base_str {
            "string" => Base::String {
                wide: false,
                bound: None,
            },
            "wstring" => Base::String {
                wide: true,
                bound: None,
            },
            p if super::PRIMITIVES.contains(&p) => Base::Primitive(p.to_owned()),
            named => {
                let parts: Vec<&str> = named.split('/').collect();
                let (pkg, name) = match parts.as_slice() {
                    [name] if *name == "Header" && package != "std_msgs" => ("std_msgs", *name),
                    [name] => (package, *name),
                    [pkg, name] | [pkg, "msg", name] => (*pkg, *name),
                    _ => return Err(format!("invalid type '{s}'")),
                };
                if pkg.is_empty() || name.is_empty() {
                    return Err(format!("invalid type '{s}'"));
                }
                Base::Named {
                    package: pkg.to_owned(),
                    name: name.to_owned(),
                }
            }
        }
    };
    Ok(FieldType { base, array })
}

fn parse_bound(n: &str, s: &str) -> Result<u64, String> {
    n.parse::<u64>()
        .map_err(|_| format!("invalid bound '{n}' in '{s}'"))
}

/// 既定値・定数の値を JSON の値にする。`constant` なら文字列は引用符なしでもよい (rosidl と同じ)
pub(super) fn parse_literal(s: &str, ty: &FieldType, constant: bool) -> Result<Value, String> {
    if ty.array == ArrayKind::None {
        return parse_scalar(s, &ty.base, constant);
    }
    let inner = s
        .strip_prefix('[')
        .and_then(|r| r.strip_suffix(']'))
        .or_else(|| s.strip_prefix('(').and_then(|r| r.strip_suffix(')')))
        .ok_or_else(|| format!("an array default must be written as [a, b, ...]: '{s}'"))?;
    let items = split_items(inner)?;
    items
        .iter()
        .map(|i| parse_scalar(i, &ty.base, false))
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}

/// 配列の要素をカンマで分ける (引用符の中のカンマでは分けない)
fn split_items(s: &str) -> Result<Vec<String>, String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for c in s.chars() {
        if escaped {
            current.push(c);
            escaped = false;
            continue;
        }
        match (quote, c) {
            (Some(_), '\\') => {
                current.push(c);
                escaped = true;
            }
            (Some(q), c) if c == q => {
                current.push(c);
                quote = None;
            }
            (None, '"' | '\'') => {
                current.push(c);
                quote = Some(c);
            }
            (None, ',') => items.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    if quote.is_some() {
        return Err(format!("unterminated string in '{s}'"));
    }
    items.push(current);
    let items: Vec<String> = items.into_iter().map(|i| i.trim().to_owned()).collect();
    if items.len() == 1 && items[0].is_empty() {
        return Ok(Vec::new());
    }
    if items.iter().any(|i| i.is_empty()) {
        return Err(format!("empty item in '[{s}]'"));
    }
    Ok(items)
}

fn parse_scalar(s: &str, base: &Base, constant: bool) -> Result<Value, String> {
    match base {
        Base::Primitive(p) => match p.as_str() {
            "bool" => match s.to_ascii_lowercase().as_str() {
                "true" | "1" => Ok(Value::Bool(true)),
                "false" | "0" => Ok(Value::Bool(false)),
                _ => Err(format!("invalid bool value '{s}'")),
            },
            "float32" | "float64" => s
                .parse::<f64>()
                .ok()
                .filter(|f| f.is_finite())
                .and_then(serde_json::Number::from_f64)
                .map(Value::Number)
                .ok_or_else(|| format!("invalid {p} value '{s}'")),
            _ => parse_integer(s)
                .and_then(|v| match i64::try_from(v) {
                    Ok(i) => Some(Value::from(i)),
                    Err(_) => u64::try_from(v).ok().map(Value::from),
                })
                .ok_or_else(|| format!("invalid {p} value '{s}'")),
        },
        Base::String { .. } => match unquote(s)? {
            Some(v) => Ok(Value::String(v)),
            None if constant => Ok(Value::String(s.to_owned())),
            None => Err(format!("a string default must be quoted: {s}")),
        },
        Base::Named { .. } => Err("a field of a message type cannot have a default".to_owned()),
    }
}

/// 10 進・16 進 (`0x`)・8 進 (`0o`)・2 進 (`0b`) の整数。i64 か u64 の範囲
fn parse_integer(s: &str) -> Option<i128> {
    let (neg, body) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let (radix, digits) = if let Some(r) = body.strip_prefix("0x").or(body.strip_prefix("0X")) {
        (16, r)
    } else if let Some(r) = body.strip_prefix("0o") {
        (8, r)
    } else if let Some(r) = body.strip_prefix("0b") {
        (2, r)
    } else {
        (10, body)
    };
    let v = i128::from_str_radix(digits, radix).ok()?;
    Some(if neg { -v } else { v })
}

/// `"…"` / `'…'` の中身 (`\` のエスケープを解く)。引用符で囲んでいなければ `None`
pub(super) fn unquote(s: &str) -> Result<Option<String>, String> {
    let Some(q) = s.chars().next().filter(|c| *c == '"' || *c == '\'') else {
        return Ok(None);
    };
    if s.len() < 2 || !s.ends_with(q) {
        return Err(format!("unterminated string {s}"));
    }
    let body = &s[1..s.len() - 1];
    let mut out = String::new();
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            if c == q {
                return Err(format!("unescaped quote in {s}"));
            }
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some(other) => out.push(other),
            None => return Err(format!("invalid escape in {s}")),
        }
    }
    Ok(Some(out))
}
