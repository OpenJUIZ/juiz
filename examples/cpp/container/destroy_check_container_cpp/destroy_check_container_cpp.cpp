// Container を削除したときに juiz が解放関数 (container_factory_destroy_entry_point) を呼び、
// C++ のオブジェクトが delete されることを確かめるためのテスト用 Container (issue #130)。
// 生きているオブジェクトの数を destroy_check_container_cpp_alive_count() で返す。
// テストは juiz_core/tests/cpp_container_destroy_test.rs。
#include <atomic>
#include "juiz/juiz.h"

static std::atomic<int64_t> alive_count{0};

class DestroyCheckContainer {
public:
    DestroyCheckContainer() { alive_count++; }
    ~DestroyCheckContainer() { alive_count--; }
};

extern "C" JUIZ_API int64_t destroy_check_container_cpp_alive_count() {
    return alive_count.load();
}

auto manifest() {
    return ContainerManifest("destroy_check_container_cpp");
}

DestroyCheckContainer* create_container(juiz::CapsuleMap args) {
    return new DestroyCheckContainer();
}

CONTAINER_FACTORY(manifest, create_container);
