// 型 (Factory) の既定の 3D モデル (docs/spec/container-model.md §5.6、issue #509) を持つテスト用 Container。
// manifest の model のパスはプラグインのファイル (libmodel_container_cpp.so など) のあるディレクトリからの相対。
// テストは juiz_core/tests/integration/container_model_default_test.rs。
#include "juiz/juiz.h"

class ModelContainer {
public:
    int64_t value;
    ModelContainer(int64_t v) : value(v) {}
};

auto manifest() {
    return ContainerManifest("model_container_cpp")
        .description("Container with a default model")
        .model({
            {"path", "models/model_container_cpp.stl"},
            {"scale", 0.001},
            {"color", "#336699"},
            {"origin", {{"translation", {{"z", -0.02}}}}}
        });
}

ModelContainer* create_container(juiz::CapsuleMap args) {
    return new ModelContainer(0);
}

CONTAINER_FACTORY(manifest, create_container);
