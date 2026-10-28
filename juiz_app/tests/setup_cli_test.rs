//! `juiz setup home / cleanhome` (issue #222)
//!
//! HOME (Windows では USERPROFILE) を作業ディレクトリに向けて juiz を起動し、~/.juiz を作って消す。
mod common;

use common::{juiz, path_str, temp_dir};
use std::path::Path;

fn setup(home: &Path, args: &[&str]) {
    let out = juiz()
        .env("HOME", home)
        .env("USERPROFILE", home)
        .args(["-f", path_str(&home.join("juiz.conf"))])
        .arg("setup")
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "{args:?}: {out:?}");
}

#[test]
fn setup_home_and_cleanhome() {
    let home = temp_dir("setup_cli_test");
    let juiz_home = home.join(".juiz");
    let conf = juiz_home.join("conf");

    setup(&home, &["home"]);
    assert!(conf.join("default.conf").is_file());
    assert!(juiz_home.join("plugins").join("broker_factories").is_dir());
    let default_conf = std::fs::read_to_string(conf.join("default.conf")).unwrap();
    assert!(!default_conf.is_empty());

    // もう一度実行しても、既にあるものはそのまま
    std::fs::write(conf.join("default.conf"), "name: mine\n").unwrap();
    setup(&home, &["home"]);
    assert_eq!(
        std::fs::read_to_string(conf.join("default.conf")).unwrap(),
        "name: mine\n"
    );

    // -f なしは default.conf を改名するだけ。重なったら _backup1, _backup2 と番号を付ける
    setup(&home, &["cleanhome"]);
    assert!(!conf.join("default.conf").exists());
    assert!(conf.join("default.conf_backup").is_file());
    for expected in ["default.conf_backup1", "default.conf_backup2"] {
        setup(&home, &["home"]);
        setup(&home, &["cleanhome"]);
        assert!(conf.join(expected).is_file(), "{expected}");
    }
    // default.conf が無ければ何もしない
    setup(&home, &["cleanhome"]);
    assert!(juiz_home.join("plugins").is_dir());

    // -f はバックアップごとすべて消す
    setup(&home, &["cleanhome", "-f"]);
    assert!(!juiz_home.exists());
    // ~/.juiz が無くても失敗しない
    setup(&home, &["cleanhome", "-f"]);
    setup(&home, &["cleanhome"]);

    let _ = std::fs::remove_dir_all(&home);
}
