//! juiz-idl に組み込んだ標準の型 `juiz_std` の JIDL (仕様 docs/spec/datatype.md §24)
//!
//! `import idl "juiz_std/geometry.jidl";` は、import を書いたファイルからの相対パスと import の探し先
//! (`-I`) に見つからなければ、ここから読む。コマンドの入力に `juiz_std/geometry.jidl` と書いても同じ。

/// 組み込みのファイルの、パスの頭に付ける印 (エラーの表示と、読んだファイルの区別に使う)
pub const BUILTIN_PREFIX: &str = "<builtin>/";

/// 組み込みのファイル (import に書くパス、中身)
pub const BUILTIN_FILES: [(&str, &str); 3] = [
    (
        "juiz_std/common.jidl",
        include_str!("../std/juiz_std/common.jidl"),
    ),
    (
        "juiz_std/geometry.jidl",
        include_str!("../std/juiz_std/geometry.jidl"),
    ),
    (
        "juiz_std/sensor.jidl",
        include_str!("../std/juiz_std/sensor.jidl"),
    ),
];

/// `juiz_std/geometry.jidl` のようなパス (`./` や `a/../` を含んでもよい) の組み込みのファイル
pub fn builtin_file(path: &str) -> Option<(&'static str, &'static str)> {
    let normalized = normalize(path)?;
    BUILTIN_FILES
        .iter()
        .find(|(p, _)| *p == normalized)
        .copied()
}

/// `/` 区切りにして `.` と `..` を畳む。根より上に出たら None
fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for p in path.split(['/', '\\']) {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_normalized() {
        assert_eq!(
            builtin_file("juiz_std/geometry.jidl").unwrap().0,
            "juiz_std/geometry.jidl"
        );
        assert_eq!(
            builtin_file("./juiz_std/../juiz_std/common.jidl")
                .unwrap()
                .0,
            "juiz_std/common.jidl"
        );
        assert!(builtin_file("../juiz_std/common.jidl").is_none());
        assert!(builtin_file("juiz_std/none.jidl").is_none());
    }
}
