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
