//! juiz_idl の build.rs ヘルパで生成したコードを読み込むだけのクレート (テストは tests/ にある)

include!(concat!(env!("OUT_DIR"), "/juiz_idl.rs"));
