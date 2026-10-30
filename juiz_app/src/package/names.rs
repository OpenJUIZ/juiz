//! パッケージの中のパスの規則(docs/spec/app-package.md §11)と、パッケージ名の規則(§4.2)

use std::collections::HashSet;

/// zip のエントリ名(`/` 区切りの相対パス)が §11 の規則に合うかを調べ、合わなければ理由を返す
pub(crate) fn check_entry_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("empty path".to_owned());
    }
    if name.starts_with('/') || name.starts_with("\\\\") {
        return Err("absolute path".to_owned());
    }
    let b = name.as_bytes();
    if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        return Err("absolute path".to_owned());
    }
    if name.contains('\\') {
        return Err("'\\' is not allowed; use '/'".to_owned());
    }
    for comp in name.split('/') {
        check_component(comp)?;
    }
    Ok(())
}

fn check_component(comp: &str) -> Result<(), String> {
    match comp {
        "" => return Err("empty path element".to_owned()),
        "." => return Err("'.' path element".to_owned()),
        ".." => return Err("'..' path element".to_owned()),
        _ => {}
    }
    if let Some(c) = comp
        .chars()
        .find(|c| c.is_control() || matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
    {
        return Err(format!("character {c:?} is not allowed"));
    }
    if comp.ends_with('.') || comp.ends_with(' ') {
        return Err("a path element must not end with '.' or a space".to_owned());
    }
    let stem = comp.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && matches!(stem.as_bytes()[3], b'1'..=b'9'));
    if reserved {
        return Err(format!("'{comp}' is a reserved name on Windows"));
    }
    Ok(())
}

/// format 1 では入れてはならないパス(§3 の予約、署名と juiz 同梱の単体版用)
pub(crate) fn is_reserved_path(name: &str) -> bool {
    name == "juizapp.yaml.sig" || name == "bin" || name.starts_with("bin/")
}

/// 名前の集まりに、同じパスか大文字小文字だけが違うパスがあれば、その 1 つを返す
pub(crate) fn find_case_collision<'a>(names: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let mut seen = HashSet::new();
    for n in names {
        if !seen.insert(n.to_lowercase()) {
            return Some(n.to_owned());
        }
    }
    None
}

/// conf に書いたパス(相対)を `/` 区切りに正規化する。`.` を除き `..` をたたむ。
/// パッケージの外に出る、または絶対パスなら None
pub(crate) fn normalize_relative(path: &str) -> Option<String> {
    let p = path.replace('\\', "/");
    if p.starts_with('/') {
        return None;
    }
    let b = p.as_bytes();
    if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        return None;
    }
    let mut out: Vec<&str> = Vec::new();
    for comp in p.split('/') {
        match comp {
            "" | "." => {}
            ".." => {
                out.pop()?;
            }
            c => out.push(c),
        }
    }
    Some(out.join("/"))
}

/// パッケージ名の規則(§4.2): `[a-z0-9_-]` の 1〜64 文字で、英小文字で始まる
pub(crate) fn check_package_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 64 {
        return Err(format!("package name '{name}' must be 1 to 64 characters"));
    }
    if !name.starts_with(|c: char| c.is_ascii_lowercase()) {
        return Err(format!(
            "package name '{name}' must start with a lowercase letter (a-z)"
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(format!(
            "package name '{name}' may contain only a-z, 0-9, '_' and '-'"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_entries() {
        for n in [
            "app.conf",
            "plugins/x86_64-unknown-linux-gnu/libtalker.so",
            "python/listener.py",
            "assets/a b.png",
            ".hidden",
        ] {
            assert!(check_entry_name(n).is_ok(), "{n}");
        }
    }

    #[test]
    fn rejects_unsafe_entries() {
        for n in [
            "",
            "/etc/passwd",
            "\\\\server\\share",
            "C:/x",
            "c:x",
            "a\\b",
            "../evil",
            "a/../../evil",
            "a//b",
            "./a",
            "a/./b",
            "a/",
            "a:b",
            "a*b",
            "a?b",
            "a\"b",
            "a<b",
            "a>b",
            "a|b",
            "a\u{1}b",
            "dir./x",
            "x ",
            "CON",
            "con.txt",
            "a/nul.so",
            "COM1",
            "lpt9.log",
        ] {
            assert!(check_entry_name(n).is_err(), "{n:?} should be rejected");
        }
        // COM0 や COM10 は予約名ではない
        assert!(check_entry_name("COM0").is_ok());
        assert!(check_entry_name("COM10").is_ok());
        assert!(check_entry_name("console.txt").is_ok());
    }

    #[test]
    fn reserved_paths() {
        assert!(is_reserved_path("juizapp.yaml.sig"));
        assert!(is_reserved_path("bin/x86_64-unknown-linux-gnu/juiz"));
        assert!(!is_reserved_path("binary/x"));
        assert!(!is_reserved_path("plugins/bin/x"));
    }

    #[test]
    fn case_collisions() {
        assert_eq!(
            find_case_collision(["A.so", "b", "a.so"]),
            Some("a.so".to_owned())
        );
        assert_eq!(find_case_collision(["a", "a"]), Some("a".to_owned()));
        assert_eq!(find_case_collision(["a", "b"]), None);
    }

    #[test]
    fn normalizes_relative_paths() {
        assert_eq!(
            normalize_relative("./plugins/x").as_deref(),
            Some("plugins/x")
        );
        assert_eq!(normalize_relative("a/../b/").as_deref(), Some("b"));
        assert_eq!(normalize_relative(".").as_deref(), Some(""));
        assert_eq!(normalize_relative("../x"), None);
        assert_eq!(normalize_relative("a/../../x"), None);
        assert_eq!(normalize_relative("/abs"), None);
        assert_eq!(normalize_relative("C:\\x"), None);
    }

    #[test]
    fn package_names() {
        for n in ["robot_base", "a", "talker-listener2"] {
            assert!(check_package_name(n).is_ok(), "{n}");
        }
        for n in ["", "Robot", "1robot", "_a", "a.b", "a b", &"a".repeat(65)] {
            assert!(check_package_name(n).is_err(), "{n}");
        }
    }
}
