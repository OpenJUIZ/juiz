// 配列とオブジェクトの引数を C++ で受け取れることを確かめるテスト用のプロセス (issue #130)。
// 受け取った値を型が分かる文字列にして返す。
// cm.get_array / cm.get_object は、libjuiz_sdk の value_array_foreach / value_object_foreach の戻り値と
// value_get_bool を使うので、それらの C の宣言が Rust 側の定義と合っていないと結果が壊れる。
// テストは juiz_core/tests/integration/cpp_value_types_test.rs。
#include <string>
#include "juiz/juiz.h"

auto manifest() {
    return ProcessManifest{"value_types_process_cpp"}
        .add_array_arg("list", "values to describe")
        .add_object_arg("obj", "values to describe");
}

static std::string describe(const juiz::Value& v) {
    if (v.isBoolValue()) {
        return v.boolValue() ? "true" : "false";
    } else if (v.isIntValue()) {
        return std::to_string(v.intValue());
    } else if (v.isStringValue()) {
        return "'" + v.stringValue() + "'";
    } else if (v.isListValue()) {
        std::string s = "[";
        for (const auto& e : v.listValue()) {
            if (s.size() > 1) s += ",";
            s += describe(e);
        }
        return s + "]";
    } else if (v.isObjectValue()) {
        std::string s = "{";
        for (const auto& [k, e] : v.objectValue()) {
            if (s.size() > 1) s += ",";
            s += k + ":" + describe(e);
        }
        return s + "}";
    }
    return v.getTypeString();
}

std::optional<std::string> value_types_process(const std::vector<juiz::Value>& list, const juiz::Value& obj) {
    return describe(juiz::Value(list)) + " " + describe(obj);
}

PROCESS_FACTORY(manifest, value_types_process);
