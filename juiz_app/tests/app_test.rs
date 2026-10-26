// extern crate juiz_meta;
// use juiz_core::prelude::*;

use assert_cmd::prelude::*; // Add methods on commands
                            // use predicates::prelude::*; // Used for writing assertions
use std::process::Command; // Run programs

#[test]
fn file_doesnt_exist() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("juiz")?;
    // cmd.arg("-d");
    // cmd.arg("foobar").arg("test/file/doesnt/exist");
    cmd.assert()
    //     .failure()
    //    .stderr(predicate::str::contains("could not read file"));
    ;
    println!("OK?");
    Ok(())
}

#[test]
fn main_with_args_help_returns_zero() {
    assert_eq!(juiz_app::main_with_args(["juiz", "--help"]), 0);
}

#[test]
fn main_with_args_bad_option_returns_error_code() {
    assert_eq!(juiz_app::main_with_args(["juiz", "--no-such-option"]), 2);
}

/// juiz idl は設定ファイルを読まずに juiz-idl と同じように生成する (issue #191)
#[test]
fn idl_subcommand_generates_files() {
    let dir = std::env::temp_dir().join(format!("juiz_app_idl_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("t.jidl");
    std::fs::write(&input, "namespace t;\nrecord R { long x; }\n").unwrap();
    let out = dir.join("out");
    let code = juiz_app::main_with_args([
        "juiz".as_ref(),
        "idl".as_ref(),
        "-o".as_ref(),
        out.as_os_str(),
        "-g".as_ref(),
        "avsc,rust".as_ref(),
        input.as_os_str(),
    ] as [&std::ffi::OsStr; 7]);
    assert_eq!(code, 0);
    assert!(out.join("t.R.avsc").exists());
    assert!(out.join("t.rs").exists());
    std::fs::write(&input, "namespace t;\nrecord R { long x }\n").unwrap();
    assert_eq!(
        juiz_app::main_with_args(
            ["juiz".as_ref(), "idl".as_ref(), input.as_os_str()] as [&std::ffi::OsStr; 3]
        ),
        1
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
