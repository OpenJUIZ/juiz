//! テンプレートの置換 (docs/spec/pkg.md §10.2)
//!
//! `{{変数名}}` を値に置き換える。値が無い変数を含む行は行ごと消す。

use std::collections::BTreeMap;

use crate::PkgError;

/// テンプレートで使える変数。ここに無い名前はエラー (§10.2 の表)
pub const VARIABLES: &[&str] = &[
    "name",
    "Name",
    "name_kebab",
    "juiz_version",
    "plugin_language",
    "plugin_path",
    "juiz_sdk_dep",
    "juiz_sdk_dir",
    "juiz_sdk_dir_arg",
    "hsjuiz_dir",
    "license",
    "author",
    "year",
    "types_table",
    "next_steps",
    "idl_main",
    "idl_conf",
    "idl_extern",
    "juiz_idl_dep",
    "juiz_std_types_dep",
    "idl_depends",
    "idl_hs_modules",
    "idl_hs_gen_dir",
];

/// 変数の値。None は「無し」(その変数を含む行を消す)
#[derive(Clone, Debug, Default)]
pub struct Vars {
    values: BTreeMap<&'static str, Option<String>>,
}

impl Vars {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, name: &'static str, value: impl Into<String>) -> &mut Self {
        assert!(VARIABLES.contains(&name), "unknown template variable {name}");
        self.values.insert(name, Some(value.into()));
        self
    }

    pub fn set_opt(&mut self, name: &'static str, value: Option<String>) -> &mut Self {
        assert!(VARIABLES.contains(&name), "unknown template variable {name}");
        self.values.insert(name, value);
        self
    }

    fn get(&self, name: &str) -> Result<Option<&str>, PkgError> {
        if !VARIABLES.contains(&name) {
            return Err(PkgError::new(format!("unknown template variable '{{{{{name}}}}}'")));
        }
        match self.values.get(name) {
            Some(v) => Ok(v.as_deref()),
            None => Err(PkgError::new(format!("template variable '{{{{{name}}}}}' has no value"))),
        }
    }
}

/// 1 行の中の変数名を、出てくる順に返す。`{{` に対応する `}}` が無い、名前の形が違う、はエラー
pub fn variables_in(line: &str) -> Result<Vec<&str>, PkgError> {
    let mut names = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let end = after
            .find("}}")
            .ok_or_else(|| PkgError::new(format!("unclosed '{{{{' in template line: {line}")))?;
        let name = &after[..end];
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphabetic() || c == '_') {
            return Err(PkgError::new(format!("bad template variable '{{{{{name}}}}}' in: {line}")));
        }
        names.push(name);
        rest = &after[end + 2..];
    }
    Ok(names)
}

fn substitute(line: &str, vars: &Vars) -> Result<Option<String>, PkgError> {
    // 先に形を確かめる (エラーを値の有無より先に出す)
    for name in variables_in(line)? {
        vars.get(name)?;
    }
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let end = after.find("}}").unwrap();
        match vars.get(&after[..end])? {
            Some(v) => {
                out.push_str(&rest[..start]);
                out.push_str(v);
            }
            None => return Ok(None),
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(Some(out))
}

/// テンプレートの中身を置き換える。値が無い変数を含む行は消す
pub fn render(text: &str, vars: &Vars) -> Result<String, PkgError> {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        if let Some(l) = substitute(line, vars)? {
            out.push_str(&l);
        }
    }
    Ok(out)
}

/// パスの中の変数を置き換える。値が無い変数はエラー
pub fn render_path(path: &str, vars: &Vars) -> Result<String, PkgError> {
    match substitute(path, vars)? {
        Some(p) => Ok(p),
        None => Err(PkgError::new(format!("template path {path} uses a variable with no value"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> Vars {
        let mut v = Vars::new();
        v.set("name", "my_robot").set("Name", "MyRobot").set_opt("license", None);
        v
    }

    #[test]
    fn replaces_variables() {
        let r = render("a {{name}} b {{Name}}\n{{name}}{{name}}\n", &vars()).unwrap();
        assert_eq!(r, "a my_robot b MyRobot\nmy_robotmy_robot\n");
    }

    #[test]
    fn removes_lines_with_no_value() {
        let r = render("x\nlicense = \"{{license}}\"\ny {{name}}\n", &vars()).unwrap();
        assert_eq!(r, "x\ny my_robot\n");
    }

    #[test]
    fn values_with_braces_are_not_replaced_again() {
        let mut v = vars();
        v.set("name", "{{Name}}");
        assert_eq!(render("{{name}} {{Name}}", &v).unwrap(), "{{Name}} MyRobot");
    }

    #[test]
    fn single_braces_are_kept() {
        let r = render("path: plugins/{target} ${x} @{ }\n", &vars()).unwrap();
        assert_eq!(r, "path: plugins/{target} ${x} @{ }\n");
    }

    #[test]
    fn unknown_or_unset_variables_are_errors() {
        assert!(render("{{nope}}", &vars()).is_err());
        assert!(render("{{year}}", &vars()).is_err());
        assert!(render("{{name", &vars()).is_err());
        assert!(render("{{na me}}", &vars()).is_err());
    }

    #[test]
    fn paths() {
        assert_eq!(render_path("src/{{name}}.py", &vars()).unwrap(), "src/my_robot.py");
        assert!(render_path("{{license}}", &vars()).is_err());
    }
}
