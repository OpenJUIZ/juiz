#pragma once

// 手で書いた型を JUIZ_TYPE で登録する例 (IDL を使わないとき、docs/spec/datatype.md §21)。
// テストのために、辞書への変換 (encode) と辞書からの変換 (decode) の回数を数える。
// cpp_typed_component と cpp_typed_component_other_abi が使う (回数はプラグインごと)。

#include <cstdint>
#include "juiz/juiz.h"

namespace counted {
    struct Odometry {
        double distance = 0.0;
        int64_t steps = 0;
    };

    // inline 変数にすると GCC はプラグインどうしで共有するので、static にしてプラグインごとに数える
    static int64_t encodes = 0;
    static int64_t decodes = 0;

    static juiz::Value odometry_to_value(const Odometry& o) {
        ++encodes;
        return juiz::Value{{"distance", o.distance}, {"steps", o.steps}};
    }

    static Odometry odometry_from_value(const juiz::Value& v) {
        ++decodes;
        Odometry o;
        o.distance = juiz::type_traits<double>::from_value(v.at("distance"));
        o.steps = juiz::type_traits<int64_t>::from_value(v.at("steps"));
        return o;
    }

    // encode・decode の回数を {encodes, decodes} で返して 0 に戻す
    static juiz::Value take_stats() {
        juiz::Value v{{"encodes", encodes}, {"decodes", decodes}};
        encodes = 0;
        decodes = 0;
        return v;
    }
}

JUIZ_TYPE(counted::Odometry,
    R"({"type": "record", "name": "Odometry", "namespace": "counted", "fields": [
        {"name": "distance", "type": "double"},
        {"name": "steps", "type": "long", "default": 0}
    ]})",
    "1",
    counted::odometry_to_value,
    counted::odometry_from_value)
