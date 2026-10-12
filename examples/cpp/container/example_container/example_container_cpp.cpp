#include <iostream>
#include "juiz/juiz.h"
#include "example_container.h"

auto manifest() {
    return ContainerManifest("example_container_cpp");
}

CppContainer* create_container(juiz::CapsuleMap args) {
    int64_t int_value = 0;
    if (args.has("value")) {
        int_value = args.get_int("value");
    }
    return new CppContainer(int_value);
}


CONTAINER_FACTORY(manifest, create_container);

