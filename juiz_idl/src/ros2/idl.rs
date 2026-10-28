//! ROS 2 の `.idl` の構文解析。rosidl が `.msg` / `.srv` から作る範囲の OMG IDL だけを読む
//! (`module`・`struct`・`typedef`・`const`・`sequence`・`string<N>`・注釈 `@default` / `@verbatim`)

use std::collections::HashMap;

use serde_json::Value;

use super::{
    msg, ArrayKind, Base, Constant, FieldType, Interface, Message, RosField, Service, SubFolder,
};
use crate::IdlError;

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    /// 数 (書かれたまま)
    Number(String),
    /// 文字列 (エスケープを解いたもの)
    Str(String),
    Punct(char),
    /// `::`
    Scope,
    Eof,
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    file: &'a str,
}

impl<'a> Lexer<'a> {
    fn line(&self, offset: usize) -> (usize, usize) {
        let before = &self.src[..offset.min(self.src.len())];
        let line = before.matches('\n').count() + 1;
        let column = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
        (line, column)
    }

    fn error(&self, offset: usize, message: impl Into<String>) -> IdlError {
        let (line, column) = self.line(offset);
        IdlError::Syntax {
            file: self.file.to_owned(),
            line,
            column,
            message: message.into(),
        }
    }

    fn tokens(mut self) -> Result<Vec<(Tok, usize)>, IdlError> {
        let mut out = Vec::new();
        loop {
            let rest = &self.src[self.pos..];
            let trimmed = rest.trim_start();
            self.pos += rest.len() - trimmed.len();
            let start = self.pos;
            let Some(c) = trimmed.chars().next() else {
                out.push((Tok::Eof, start));
                return Ok(out);
            };
            if trimmed.starts_with("//") || (c == '#' && self.at_line_start(start)) {
                // コメントと前処理の行 (#include など。参照先は型の名前から探す)
                self.pos += trimmed.find('\n').unwrap_or(trimmed.len());
            } else if trimmed.starts_with("/*") {
                let Some(end) = trimmed[2..].find("*/") else {
                    return Err(self.error(start, "unterminated comment"));
                };
                self.pos += end + 4;
            } else if c.is_ascii_alphabetic() || c == '_' {
                let len = trimmed
                    .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                    .unwrap_or(trimmed.len());
                out.push((Tok::Ident(trimmed[..len].to_owned()), start));
                self.pos += len;
            } else if c.is_ascii_digit()
                || ((c == '-' || c == '+' || c == '.')
                    && trimmed[1..].starts_with(|d: char| d.is_ascii_digit() || d == '.'))
            {
                let len = trimmed[1..]
                    .find(|ch: char| {
                        !(ch.is_ascii_alphanumeric() || ch == '.' || ch == '_')
                            && !matches!(ch, '+' | '-')
                    })
                    .map(|l| l + 1)
                    .unwrap_or(trimmed.len());
                // 指数の後以外の +/- は数に含めない
                let mut len = len;
                let bytes = trimmed.as_bytes();
                for i in 1..len {
                    if (bytes[i] == b'+' || bytes[i] == b'-')
                        && !matches!(bytes[i - 1], b'e' | b'E')
                    {
                        len = i;
                        break;
                    }
                }
                out.push((Tok::Number(trimmed[..len].to_owned()), start));
                self.pos += len;
            } else if c == '"' {
                let mut escaped = false;
                let mut end = None;
                for (i, ch) in trimmed.char_indices().skip(1) {
                    if escaped {
                        escaped = false;
                    } else if ch == '\\' {
                        escaped = true;
                    } else if ch == '"' {
                        end = Some(i);
                        break;
                    }
                }
                let Some(end) = end else {
                    return Err(self.error(start, "unterminated string"));
                };
                let s = msg::unquote(&trimmed[..=end])
                    .map_err(|m| self.error(start, m))?
                    .unwrap_or_default();
                self.pos += end + 1;
                // 隣り合う文字列はつなげる
                if let Some((Tok::Str(prev), _)) = out.last_mut() {
                    prev.push_str(&s);
                } else {
                    out.push((Tok::Str(s), start));
                }
            } else if trimmed.starts_with("::") {
                out.push((Tok::Scope, start));
                self.pos += 2;
            } else {
                out.push((Tok::Punct(c), start));
                self.pos += c.len_utf8();
            }
        }
    }

    fn at_line_start(&self, offset: usize) -> bool {
        self.src[..offset]
            .rsplit('\n')
            .next()
            .map(|l| l.trim().is_empty())
            .unwrap_or(true)
    }
}

struct Parser<'a> {
    tokens: Vec<(Tok, usize)>,
    i: usize,
    lexer: Lexer<'a>,
    /// typedef の名前 → 型
    typedefs: HashMap<String, FieldType>,
    /// `<Struct>_Constants` モジュールの定数
    constants: HashMap<String, Vec<Constant>>,
    structs: Vec<Message>,
}

#[derive(Default)]
struct Annotations {
    default: Option<Tok>,
    doc: Vec<String>,
}

/// `.idl` を読み、中のメッセージ・サービスを返す
pub(super) fn parse_ros_idl(text: &str, file: &str) -> Result<Vec<Interface>, IdlError> {
    let lexer = Lexer {
        src: text,
        pos: 0,
        file,
    };
    let tokens = Lexer {
        src: text,
        pos: 0,
        file,
    }
    .tokens()?;
    let mut p = Parser {
        tokens,
        i: 0,
        lexer,
        typedefs: HashMap::new(),
        constants: HashMap::new(),
        structs: Vec::new(),
    };
    let mut path = Vec::new();
    while p.peek() != &Tok::Eof {
        p.definition(&mut path)?;
    }
    p.finish()
}

impl<'a> Parser<'a> {
    fn peek(&self) -> &Tok {
        &self.tokens[self.i].0
    }

    fn offset(&self) -> usize {
        self.tokens[self.i].1
    }

    fn next(&mut self) -> Tok {
        let t = self.tokens[self.i].0.clone();
        if t != Tok::Eof {
            self.i += 1;
        }
        t
    }

    fn error(&self, message: impl Into<String>) -> IdlError {
        self.lexer.error(self.offset(), message)
    }

    fn expect_punct(&mut self, c: char) -> Result<(), IdlError> {
        if self.peek() == &Tok::Punct(c) {
            self.next();
            Ok(())
        } else {
            Err(self.error(format!("expected '{c}' but found {:?}", self.peek())))
        }
    }

    fn eat_punct(&mut self, c: char) -> bool {
        if self.peek() == &Tok::Punct(c) {
            self.next();
            true
        } else {
            false
        }
    }

    fn ident(&mut self) -> Result<String, IdlError> {
        match self.peek().clone() {
            Tok::Ident(s) => {
                self.next();
                Ok(s)
            }
            other => Err(self.error(format!("expected a name but found {other:?}"))),
        }
    }

    fn is_ident(&self, s: &str) -> bool {
        matches!(self.peek(), Tok::Ident(t) if t == s)
    }

    fn definition(&mut self, path: &mut Vec<String>) -> Result<(), IdlError> {
        let annotations = self.annotations()?;
        let keyword = self.ident()?;
        match keyword.as_str() {
            "module" => {
                let name = self.ident()?;
                self.expect_punct('{')?;
                path.push(name);
                while !self.eat_punct('}') {
                    if self.peek() == &Tok::Eof {
                        return Err(self.error("unterminated module"));
                    }
                    self.definition(path)?;
                }
                path.pop();
                self.eat_punct(';');
                Ok(())
            }
            "struct" => {
                let s = self.struct_decl(path, annotations)?;
                self.structs.push(s);
                Ok(())
            }
            "typedef" => {
                let ty = self.type_spec(path)?;
                let name = self.ident()?;
                let ty = self.array_suffix(ty)?;
                self.expect_punct(';')?;
                self.typedefs.insert(name, ty);
                Ok(())
            }
            "const" => {
                let ty = self.type_spec(path)?;
                let name = self.ident()?;
                self.expect_punct('=')?;
                let value = self.literal(&ty)?;
                self.expect_punct(';')?;
                let owner = path
                    .last()
                    .and_then(|m| m.strip_suffix("_Constants"))
                    .ok_or_else(|| {
                        self.error("a const must be in a <Type>_Constants module (as rosidl writes)")
                    })?
                    .to_owned();
                let doc = (!annotations.doc.is_empty()).then(|| annotations.doc.join("\n"));
                self.constants.entry(owner).or_default().push(Constant {
                    name,
                    ty,
                    value,
                    doc,
                });
                Ok(())
            }
            other => Err(self.error(format!(
                "'{other}' is not supported (only module, struct, typedef and const as rosidl writes)"
            ))),
        }
    }

    /// `@default (value=…)`・`@verbatim (language="comment", text="…")` を読む。ほかの注釈は読み飛ばす
    fn annotations(&mut self) -> Result<Annotations, IdlError> {
        let mut a = Annotations::default();
        while self.eat_punct('@') {
            let name = self.ident()?;
            let mut args: Vec<(Option<String>, Tok)> = Vec::new();
            if self.eat_punct('(') {
                loop {
                    if self.eat_punct(')') {
                        break;
                    }
                    let mut key = None;
                    if let Tok::Ident(k) = self.peek().clone() {
                        if self.tokens.get(self.i + 1).map(|t| &t.0) == Some(&Tok::Punct('=')) {
                            self.next();
                            self.next();
                            key = Some(k);
                        }
                    }
                    let value = match self.next() {
                        Tok::Punct('-') => match self.next() {
                            Tok::Number(n) => Tok::Number(format!("-{n}")),
                            other => other,
                        },
                        t => t,
                    };
                    args.push((key, value));
                    if !self.eat_punct(',') {
                        self.expect_punct(')')?;
                        break;
                    }
                }
            }
            let arg = |k: &str| {
                args.iter()
                    .find(|(key, _)| key.as_deref() == Some(k))
                    .or_else(|| args.iter().find(|(key, _)| key.is_none()))
                    .map(|(_, v)| v.clone())
            };
            match name.as_str() {
                "default" => {
                    a.default = match arg("value") {
                        Some(t @ (Tok::Str(_) | Tok::Number(_) | Tok::Ident(_))) => Some(t),
                        _ => return Err(self.error("@default takes value=…")),
                    };
                }
                "verbatim" => {
                    let lang = args
                        .iter()
                        .find(|(k, _)| k.as_deref() == Some("language"))
                        .map(|(_, v)| v.clone());
                    if lang == Some(Tok::Str("comment".to_owned())) {
                        if let Some(Tok::Str(t)) = args
                            .iter()
                            .find(|(k, _)| k.as_deref() == Some("text"))
                            .map(|(_, v)| v.clone())
                        {
                            a.doc.push(t);
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(a)
    }

    fn struct_decl(
        &mut self,
        path: &[String],
        annotations: Annotations,
    ) -> Result<Message, IdlError> {
        let name = self.ident()?;
        let (package, sub) = match path {
            [pkg, sub] => (
                pkg.clone(),
                match sub.as_str() {
                    "msg" => SubFolder::Msg,
                    "srv" => SubFolder::Srv,
                    "action" => return Err(self.error("ROS 2 actions are not supported")),
                    other => {
                        return Err(
                            self.error(format!("unknown module '{other}' (expected msg or srv)"))
                        )
                    }
                },
            ),
            _ => return Err(self.error(
                "a struct must be in module <package> { module msg|srv { … } } (as rosidl writes)",
            )),
        };
        self.expect_punct('{')?;
        let mut fields = Vec::new();
        while !self.eat_punct('}') {
            let fa = self.annotations()?;
            let ty = self.type_spec(path)?;
            loop {
                let fname = self.ident()?;
                let fty = self.array_suffix(ty.clone())?;
                let default = match &fa.default {
                    // 文字列の既定値はそのまま。配列の既定値は "(1, 2)" のように文字列で書かれる
                    Some(Tok::Str(s)) if fty.array == ArrayKind::None => match fty.base {
                        Base::String { .. } => Some(Value::String(s.clone())),
                        _ => Some(msg::parse_literal(s, &fty, true).map_err(|m| self.error(m))?),
                    },
                    Some(Tok::Str(s)) => {
                        Some(msg::parse_literal(s, &fty, false).map_err(|m| self.error(m))?)
                    }
                    Some(Tok::Number(n) | Tok::Ident(n)) => {
                        Some(msg::parse_literal(n, &fty, true).map_err(|m| self.error(m))?)
                    }
                    _ => None,
                };
                fields.push(RosField {
                    name: fname,
                    ty: fty,
                    default,
                    doc: (!fa.doc.is_empty()).then(|| fa.doc.join("\n")),
                });
                if !self.eat_punct(',') {
                    break;
                }
            }
            self.expect_punct(';')?;
        }
        self.eat_punct(';');
        // rosidl が空のメッセージに足すフィールド
        fields.retain(|f| f.name != "structure_needs_at_least_one_member");
        Ok(Message {
            package,
            sub,
            name,
            doc: (!annotations.doc.is_empty()).then(|| annotations.doc.join("\n")),
            fields,
            constants: Vec::new(),
        })
    }

    fn array_suffix(&mut self, ty: FieldType) -> Result<FieldType, IdlError> {
        if !self.eat_punct('[') {
            return Ok(ty);
        }
        let n = self.bound()?;
        self.expect_punct(']')?;
        if ty.array != ArrayKind::None {
            return Err(self.error("arrays of arrays are not supported"));
        }
        Ok(FieldType {
            base: ty.base,
            array: ArrayKind::Fixed(n),
        })
    }

    fn bound(&mut self) -> Result<u64, IdlError> {
        match self.next() {
            Tok::Number(n) => n
                .parse::<u64>()
                .map_err(|_| self.error(format!("invalid bound {n}"))),
            other => Err(self.error(format!("expected a number but found {other:?}"))),
        }
    }

    fn type_spec(&mut self, path: &[String]) -> Result<FieldType, IdlError> {
        let first = self.ident()?;
        let scalar = |p: &str| FieldType {
            base: Base::Primitive(p.to_owned()),
            array: ArrayKind::None,
        };
        let ty = match first.as_str() {
            "sequence" => {
                self.expect_punct('<')?;
                let inner = self.type_spec(path)?;
                let array = if self.eat_punct(',') {
                    ArrayKind::Bounded(self.bound()?)
                } else {
                    ArrayKind::Unbounded
                };
                self.expect_punct('>')?;
                if inner.array != ArrayKind::None {
                    return Err(self.error("sequences of arrays are not supported"));
                }
                FieldType {
                    base: inner.base,
                    array,
                }
            }
            "string" | "wstring" => {
                let bound = if self.eat_punct('<') {
                    let n = self.bound()?;
                    self.expect_punct('>')?;
                    Some(n)
                } else {
                    None
                };
                FieldType {
                    base: Base::String {
                        wide: first == "wstring",
                        bound,
                    },
                    array: ArrayKind::None,
                }
            }
            "boolean" => scalar("bool"),
            "octet" => scalar("byte"),
            "char" => scalar("char"),
            "wchar" => scalar("wchar"),
            "short" => scalar("int16"),
            "float" => scalar("float32"),
            "double" => scalar("float64"),
            "unsigned" => {
                let w = self.ident()?;
                match w.as_str() {
                    "short" => scalar("uint16"),
                    "long" if self.is_ident("long") => {
                        self.next();
                        scalar("uint64")
                    }
                    "long" => scalar("uint32"),
                    other => return Err(self.error(format!("unknown type 'unsigned {other}'"))),
                }
            }
            "long" if self.is_ident("long") => {
                self.next();
                scalar("int64")
            }
            "long" if self.is_ident("double") => {
                self.next();
                scalar("long double")
            }
            "long" => scalar("int32"),
            p if super::PRIMITIVES.contains(&p) => scalar(p),
            _ => {
                let mut parts = vec![first];
                while self.peek() == &Tok::Scope {
                    self.next();
                    parts.push(self.ident()?);
                }
                match parts.as_slice() {
                    [name] => match self.typedefs.get(name) {
                        Some(t) => t.clone(),
                        None => FieldType {
                            base: Base::Named {
                                package: path.first().cloned().unwrap_or_default(),
                                name: name.clone(),
                            },
                            array: ArrayKind::None,
                        },
                    },
                    [pkg, _sub, name] => FieldType {
                        base: Base::Named {
                            package: pkg.clone(),
                            name: name.clone(),
                        },
                        array: ArrayKind::None,
                    },
                    _ => {
                        return Err(self.error(format!(
                            "type '{}' must be written as <package>::msg::<Name>",
                            parts.join("::")
                        )))
                    }
                }
            }
        };
        Ok(ty)
    }

    fn literal(&mut self, ty: &FieldType) -> Result<Value, IdlError> {
        let text = match self.next() {
            Tok::Number(n) => n,
            Tok::Ident(i) => i,
            Tok::Str(s) => return Ok(Value::String(s)),
            Tok::Punct('-') => match self.next() {
                Tok::Number(n) => format!("-{n}"),
                other => return Err(self.error(format!("expected a value but found {other:?}"))),
            },
            other => return Err(self.error(format!("expected a value but found {other:?}"))),
        };
        msg::parse_literal(&text, ty, true).map_err(|m| self.error(m))
    }

    fn finish(mut self) -> Result<Vec<Interface>, IdlError> {
        let mut structs = std::mem::take(&mut self.structs);
        for s in structs.iter_mut() {
            if let Some(c) = self.constants.remove(&s.name) {
                s.constants = c;
            }
        }
        let mut out = Vec::new();
        let mut services: Vec<(Message, Option<Message>)> = Vec::new();
        for s in structs {
            match s.sub {
                SubFolder::Msg => out.push(Interface::Message(s)),
                SubFolder::Srv => {
                    if let Some(base) = s.name.strip_suffix("_Request") {
                        let base = base.to_owned();
                        if let Some(entry) = services
                            .iter_mut()
                            .find(|(r, _)| r.name.strip_suffix("_Response") == Some(base.as_str()))
                        {
                            let resp = std::mem::replace(&mut entry.0, s);
                            entry.1 = Some(resp);
                        } else {
                            services.push((s, None));
                        }
                    } else if let Some(base) = s.name.strip_suffix("_Response") {
                        let base = base.to_owned();
                        if let Some(entry) = services.iter_mut().find(|(r, resp)| {
                            resp.is_none() && r.name.strip_suffix("_Request") == Some(base.as_str())
                        }) {
                            entry.1 = Some(s);
                        } else {
                            services.push((s, None));
                        }
                    } else if s.name.ends_with("_Event") {
                        // サービスのイベント (Jazzy 以降) は取り込まない
                    } else {
                        return Err(self.lexer.error(
                            0,
                            format!(
                                "struct '{}' in module srv must end with _Request or _Response",
                                s.name
                            ),
                        ));
                    }
                }
            }
        }
        for (request, response) in services {
            let Some(response) = response else {
                return Err(self.lexer.error(
                    0,
                    format!(
                        "'{}' has no matching _Request / _Response struct",
                        request.name
                    ),
                ));
            };
            let name = request
                .name
                .strip_suffix("_Request")
                .unwrap_or(&request.name)
                .to_owned();
            out.push(Interface::Service(Service {
                package: request.package.clone(),
                name,
                doc: None,
                request,
                response,
            }));
        }
        Ok(out)
    }
}
