//! コマンドライン (`juiz-idl` と `juiz idl` で同じ引数を使う)

use std::path::PathBuf;

use clap::{Args, ValueEnum};

use crate::{generate, load_files, write_files, IdlError, OutputKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Generator {
    /// 名前付き型ごとの .avsc
    Avsc,
    /// 名前付き型ごとの JSON Schema (.schema.json)
    JsonSchema,
    /// process / container 宣言のシグネチャ (<namespace>.processes.json)
    Processes,
    /// Rust の struct / enum と JuizType の実装 (<namespace>.rs と juiz_idl.rs)
    Rust,
}

impl From<Generator> for OutputKind {
    fn from(g: Generator) -> Self {
        match g {
            Generator::Avsc => OutputKind::Avsc,
            Generator::JsonSchema => OutputKind::JsonSchema,
            Generator::Processes => OutputKind::Processes,
            Generator::Rust => OutputKind::Rust,
        }
    }
}

/// .jidl / .avsc から型のスキーマとコードを生成する
#[derive(Args, Clone, Debug)]
pub struct IdlArgs {
    /// 入力ファイル (.jidl または .avsc)。import した先も読む
    #[arg(required = true)]
    pub files: Vec<PathBuf>,

    /// 出力先のディレクトリ
    #[arg(short = 'o', long = "out-dir", default_value = ".")]
    pub out_dir: PathBuf,

    /// 書き出すもの (カンマ区切りか繰り返し)。省略時は avsc,json-schema,processes
    #[arg(short = 'g', long = "gen", value_enum, value_delimiter = ',')]
    pub generators: Vec<Generator>,

    /// 書き出さずに検査だけして、型の識別子を表示する
    #[arg(long)]
    pub check: bool,
}

/// `IdlArgs` のとおりに実行する
pub fn run(args: &IdlArgs) -> Result<(), IdlError> {
    let model = load_files(&args.files)?;
    if args.check {
        for t in model.types.iter() {
            println!("{}@{}", t.display_name(), t.fingerprint());
        }
        for s in model.signatures.iter() {
            for p in s.processes.iter() {
                println!("process {}.{}", s.namespace, p.type_name);
            }
            for c in s.containers.iter() {
                println!("container {}.{}", s.namespace, c.type_name);
            }
        }
        return Ok(());
    }
    let kinds: Vec<OutputKind> = if args.generators.is_empty() {
        vec![
            OutputKind::Avsc,
            OutputKind::JsonSchema,
            OutputKind::Processes,
        ]
    } else {
        args.generators.iter().map(|g| (*g).into()).collect()
    };
    let files = generate(&model, &kinds)?;
    write_files(&files, &args.out_dir)?;
    for f in files.iter() {
        println!("{}", args.out_dir.join(&f.path).display());
    }
    Ok(())
}
