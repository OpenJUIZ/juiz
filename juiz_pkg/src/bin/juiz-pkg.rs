//! juiz-pkg コマンド (`juiz pkg` と同じ、docs/spec/pkg.md §2.1)

use clap::Parser;

#[derive(Parser)]
#[command(
    name = "juiz-pkg",
    version,
    about = "Create juiz package skeletons (same as juiz pkg)"
)]
struct Cli {
    #[command(flatten)]
    args: juiz_pkg::cli::PkgArgs,
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = juiz_pkg::cli::run(&cli.args) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
