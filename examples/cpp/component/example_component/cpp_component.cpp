
#include <iostream>
#include "juiz/juiz.h"

class CppContainer {
public:
    int64_t value;
    CppContainer(int64_t v) : value(v) {}
};

// example cpp container
static auto example_cpp_container_manif = ContainerManifest ("example_cpp_container");
CppContainer* example_cpp_container(juiz::CapsuleMap args) {
    int64_t int_value = 0;
    if (args.has("value")) {
        int_value = args.get_int("value");
    }
    return new CppContainer(int_value);
}
COMPONENT_CONTAINER_FACTORY(example_cpp_container_manif, example_cpp_container);

static ProcessManifest example_container_process_get_manif("example_cpp_container_get");
std::optional<int64_t> example_cpp_container_get(CppContainer* container) {
    if (!container) {
        return std::nullopt;
    }
    return container->value;
}
COMPONENT_CONTAINER_PROCESS_FACTORY(CppContainer, example_container_process_get_manif, example_cpp_container_get);


static auto example_container_process_inc_manif = ProcessManifest("example_cpp_container_increment").add_int_arg("arg0", "", 1);
std::optional<int64_t> example_cpp_container_increment(CppContainer* container, int64_t arg0) {
    if (!container) {
        return std::nullopt;
    }
    container->value += arg0;
    return container->value;
}
COMPONENT_CONTAINER_PROCESS_FACTORY(CppContainer, example_container_process_inc_manif, example_cpp_container_increment);

static auto example_increment_manif = ProcessManifest("example_increment").add_int_arg("arg0", "", 3);
std::optional<int64_t> example_increment(int64_t arg0) {
    return arg0 + 1;
}
COMPONENT_PROCESS_FACTORY(example_increment_manif , example_increment);


auto component_manifest() {
    auto c = ComponentManifest("cpp_component")
      .add_process(example_increment_manifest())
      .add_container(example_cpp_container_manifest()
        .add_process(example_cpp_container_get_manifest())
        .add_process(example_cpp_container_increment_manifest())
      );
    return c;
}
DEFINE_COMPONENT_MANIFEST_ENTRY_POINT(component_manifest) 