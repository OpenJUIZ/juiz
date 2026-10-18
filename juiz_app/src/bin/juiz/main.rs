//! juizローダーアプリ

fn main() {
    std::process::exit(juiz_app::main_with_args(std::env::args_os()));
}
