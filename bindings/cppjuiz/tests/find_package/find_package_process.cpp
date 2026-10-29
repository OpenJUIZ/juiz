#include "juiz/juiz.h"

auto manifest() {
    return ProcessManifest{"find_package_process"}
        .add_int_arg("arg1", "test_argument", 1);
}

std::optional<int64_t> find_package_process(int64_t arg1) {
    return arg1 + 1;
}

PROCESS_FACTORY(manifest, find_package_process);
