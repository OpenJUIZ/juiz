//! コマンドライン (`juiz-idl` と `juiz idl` で同じ引数を使う)

use std::path::PathBuf;

use clap::{Args, ValueEnum};

use crate::{
    generate_with, load_files_with, write_files, ExternNamespace, GenerateOptions, IdlError,
    LoadOptions, OutputKind,
};

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
    /// process / container 宣言の Rust の雛形 (<namespace>.skeleton.rs)。コピーして本体を書く
    RustSkeleton,
    /// Python の dataclass / enum.Enum と型付き client stub (名前空間ごとのパッケージ <a>/<b>/__init__.py)
    Python,
    /// C++ の struct / enum class と cppjuiz の juiz::type_traits の特殊化 (<namespace>.h と juiz_idl.h、ヘッダのみ)
    Cpp,
    /// Haskell の record・直和型と hsjuiz のインスタンス (名前空間ごとのモジュール <A>/<B>.hs)
    Haskell,
    /// TypeScript の interface・文字列リテラルの union と型参照 (<namespace>.ts と juiz_idl.ts)
    Typescript,
}

impl From<Generator> for OutputKind {
    fn from(g: Generator) -> Self {
        match g {
            Generator::Avsc => OutputKind::Avsc,
            Generator::JsonSchema => OutputKind::JsonSchema,
            Generator::Processes => OutputKind::Processes,
            Generator::Rust => OutputKind::Rust,
            Generator::RustSkeleton => OutputKind::RustSkeleton,
            Generator::Python => OutputKind::Python,
            Generator::Cpp => OutputKind::Cpp,
            Generator::Haskell => OutputKind::Haskell,
            Generator::Typescript => OutputKind::TypeScript,
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

    /// import の探し先 (繰り返し可)。import を書いたファイルからの相対パスになければ、この順に探す。
    /// 環境変数 JUIZ_IDL_PATH のディレクトリもこの後に探し、最後に組み込みの型 (juiz_std/…) を探す
    #[arg(short = 'I', long = "import-dir")]
    pub import_dirs: Vec<PathBuf>,

    /// ほかのパッケージが持っている名前空間 (繰り返し可)。その型は書き出さない。
    /// Rust を書き出すときは <名前空間>=<Rust のパス> (例 juiz_std=::juiz_std_types::juiz_std)
    #[arg(long = "extern", value_name = "NAMESPACE[=RUST_PATH]")]
    pub externs: Vec<String>,
}

/// `IdlArgs` のとおりに実行する
pub fn run(args: &IdlArgs) -> Result<(), IdlError> {
    let load = LoadOptions {
        import_dirs: args.import_dirs.clone(),
    }
    .with_env_import_path();
    let model = load_files_with(&args.files, &load)?;
    let options = GenerateOptions {
        externs: args
            .externs
            .iter()
            .map(|e| ExternNamespace::parse(e))
            .collect::<Result<_, _>>()?,
    };
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
    let files = generate_with(&model, &kinds, &options)?;
    write_files(&files, &args.out_dir)?;
    for f in files.iter() {
        println!("{}", args.out_dir.join(&f.path).display());
    }
    Ok(())
}
