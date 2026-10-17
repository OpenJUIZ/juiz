#include "juiz/juiz.h"
#include <algorithm>

// double を返すプロセスの例 (issue #77)
auto manifest() {
    return ProcessManifest{"limit_process_cpp"}
        .add_float_arg("v", "velocity", 0.0)
        .add_float_arg("v_max", "upper bound of |v|", 1.0);
}

std::optional<double> limit_process(double v, double v_max) {
    return std::clamp(v, -v_max, v_max);
}

PROCESS_FACTORY(manifest, limit_process);
