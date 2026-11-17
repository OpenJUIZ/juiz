//! `--launched`(関連付けや別窓から起動された)ときの窓の扱い(docs/spec/app-package.md §17.3、#395)
//!
//! - 窓のタイトルを `juiz - <name> <version>` にする
//! - `juiz run` は止め方を 1 行出す
//! - エラーで終わったら `Press Enter to close.` と出して待つ(エラーの文が窓ごと消えないように)

use std::io::{BufRead, IsTerminal, Write};

/// 窓のタイトルの文字列
pub(crate) fn title(name: &str, version: &str) -> String {
    format!("juiz - {name} {version}")
}

/// 窓のタイトルを変える。Windows は SetConsoleTitleW、ほかは端末のエスケープ(標準エラーが端末のときだけ)
pub(crate) fn set_title(title: &str) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Console::SetConsoleTitleW;
        let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe { SetConsoleTitleW(wide.as_ptr()) };
    }
    #[cfg(not(windows))]
    {
        let mut err = std::io::stderr();
        if err.is_terminal() {
            let _ = write!(err, "{}", title_escape(title));
            let _ = err.flush();
        }
    }
}

/// 端末のタイトルを変えるエスケープ `ESC ] 0 ; <title> BEL`。制御文字は落とす
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn title_escape(title: &str) -> String {
    let clean: String = title.chars().filter(|c| !c.is_control()).collect();
    format!("\x1b]0;{clean}\x07")
}

/// `juiz run --launched` の起動時: タイトルと止め方の 1 行
pub(crate) fn announce_run(name: &str, version: &str) {
    set_title(&title(name, version));
    eprintln!("Close this window or press Ctrl+C to stop.");
}

/// `Press Enter to close.` と出して 1 行読むまで待つ。標準入力が端末でなければ待たない
pub(crate) fn wait_for_enter() {
    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        return;
    }
    wait_for_enter_with(&mut stdin.lock(), &mut std::io::stderr());
}

pub(crate) fn wait_for_enter_with(input: &mut impl BufRead, out: &mut impl Write) {
    let _ = writeln!(out, "Press Enter to close.");
    let _ = out.flush();
    let mut line = String::new();
    let _ = input.read_line(&mut line);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_and_escape() {
        assert_eq!(title("my_patrol", "1.2.0"), "juiz - my_patrol 1.2.0");
        assert_eq!(title_escape("juiz - a\x07b 1"), "\x1b]0;juiz - ab 1\x07");
    }

    #[test]
    fn waits_for_one_line() {
        let mut input = std::io::Cursor::new(b"\nrest\n".to_vec());
        let mut out = Vec::new();
        wait_for_enter_with(&mut input, &mut out);
        assert_eq!(String::from_utf8(out).unwrap(), "Press Enter to close.\n");
        assert_eq!(input.position(), 1);
    }
}
