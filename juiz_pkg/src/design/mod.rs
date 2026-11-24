//! 設計文書 juizdesign.yaml (docs/spec/design.md)
//!
//! Component の Process・Container・ContainerProcess の型を、言語によらずに書いた文書。
//! ここでは文書の読み書き (§3・§9)、検査 (§5)、プリセット (§7.1) を持つ。

mod check;
pub mod command;
pub mod generate;
mod read;
pub(crate) mod write;
pub mod yaml;

use serde_json::Value;

use crate::render::{render, Vars};
use crate::{names, templates, Lang, Template, JUIZ_VERSION};

pub use check::{check, check_and_resolve, Resolved};
pub use read::parse;
pub use write::to_yaml;
pub use yaml::Pos;

/// 文書の既定のファイル名 (§2 の 3)
pub const FILE_NAME: &str = "juizdesign.yaml";

/// 読める文書の形式の版 (§3.2 の format)
pub const FORMAT: i64 = 1;

/// 1 つの文書 = 1 つの Component (§2 の 1)
#[derive(Clone, Debug, PartialEq)]
pub struct Design {
    pub component: ComponentDesign,
    /// 読む .jidl (文書のあるディレクトリからの相対パス、§4.3)
    pub idl: Vec<String>,
    pub processes: Vec<ProcessDesign>,
    pub containers: Vec<ContainerDesign>,
    /// 道筋 (`containers[0].processes[1].arguments[0].type`) → 文書の中の位置。読んだ文書だけが持つ
    pub positions: Vec<(String, Pos)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComponentDesign {
    pub name: String,
    pub description: Option<String>,
    pub language: Option<Lang>,
}

/// Process と ContainerProcess
#[derive(Clone, Debug, PartialEq)]
pub struct ProcessDesign {
    pub name: String,
    pub description: Option<String>,
    pub arguments: Vec<ArgumentDesign>,
    /// 型の式。None は void
    pub returns: Option<String>,
    /// ContainerProcess だけ
    pub readonly: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArgumentDesign {
    pub name: String,
    pub type_expr: String,
    /// 書いたままの既定値 (`default: null` も Some(Null))
    pub default: Option<Value>,
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContainerDesign {
    pub name: String,
    pub description: Option<String>,
    /// Container を作るときの引数
    pub arguments: Vec<ArgumentDesign>,
    pub state: Vec<StateDesign>,
    pub processes: Vec<ProcessDesign>,
}

/// Container が持つ値 (§3.3)
#[derive(Clone, Debug, PartialEq)]
pub struct StateDesign {
    pub name: String,
    pub type_expr: String,
    pub init: Option<String>,
    pub default: Option<Value>,
    pub description: Option<String>,
}

impl Design {
    /// 道筋の位置。無ければ親の道筋の位置
    pub fn position(&self, path: &str) -> Option<Pos> {
        let mut p = path;
        loop {
            if let Some((_, pos)) = self.positions.iter().find(|(q, _)| q == p) {
                return Some(*pos);
            }
            let cut = p.rfind(['.', '['])?;
            p = &p[..cut];
        }
    }

    pub fn container_process_count(&self) -> usize {
        self.containers.iter().map(|c| c.processes.len()).sum()
    }

    /// `component my_robot: 1 process, 1 container, 2 container processes`
    pub fn counts(&self) -> String {
        let n = |k: usize, one: &str, many: &str| format!("{k} {}", if k == 1 { one } else { many });
        format!(
            "component {}: {}, {}, {}",
            self.component.name,
            n(self.processes.len(), "process", "processes"),
            n(self.containers.len(), "container", "containers"),
            n(self.container_process_count(), "container process", "container processes"),
        )
    }
}

/// 検査で見つけたもの 1 つ (§5)
#[derive(Clone, Debug, PartialEq)]
pub struct Diag {
    /// 文書の中の道筋。文書全体のことなら空
    pub path: String,
    pub pos: Option<Pos>,
    pub message: String,
}

impl Diag {
    /// `<file>:<行>:<桁>: <level>: <道筋>: <文>` (pkg.md §2.7)
    pub fn format(&self, file: &str, level: &str) -> String {
        let at = match self.pos {
            Some(p) => format!("{file}:{}:{}", p.line, p.column),
            None => file.to_owned(),
        };
        if self.path.is_empty() {
            format!("{at}: {level}: {}", self.message)
        } else {
            format!("{at}: {level}: {}: {}", self.path, self.message)
        }
    }
}

/// 検査の結果
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub errors: Vec<Diag>,
    pub warnings: Vec<Diag>,
}

impl Report {
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// エラーをすべて 1 つの文にする (`juiz pkg design` などのエラーに使う)
    pub fn error_text(&self, file: &str) -> String {
        self.errors
            .iter()
            .map(|d| d.format(file, "error"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// プリセットの文書 (§7.1)。templates/design/<template>.juizdesign.yaml.tmpl を置換して読む
pub fn preset(name: &str, template: Template) -> Design {
    let text = preset_text(name, template);
    match parse(&text) {
        Ok(d) => Design {
            positions: Vec::new(),
            ..d
        },
        Err(r) => panic!("the preset {} is broken:\n{}", template.dir(), r.error_text("preset")),
    }
}

/// プリセットの文書の文字列 (置換しただけのもの)
pub fn preset_text(name: &str, template: Template) -> String {
    let key = format!("design/{}.juizdesign.yaml.tmpl", template.dir());
    let (_, text) = templates()
        .iter()
        .find(|(p, _)| *p == key)
        .unwrap_or_else(|| panic!("no template {key}"));
    let mut vars = Vars::new();
    vars.set("name", name)
        .set("Name", names::camel(name))
        .set("juiz_version", JUIZ_VERSION);
    render(&text.replace("\r\n", "\n"), &vars).expect("the preset template renders")
}

#[cfg(test)]
pub(crate) mod tests;
