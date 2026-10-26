//! juiz-idl コマンド (`juiz idl` と同じ)

use clap::Parser;

#[derive(Parser)]
#[command(
    name = "juiz-idl",
    version,
    about = "Generate schemas and code from juiz IDL (.jidl) files"
)]
struct Cli {
    #[command(flatten)]
    args: juiz_idl::cli::IdlArgs,
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = juiz_idl::cli::run(&cli.args) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
