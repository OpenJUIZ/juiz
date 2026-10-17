#include "juiz/juiz.h"

// bool を返すプロセスの例 (issue #77)
auto manifest() {
    return ProcessManifest{"is_positive_process_cpp"}
        .add_float_arg("v", "value", 0.0);
}

std::optional<bool> is_positive_process(double v) {
    return v > 0.0;
}

PROCESS_FACTORY(manifest, is_positive_process);
