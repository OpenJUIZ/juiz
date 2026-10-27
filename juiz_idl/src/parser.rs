//! JIDL の字句・構文解析 (仕様 docs/spec/datatype.md §9.2)
//!
//! 結果は名前を解決する前の構文木 ([`IdlFile`])。名前の解決と型の組み立ては `resolve` で行う。

use serde_json::Value;

use crate::IdlError;

/// 1 つの .jidl ファイルの構文木
#[derive(Clone, Debug, PartialEq)]
pub struct IdlFile {
    pub namespace: String,
    pub imports: Vec<Import>,
    pub declarations: Vec<Declaration>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportKind {
    Idl,
    Schema,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Import {
    pub kind: ImportKind,
    pub path: String,
    pub pos: Pos,
}

/// ファイルの中の位置 (1 から数える)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Pos {
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Declaration {
    Record(RecordDecl),
    Enum(EnumDecl),
    Process(ProcessDecl),
    Container(ContainerDecl),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecordDecl {
    pub name: String,
    pub namespace: Option<String>,
    pub doc: Option<String>,
    pub aliases: Vec<String>,
    pub fields: Vec<FieldDecl>,
    pub pos: Pos,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FieldDecl {
    pub name: String,
    pub doc: Option<String>,
    pub aliases: Vec<String>,
    pub type_expr: TypeExpr,
    /// 辞書の表し方 (仕様 §4) の既定値
    pub default: Option<Value>,
    pub pos: Pos,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnumDecl {
    pub name: String,
    pub namespace: Option<String>,
    pub doc: Option<String>,
    pub aliases: Vec<String>,
    pub symbols: Vec<String>,
    pub default: Option<String>,
    pub pos: Pos,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProcessDecl {
    pub name: String,
    pub doc: Option<String>,
    /// `void` は `TypeExpr::Void`
    pub return_type: TypeExpr,
    pub params: Vec<FieldDecl>,
    pub pos: Pos,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContainerDecl {
    pub name: String,
    pub doc: Option<String>,
    pub processes: Vec<ProcessDecl>,
    pub pos: Pos,
}

/// 名前を解決する前の型
#[derive(Clone, Debug, PartialEq)]
pub enum TypeExpr {
    /// `boolean`、`long` などの基本型と `null`
    Primitive(String),
    Void,
    Any,
    Image,
    Array(Box<TypeExpr>),
    Map(Box<TypeExpr>),
    /// `T?` (`null_first` は書いた並び。`T?` は `None` で、既定値から決める)
    Optional {
        inner: Box<TypeExpr>,
        null_first: Option<bool>,
    },
    /// 名前付き型の参照 (書かれたままの名前)
    Named(String),
}

/// JIDL を構文解析する。`file_name` はエラーの表示に使う
pub fn parse_idl(source: &str, file_name: &str) -> Result<IdlFile, IdlError> {
    let mut p = Parser {
        src: source,
        pos: 0,
        file_name,
    };
    p.file()
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    /// 識別子 (`quoted` はバッククォートで囲んだもの。キーワードとして扱わない)
    Ident {
        text: String,
        quoted: bool,
    },
    Str(String),
    Punct(char),
    Eof,
}

const PRIMITIVES: [&str; 8] = [
    "boolean", "int", "long", "float", "double", "string", "bytes", "null",
];

struct Parser<'a> {
    src: &'a str,
    pos: usize,
    file_name: &'a str,
}

impl<'a> Parser<'a> {
    fn position_of(&self, offset: usize) -> Pos {
        let before = &self.src[..offset.min(self.src.len())];
        let line = before.matches('\n').count() + 1;
        let column = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
        Pos { line, column }
    }

    fn error_at(&self, offset: usize, message: impl Into<String>) -> IdlError {
        let p = self.position_of(offset);
        IdlError::Syntax {
            file: self.file_name.to_owned(),
            line: p.line,
            column: p.column,
            message: message.into(),
        }
    }

    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    /// 空白とコメントを飛ばす。`/** … */` があればその中身 (最後のもの) を返す
    fn skip_trivia(&mut self) -> Result<Option<String>, IdlError> {
        let mut doc = None;
        loop {
            let rest = self.rest();
            let trimmed = rest.trim_start();
            self.pos += rest.len() - trimmed.len();
            if trimmed.starts_with("//") {
                let end = trimmed.find('\n').unwrap_or(trimmed.len());
                self.pos += end;
            } else if trimmed.starts_with("/*") {
                let start = self.pos;
                let Some(end) = trimmed[2..].find("*/") else {
                    return Err(self.error_at(start, "unterminated comment"));
                };
                let body = &trimmed[2..2 + end];
                self.pos += 2 + end + 2;
                if body.starts_with('*') && body != "*" {
                    doc = Some(clean_doc(&body[1..]));
                }
            } else {
                return Ok(doc);
            }
        }
    }

    /// 次のトークンを読む (位置を進める)。`/** … */` が前にあれば一緒に返す
    fn next_with_doc(&mut self) -> Result<(Token, usize, Option<String>), IdlError> {
        let doc = self.skip_trivia()?;
        let start = self.pos;
        let rest = self.rest();
        let Some(c) = rest.chars().next() else {
            return Ok((Token::Eof, start, doc));
        };
        if c.is_ascii_alphabetic() || c == '_' {
            let len = rest
                .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'))
                .unwrap_or(rest.len());
            self.pos += len;
            return Ok((
                Token::Ident {
                    text: rest[..len].to_owned(),
                    quoted: false,
                },
                start,
                doc,
            ));
        }
        if c == '`' {
            let Some(end) = rest[1..].find('`') else {
                return Err(self.error_at(start, "unterminated `name`"));
            };
            self.pos += end + 2;
            return Ok((
                Token::Ident {
                    text: rest[1..1 + end].to_owned(),
                    quoted: true,
                },
                start,
                doc,
            ));
        }
        if c == '"' {
            let mut stream = serde_json::Deserializer::from_str(rest).into_iter::<Value>();
            return match stream.next() {
                Some(Ok(Value::String(s))) => {
                    self.pos += stream.byte_offset();
                    Ok((Token::Str(s), start, doc))
                }
                _ => Err(self.error_at(start, "invalid string literal")),
            };
        }
        self.pos += c.len_utf8();
        Ok((Token::Punct(c), start, doc))
    }

    fn next(&mut self) -> Result<(Token, usize), IdlError> {
        let (t, start, _) = self.next_with_doc()?;
        Ok((t, start))
    }

    fn peek(&mut self) -> Result<Token, IdlError> {
        let saved = self.pos;
        let (t, _) = self.next()?;
        self.pos = saved;
        Ok(t)
    }

    fn expect_punct(&mut self, c: char) -> Result<(), IdlError> {
        let (t, start) = self.next()?;
        if t == Token::Punct(c) {
            Ok(())
        } else {
            Err(self.error_at(start, format!("expected '{c}' but found {}", describe(&t))))
        }
    }

    fn eat_punct(&mut self, c: char) -> Result<bool, IdlError> {
        if self.peek()? == Token::Punct(c) {
            self.next()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn is_keyword(&mut self, keyword: &str) -> Result<bool, IdlError> {
        Ok(matches!(self.peek()?, Token::Ident { text, quoted: false } if text == keyword))
    }

    fn expect_keyword(&mut self, keyword: &str) -> Result<(), IdlError> {
        let (t, start) = self.next()?;
        match t {
            Token::Ident {
                text,
                quoted: false,
            } if text == keyword => Ok(()),
            other => Err(self.error_at(
                start,
                format!("expected '{keyword}' but found {}", describe(&other)),
            )),
        }
    }

    /// 名前 (`.` を含まない)
    fn name(&mut self) -> Result<(String, usize), IdlError> {
        let (t, start) = self.next()?;
        match t {
            Token::Ident { text, .. } if !text.contains('.') => Ok((text, start)),
            other => Err(self.error_at(
                start,
                format!("expected a name but found {}", describe(&other)),
            )),
        }
    }

    fn qualified_name(&mut self) -> Result<(String, usize), IdlError> {
        let (t, start) = self.next()?;
        match t {
            Token::Ident { text, .. }
                if !text.starts_with('.') && !text.ends_with('.') && !text.contains("..") =>
            {
                Ok((text, start))
            }
            other => Err(self.error_at(
                start,
                format!("expected a qualified name but found {}", describe(&other)),
            )),
        }
    }

    fn string(&mut self) -> Result<String, IdlError> {
        let (t, start) = self.next()?;
        match t {
            Token::Str(s) => Ok(s),
            other => Err(self.error_at(
                start,
                format!("expected a string but found {}", describe(&other)),
            )),
        }
    }

    /// JSON の値 (既定値・注釈の引数)
    fn json_value(&mut self) -> Result<Value, IdlError> {
        self.skip_trivia()?;
        let start = self.pos;
        let rest = self.rest();
        if !rest.starts_with(['{', '[', '"']) {
            // 数・true・false・null は区切りまでを読む (serde_json の stream は後ろに `;` が続くとエラーにする)
            let end = rest
                .find(|c: char| c.is_whitespace() || matches!(c, ';' | ',' | ')' | '/'))
                .unwrap_or(rest.len());
            return match serde_json::from_str::<Value>(&rest[..end]) {
                Ok(v) if end > 0 => {
                    self.pos += end;
                    Ok(v)
                }
                Ok(_) => Err(self.error_at(start, "expected a JSON value")),
                Err(e) => Err(self.error_at(start, format!("invalid JSON value: {e}"))),
            };
        }
        let mut stream = serde_json::Deserializer::from_str(rest).into_iter::<Value>();
        match stream.next() {
            Some(Ok(v)) => {
                self.pos += stream.byte_offset();
                Ok(v)
            }
            Some(Err(e)) => Err(self.error_at(start, format!("invalid JSON value: {e}"))),
            None => Err(self.error_at(start, "expected a JSON value")),
        }
    }

    fn file(&mut self) -> Result<IdlFile, IdlError> {
        self.expect_keyword("namespace")?;
        let (namespace, _) = self.qualified_name()?;
        self.expect_punct(';')?;
        let mut imports = Vec::new();
        while self.is_keyword("import")? {
            let (_, start) = self.next()?;
            let (kind_token, kind_start) = self.next()?;
            let kind = match kind_token {
                Token::Ident {
                    text,
                    quoted: false,
                } if text == "idl" => ImportKind::Idl,
                Token::Ident {
                    text,
                    quoted: false,
                } if text == "schema" => ImportKind::Schema,
                other => {
                    return Err(self.error_at(
                        kind_start,
                        format!("expected 'idl' or 'schema' but found {}", describe(&other)),
                    ))
                }
            };
            let path = self.string()?;
            self.expect_punct(';')?;
            imports.push(Import {
                kind,
                path,
                pos: self.position_of(start),
            });
        }
        let mut declarations = Vec::new();
        loop {
            let (doc, annotations) = self.doc_and_annotations()?;
            let (t, start) = self.next()?;
            let keyword = match &t {
                Token::Eof if doc.is_none() && annotations.is_empty() => break,
                Token::Ident {
                    text,
                    quoted: false,
                } => text.clone(),
                other => {
                    return Err(self.error_at(
                        start,
                        format!("expected a declaration but found {}", describe(other)),
                    ))
                }
            };
            let decl = match keyword.as_str() {
                "record" => Declaration::Record(self.record(doc, annotations, start)?),
                "enum" => Declaration::Enum(self.enum_decl(doc, annotations, start)?),
                "process" => {
                    no_annotations(self, &annotations, "process")?;
                    Declaration::Process(self.process(doc, start)?)
                }
                "container" => {
                    no_annotations(self, &annotations, "container")?;
                    Declaration::Container(self.container(doc, start)?)
                }
                "import" => {
                    return Err(self.error_at(start, "import must come right after namespace"))
                }
                "namespace" => {
                    return Err(self.error_at(start, "only one namespace can be written in a file"))
                }
                other => {
                    return Err(self.error_at(
                        start,
                        format!("expected record, enum, process or container but found '{other}'"),
                    ))
                }
            };
            declarations.push(decl);
        }
        Ok(IdlFile {
            namespace,
            imports,
            declarations,
        })
    }

    /// 宣言・フィールドの前の `/** … */` と `@注釈(…)`。どちらの順でもよい
    fn doc_and_annotations(&mut self) -> Result<(Option<String>, Vec<Annotation>), IdlError> {
        let mut doc = None;
        let mut annotations = Vec::new();
        loop {
            if let Some(d) = self.skip_trivia()? {
                doc = Some(d);
            }
            if self.rest().starts_with('@') {
                let start = self.pos;
                self.pos += 1;
                let (name, _) = self.name()?;
                self.expect_punct('(')?;
                let value = self.json_value()?;
                self.expect_punct(')')?;
                annotations.push(Annotation { name, value, start });
            } else {
                return Ok((doc, annotations));
            }
        }
    }

    fn record(
        &mut self,
        doc: Option<String>,
        annotations: Vec<Annotation>,
        start: usize,
    ) -> Result<RecordDecl, IdlError> {
        let (name, _) = self.name()?;
        let (namespace, aliases) = self.type_annotations(&annotations)?;
        self.expect_punct('{')?;
        let mut fields = Vec::new();
        loop {
            let (fdoc, fannotations) = self.doc_and_annotations()?;
            if fdoc.is_none() && fannotations.is_empty() && self.eat_punct('}')? {
                break;
            }
            let fstart = self.pos;
            let mut aliases = Vec::new();
            for a in fannotations.iter() {
                match a.name.as_str() {
                    "aliases" => aliases = self.string_list(a)?,
                    other => {
                        return Err(self.error_at(
                            a.start,
                            format!(
                                "annotation @{other} cannot be used on a field (only @aliases)"
                            ),
                        ))
                    }
                }
            }
            let type_expr = self.type_expr()?;
            let (fname, _) = self.name()?;
            let default = if self.eat_punct('=')? {
                Some(self.json_value()?)
            } else {
                None
            };
            self.expect_punct(';')?;
            fields.push(FieldDecl {
                name: fname,
                doc: fdoc,
                aliases,
                type_expr,
                default,
                pos: self.position_of(fstart),
            });
        }
        Ok(RecordDecl {
            name,
            namespace,
            doc,
            aliases,
            fields,
            pos: self.position_of(start),
        })
    }

    fn enum_decl(
        &mut self,
        doc: Option<String>,
        annotations: Vec<Annotation>,
        start: usize,
    ) -> Result<EnumDecl, IdlError> {
        let (name, _) = self.name()?;
        let (namespace, aliases) = self.type_annotations(&annotations)?;
        self.expect_punct('{')?;
        let mut symbols = vec![self.name()?.0];
        while self.eat_punct(',')? {
            symbols.push(self.name()?.0);
        }
        self.expect_punct('}')?;
        let default = if self.eat_punct('=')? {
            Some(self.name()?.0)
        } else {
            None
        };
        self.expect_punct(';')?;
        Ok(EnumDecl {
            name,
            namespace,
            doc,
            aliases,
            symbols,
            default,
            pos: self.position_of(start),
        })
    }

    fn type_annotations(
        &self,
        annotations: &[Annotation],
    ) -> Result<(Option<String>, Vec<String>), IdlError> {
        let mut namespace = None;
        let mut aliases = Vec::new();
        for a in annotations.iter() {
            match a.name.as_str() {
                "aliases" => aliases = self.string_list(a)?,
                "namespace" => match &a.value {
                    Value::String(s) => namespace = Some(s.clone()),
                    _ => return Err(self.error_at(a.start, "@namespace takes a string")),
                },
                other => {
                    return Err(self.error_at(
                        a.start,
                        format!("unknown annotation @{other} (only @aliases and @namespace)"),
                    ))
                }
            }
        }
        Ok((namespace, aliases))
    }

    fn string_list(&self, a: &Annotation) -> Result<Vec<String>, IdlError> {
        match &a.value {
            Value::Array(items) => items
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(|s| s.to_owned())
                        .ok_or_else(|| self.error_at(a.start, "@aliases takes a list of strings"))
                })
                .collect(),
            _ => Err(self.error_at(a.start, "@aliases takes a list of strings")),
        }
    }

    fn process(&mut self, doc: Option<String>, start: usize) -> Result<ProcessDecl, IdlError> {
        let return_type = if self.is_keyword("void")? {
            self.next()?;
            TypeExpr::Void
        } else {
            self.type_expr()?
        };
        let (name, _) = self.name()?;
        self.expect_punct('(')?;
        let mut params = Vec::new();
        if !self.eat_punct(')')? {
            loop {
                let (pdoc, pannotations) = self.doc_and_annotations()?;
                if let Some(a) = pannotations.first() {
                    return Err(self.error_at(a.start, "annotations cannot be used on a parameter"));
                }
                let pstart = self.pos;
                let type_expr = self.type_expr()?;
                let (pname, _) = self.name()?;
                let default = if self.eat_punct('=')? {
                    Some(self.json_value()?)
                } else {
                    None
                };
                params.push(FieldDecl {
                    name: pname,
                    doc: pdoc,
                    aliases: Vec::new(),
                    type_expr,
                    default,
                    pos: self.position_of(pstart),
                });
                if self.eat_punct(')')? {
                    break;
                }
                self.expect_punct(',')?;
            }
        }
        self.expect_punct(';')?;
        Ok(ProcessDecl {
            name,
            doc,
            return_type,
            params,
            pos: self.position_of(start),
        })
    }

    fn container(&mut self, doc: Option<String>, start: usize) -> Result<ContainerDecl, IdlError> {
        let (name, _) = self.name()?;
        self.expect_punct('{')?;
        let mut processes = Vec::new();
        loop {
            let (pdoc, annotations) = self.doc_and_annotations()?;
            no_annotations(self, &annotations, "container_process")?;
            let (t, pstart) = self.next()?;
            match t {
                Token::Punct('}') if pdoc.is_none() => break,
                Token::Ident {
                    text,
                    quoted: false,
                } if text == "container_process" => {
                    processes.push(self.process(pdoc, pstart)?);
                }
                other => {
                    return Err(self.error_at(
                        pstart,
                        format!(
                            "expected container_process or '}}' but found {}",
                            describe(&other)
                        ),
                    ))
                }
            }
        }
        Ok(ContainerDecl {
            name,
            doc,
            processes,
            pos: self.position_of(start),
        })
    }

    fn type_expr(&mut self) -> Result<TypeExpr, IdlError> {
        let (t, start) = self.next()?;
        let base = match t {
            Token::Ident {
                text,
                quoted: false,
            } => match text.as_str() {
                p if PRIMITIVES.contains(&p) => TypeExpr::Primitive(p.to_owned()),
                "any" => TypeExpr::Any,
                "image" => TypeExpr::Image,
                "void" => {
                    return Err(self.error_at(start, "void can be used only as a return type"))
                }
                "array" | "map" => {
                    self.expect_punct('<')?;
                    let inner = self.type_expr()?;
                    self.expect_punct('>')?;
                    if text == "array" {
                        TypeExpr::Array(Box::new(inner))
                    } else {
                        TypeExpr::Map(Box::new(inner))
                    }
                }
                "union" => self.union(start)?,
                _ => TypeExpr::Named(text),
            },
            Token::Ident { text, quoted: true } => TypeExpr::Named(text),
            other => {
                return Err(self.error_at(
                    start,
                    format!("expected a type but found {}", describe(&other)),
                ))
            }
        };
        if self.rest().starts_with('?') {
            self.pos += 1;
            if matches!(base, TypeExpr::Optional { .. }) {
                return Err(self.error_at(start, "an optional type cannot be optional again"));
            }
            return Ok(TypeExpr::Optional {
                inner: Box::new(base),
                null_first: None,
            });
        }
        Ok(base)
    }

    fn union(&mut self, start: usize) -> Result<TypeExpr, IdlError> {
        self.expect_punct('{')?;
        let first = self.type_expr()?;
        self.expect_punct(',')?;
        let second = self.type_expr()?;
        self.expect_punct('}')?;
        let is_null = |t: &TypeExpr| *t == TypeExpr::Primitive("null".to_owned());
        let (inner, null_first) = match (is_null(&first), is_null(&second)) {
            (true, false) => (second, true),
            (false, true) => (first, false),
            _ => {
                return Err(self.error_at(
                    start,
                    "only a union of null and one other type (union { null, T }) is supported",
                ))
            }
        };
        if matches!(inner, TypeExpr::Optional { .. }) {
            return Err(self.error_at(start, "a union cannot contain an optional type"));
        }
        Ok(TypeExpr::Optional {
            inner: Box::new(inner),
            null_first: Some(null_first),
        })
    }
}

struct Annotation {
    name: String,
    value: Value,
    start: usize,
}

fn no_annotations(p: &Parser, annotations: &[Annotation], what: &str) -> Result<(), IdlError> {
    match annotations.first() {
        Some(a) => Err(p.error_at(a.start, format!("annotations cannot be used on {what}"))),
        None => Ok(()),
    }
}

fn describe(t: &Token) -> String {
    match t {
        Token::Ident { text, .. } => format!("'{text}'"),
        Token::Str(s) => format!("string \"{s}\""),
        Token::Punct(c) => format!("'{c}'"),
        Token::Eof => "end of file".to_owned(),
    }
}

/// `/** … */` の中身から行頭の `*` と空白を除く
fn clean_doc(body: &str) -> String {
    let lines: Vec<&str> = body
        .lines()
        .map(|l| {
            let l = l.trim();
            l.strip_prefix('*').map(|s| s.trim_start()).unwrap_or(l)
        })
        .collect();
    let start = lines
        .iter()
        .position(|l| !l.is_empty())
        .unwrap_or(lines.len());
    let end = lines
        .iter()
        .rposition(|l| !l.is_empty())
        .map(|i| i + 1)
        .unwrap_or(start);
    lines[start..end].join("\n")
}
