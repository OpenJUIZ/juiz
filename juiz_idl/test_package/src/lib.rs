//! juiz_idl の build.rs ヘルパで生成したコードを読み込むだけのクレート (テストは tests/ にある)

include!(concat!(env!("OUT_DIR"), "/juiz_idl.rs"));

/// process / container 宣言から作った雛形 (コンパイルでき、manifest がシグネチャと一致することを tests/ で確かめる)
pub mod skeleton {
    include!(concat!(env!("OUT_DIR"), "/robot.geometry.skeleton.rs"));

    /// マクロが作った manifest (manifest の関数は pub(crate) なので、テストのためにここから出す)
    pub fn manifests() -> (
        Vec<ProcessManifest>,
        ContainerManifest,
        Vec<ProcessManifest>,
    ) {
        (
            vec![move_to_manifest(), capture_gray_manifest()],
            motor_manifest(),
            vec![set_mode_manifest(), stop_manifest()],
        )
    }
}
