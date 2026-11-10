//! 名前の規則と、名前から作る名前 (docs/spec/pkg.md §3)

use crate::{Lang, PkgError};

/// .juizapp のパッケージ名の上限 (app-package.md §4.2)
pub const MAX_NAME_LEN: usize = 64;

/// 名前に使えない語。Rust・Python・C++ のキーワードと、依存先や標準のものと衝突する名前 (§3 の 4)
pub const RESERVED: &[&str] = &[
    // juiz と依存先・標準のもの
    "juiz", "juiz_sdk", "pyjuiz", "hsjuiz", "cppjuiz", "std", "core", "alloc", "test", "main",
    // Rust
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use",
    "where", "while", "abstract", "become", "box", "do", "final", "gen", "macro", "override",
    "priv", "try", "typeof", "unsized", "virtual", "yield",
    // Python
    "and", "assert", "case", "class", "def", "del", "elif", "except", "finally", "from", "global",
    "import", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "with",
    // C++
    "alignas", "alignof", "and_eq", "asm", "auto", "bitand", "bitor", "bool", "catch", "char",
    "char8_t", "char16_t", "char32_t", "compl", "concept", "consteval", "constexpr", "constinit",
    "const_cast", "co_await", "co_return", "co_yield", "decltype", "default", "delete", "double",
    "dynamic_cast", "explicit", "export", "float", "friend", "goto", "inline", "int", "long",
    "mutable", "namespace", "new", "noexcept", "not_eq", "nullptr", "operator", "or_eq",
    "private", "protected", "public", "register", "reinterpret_cast", "requires", "short",
    "signed", "sizeof", "static_assert", "static_cast", "switch", "template", "this",
    "thread_local", "throw", "typedef", "typeid", "typename", "union", "unsigned", "using", "void",
    "volatile", "wchar_t", "xor", "xor_eq",
];

/// C++ だけで使えない名前。cppjuiz の大域のクラス (`<Name>` と衝突) と、
/// DEFINE_COMPONENT_MANIFEST_ENTRY_POINT が作る component_manifest_entry_point と衝突するもの
/// (Container `<name>` から COMPONENT_CONTAINER_FACTORY が `<name>_manifest_entry_point`・`<name>_entry_point` を作る)
pub const CPP_RESERVED: &[&str] = &[
    "component",
    "component_manifest",
    "process_manifest",
    "container_manifest",
    "argument_manifest",
    "topic_manifest",
];

/// Haskell だけで使えない名前 (Step 3)。GHC に付いてくるパッケージの名前 (`<name-kebab>` が cabal のパッケージ名として
/// 衝突し、hsjuiz の依存が雛形そのものに解決されてしまう) と、base の最上位のモジュール (`<Name>` のモジュール名と衝突する)
pub const HASKELL_RESERVED: &[&str] = &[
    // GHC に付いてくるパッケージ
    "base", "ghc", "ghc_prim", "ghc_bignum", "ghc_boot", "ghc_boot_th", "ghc_heap", "ghc_internal",
    "ghc_platform", "ghc_toolchain", "ghc_compact", "ghc_experimental", "ghci", "integer_gmp", "rts",
    "array", "binary", "bytestring", "containers", "deepseq", "directory", "exceptions", "file_io",
    "filepath", "haskeline", "hpc", "libiserv", "mtl", "os_string", "parsec", "pretty", "process",
    "semaphore_compat", "stm", "template_haskell", "terminfo", "text", "time", "transformers", "unix",
    "xhtml",
    // base の最上位のモジュール
    "prelude", "numeric", "foreign",
];

/// 規則に合わない理由を返す (合えば None)
fn violation(name: &str, lang: Lang) -> Option<String> {
    let mut chars = name.chars();
    let first_ok = matches!(chars.next(), Some('a'..='z'));
    if !first_ok || !name.chars().all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_')) {
        return Some(
            "it must start with a lowercase letter and contain only lowercase letters, digits and '_' ([a-z][a-z0-9_]*)"
                .to_owned(),
        );
    }
    if name.len() > MAX_NAME_LEN {
        return Some(format!("it must be at most {MAX_NAME_LEN} characters"));
    }
    if name.contains("__") || name.ends_with('_') {
        return Some("it must not contain '__' or end with '_'".to_owned());
    }
    if RESERVED.contains(&name) {
        return Some(format!("'{name}' is a reserved word"));
    }
    if lang == Lang::Haskell && name.split('_').any(|p| !p.chars().any(|c| c.is_ascii_alphabetic())) {
        return Some(
            "for Haskell, each part separated by '_' must contain a letter (cabal package names cannot have a part with only digits)"
                .to_owned(),
        );
    }
    if lang == Lang::Haskell && HASKELL_RESERVED.contains(&name) {
        return Some(format!(
            "for Haskell, '{name}' clashes with a package that comes with GHC or a module of base ({} / {})",
            kebab(name),
            camel(name)
        ));
    }
    if lang == Lang::Cpp && CPP_RESERVED.contains(&name) {
        return Some(format!(
            "for C++, '{name}' clashes with a name in the juiz C++ headers (the class {} or the function component_manifest_entry_point)",
            camel(name)
        ));
    }
    if let Err(e) = juiz_sdk::identifier::validate_name(name) {
        return Some(format!("{e}"));
    }
    None
}

/// 直した名前の例 (大文字を小文字に、使えない文字を _ に)。直しても規則に合わなければ None
pub fn suggest(name: &str, lang: Lang) -> Option<String> {
    let mut s: String = name
        .chars()
        .map(|c| {
            let c = c.to_ascii_lowercase();
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                c
            } else {
                '_'
            }
        })
        .collect();
    while s.contains("__") {
        s = s.replace("__", "_");
    }
    let s = s.trim_matches('_').to_owned();
    if s != name && violation(&s, lang).is_none() {
        Some(s)
    } else {
        None
    }
}

/// 名前の規則 (§3) を確かめる
pub fn validate(name: &str, lang: Lang) -> Result<(), PkgError> {
    match violation(name, lang) {
        None => Ok(()),
        Some(reason) => {
            let mut msg = format!("invalid package name '{name}': {reason}");
            if let Some(s) = suggest(name, lang) {
                msg.push_str(&format!(" (for example: '{s}')"));
            }
            Err(PkgError::new(msg))
        }
    }
}

/// `my_robot` → `MyRobot` (§3.1)
pub fn camel(name: &str) -> String {
    name.split('_')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut cs = p.chars();
            match cs.next() {
                Some(c) => c.to_ascii_uppercase().to_string() + cs.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// `my_robot` → `my-robot` (§3.1)
pub fn kebab(name: &str) -> String {
    name.replace('_', "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_names() {
        for n in ["my_robot", "a", "arm2", "x_1y", "camera_v4l2"] {
            assert!(validate(n, Lang::Rust).is_ok(), "{n}");
        }
        assert!(validate(&"a".repeat(64), Lang::Python).is_ok());
    }

    #[test]
    fn rejects_invalid_names() {
        for n in [
            "", "MyRobot", "my-robot", "1robot", "_robot", "my__robot", "robot_", "my robot",
            "ロボット", "type", "class", "import", "fn", "int", "juiz", "std", "test",
        ] {
            assert!(validate(n, Lang::Rust).is_err(), "{n}");
        }
        assert!(validate(&"a".repeat(65), Lang::Rust).is_err());
    }

    #[test]
    fn haskell_parts_need_a_letter() {
        assert!(validate("arm_2", Lang::Rust).is_ok());
        assert!(validate("arm_2", Lang::Haskell).is_err());
        assert!(validate("arm_v2", Lang::Haskell).is_ok());
    }

    #[test]
    fn haskell_reserved_names() {
        for n in HASKELL_RESERVED {
            assert!(validate(n, Lang::Haskell).is_err(), "{n}");
        }
        assert!(validate("process", Lang::Rust).is_ok());
        assert!(validate("my_process", Lang::Haskell).is_ok());
    }

    #[test]
    fn cpp_reserved_names() {
        for n in CPP_RESERVED {
            assert!(validate(n, Lang::Rust).is_ok(), "{n}");
            assert!(validate(n, Lang::Cpp).is_err(), "{n}");
        }
        assert!(validate("my_component", Lang::Cpp).is_ok());
    }

    #[test]
    fn error_has_suggestion() {
        let e = validate("My-Robot", Lang::Rust).unwrap_err().to_string();
        assert!(e.contains("'my_robot'"), "{e}");
        let e = validate("type", Lang::Rust).unwrap_err().to_string();
        assert!(e.contains("reserved"), "{e}");
        assert!(!e.contains("for example"), "{e}");
    }

    #[test]
    fn derived_names() {
        assert_eq!(camel("my_robot"), "MyRobot");
        assert_eq!(camel("arm2_v3"), "Arm2V3");
        assert_eq!(camel("a"), "A");
        assert_eq!(kebab("my_robot"), "my-robot");
    }
}
