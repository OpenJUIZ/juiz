// juiz の例外ではない例外を Process / ContainerProcess / Container の作成から投げる例 (issue #400)。
// entry point が捕まえてエラーコード (JUIZ_CPP_EXCEPTION_ERROR など) にするので、juiz は落ちずにエラーを返す。
// juiz のテスト juiz_core/tests/integration/cpp_exception_test.rs が使う。
#include <new>
#include <stdexcept>
#include <string>
#include "juiz/juiz.h"

// kind に合わせて例外を投げる。"none" なら投げない
static void throw_by_kind(const std::string& kind) {
    if (kind == "runtime_error") {
        throw std::runtime_error("cannot write /no/such/dir/map.pgm");
    } else if (kind == "bad_alloc") {
        throw std::bad_alloc();
    } else if (kind == "int") {
        throw 42;
    }
}

static auto cpp_throw_manif = ProcessManifest("cpp_throw")
    .add_string_arg("kind", "runtime_error / bad_alloc / int / none", "none");
std::optional<std::string> cpp_throw(const std::string& kind) {
    throw_by_kind(kind);
    return std::string("ok");
}
COMPONENT_PROCESS_FACTORY(cpp_throw_manif, cpp_throw);

class CppThrowingPart {
public:
    int64_t count = 0;
};

static auto cpp_throwing_part_manif = ContainerManifest("cpp_throwing_part");
CppThrowingPart* cpp_throwing_part(juiz::CapsuleMap) {
    return new CppThrowingPart();
}
COMPONENT_CONTAINER_FACTORY(cpp_throwing_part_manif, cpp_throwing_part);

static auto cpp_part_throw_manif = ProcessManifest("cpp_part_throw")
    .add_string_arg("kind", "runtime_error / bad_alloc / int / none", "none");
std::optional<int64_t> cpp_part_throw(CppThrowingPart* part, const std::string& kind) {
    throw_by_kind(kind);
    part->count += 1;
    return part->count;
}
COMPONENT_CONTAINER_PROCESS_FACTORY(CppThrowingPart, cpp_part_throw_manif, cpp_part_throw);

// 作るときに必ず例外を投げる Container
class CppBrokenPart {};

static auto cpp_broken_part_manif = ContainerManifest("cpp_broken_part");
CppBrokenPart* cpp_broken_part(juiz::CapsuleMap) {
    throw std::runtime_error("cpp_broken_part can not be created");
}
COMPONENT_CONTAINER_FACTORY(cpp_broken_part_manif, cpp_broken_part);

auto component_manifest() {
    return ComponentManifest("cpp_exception_component")
      .add_process(cpp_throw_manifest())
      .add_container(cpp_throwing_part_manifest()
        .add_process(cpp_part_throw_manifest())
      )
      .add_container(cpp_broken_part_manifest());
}
DEFINE_COMPONENT_MANIFEST_ENTRY_POINT(component_manifest)
