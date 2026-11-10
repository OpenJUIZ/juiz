//! コマンドライン (`juiz-pkg` と `juiz pkg` で同じ引数を使う、docs/spec/pkg.md §2)

use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};

use crate::{Lang, PkgError, Template};

/// 雛形を作る (juiz pkg / juiz-pkg)
#[derive(Args, Clone, Debug)]
pub struct PkgArgs {
    #[command(subcommand)]
    pub command: PkgCommand,
}

#[derive(Subcommand, Clone, Debug)]
pub enum PkgCommand {
    /// Create a new package skeleton (a Component that can be packed into a .juizapp as is)
    Create(CreateArgs),
    /// Print the host target triple (the <target> of app/plugins/<target>/)
    Target,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum LicenseKind {
    /// Put LICENSE-MIT and LICENSE-APACHE (MIT OR Apache-2.0)
    MitApache,
    /// No license files
    None,
}

#[derive(Args, Clone, Debug)]
pub struct CreateArgs {
    /// Package name ([a-z][a-z0-9_]*). Used as the Component name and the prefix of type names
    pub name: String,

    /// Language
    #[arg(short = 'b', long = "lang", ignore_case = true)]
    pub lang: Lang,

    /// What to put in the Component
    #[arg(short = 't', long = "template", default_value = "component", ignore_case = true)]
    pub template: Template,

    /// Directory to create (default: ./<name>). It must not exist, or be empty
    #[arg(short = 'o', long = "output")]
    pub output: Option<PathBuf>,

    /// License files to put
    #[arg(long = "license", default_value = "none")]
    pub license: LicenseKind,

    /// Copyright holder for --license mit-apache (default: git config user.name)
    #[arg(long = "author")]
    pub author: Option<String>,

    /// Haskell only: hsjuiz source directory (default: ~/juiz-<version>/bindings/hsjuiz)
    #[arg(long = "hsjuiz-dir")]
    pub hsjuiz_dir: Option<PathBuf>,

    /// C++ and Haskell only: libjuiz_sdk directory (default: ~/.juiz/sdk/<version>)
    #[arg(long = "juiz-sdk-dir")]
    pub juiz_sdk_dir: Option<PathBuf>,

    /// For juiz developers: use the juiz sources in this repository instead of the published SDK
    #[arg(long = "juiz-source")]
    pub juiz_source: Option<PathBuf>,
}

/// サブコマンドを実行する
pub fn run(args: &PkgArgs) -> Result<(), PkgError> {
    match &args.command {
        PkgCommand::Create(c) => {
            let created = crate::create::create(c)?;
            for w in &created.warnings {
                eprintln!("warning: {w}");
            }
            print!("{}", created.summary);
            Ok(())
        }
        PkgCommand::Target => {
            println!("{}", crate::HOST_TARGET);
            Ok(())
        }
    }
}
