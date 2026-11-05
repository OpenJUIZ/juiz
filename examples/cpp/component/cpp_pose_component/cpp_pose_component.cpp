// C++ の ContainerProcess から ContainerPose を読み書きする例 (Transformations Step 5、issue #372)。
// juiz のテスト juiz_core/tests/integration/cpp_transformations_test.rs が使う。
// 仕様は docs/spec/transformations.md §12.3。
#include <chrono>
#include <string>
#include <thread>
#include "juiz/juiz.h"

// 利用者の状態は持たない部品。位置姿勢は juiz が Container ごとに持つ (利用者の状態とは別)
class CppPart {
public:
    int64_t id = 0;
};

static auto cpp_part_manif = ContainerManifest("cpp_part");
CppPart* cpp_part(juiz::CapsuleMap) {
    return new CppPart();
}
COMPONENT_CONTAINER_FACTORY(cpp_part_manif, cpp_part);

// 基準 reference から見た自分の ContainerPose (juiz_std.geometry.PoseStamped の辞書)
static auto cpp_part_pose_manif = ProcessManifest("cpp_part_pose")
    .add_string_arg("reference", "基準 Container", "map");
std::optional<juiz::Value> cpp_part_pose(CppPart* part, const std::string& reference) {
    return juiz::pose_to_value(juiz::pose(part, reference), reference);
}
COMPONENT_CONTAINER_PROCESS_FACTORY(CppPart, cpp_part_pose_manif, cpp_part_pose);

// 自分を基準 reference から見て (x, y, z)・(qx, qy, qz, qw) にする
static auto cpp_part_set_pose_manif = ProcessManifest("cpp_part_set_pose")
    .add_string_arg("reference", "基準 Container", "map")
    .add_float_arg("x", "", 0.0).add_float_arg("y", "", 0.0).add_float_arg("z", "", 0.0)
    .add_float_arg("qx", "", 0.0).add_float_arg("qy", "", 0.0).add_float_arg("qz", "", 0.0).add_float_arg("qw", "", 1.0);
std::optional<juiz::Value> cpp_part_set_pose(CppPart* part, const std::string& reference, const double x, const double y, const double z,
                                             const double qx, const double qy, const double qz, const double qw) {
    auto p = juiz::set_pose(part, reference, juiz::make_pose(x, y, z, qx, qy, qz, qw));
    return juiz::pose_to_value(p, reference);
}
COMPONENT_CONTAINER_PROCESS_FACTORY(CppPart, cpp_part_set_pose_manif, cpp_part_set_pose);

// 読み書きを試し、エラーの種類 (例外のクラス名) か "ok" を返す
static auto cpp_part_try_manif = ProcessManifest("cpp_part_try")
    .add_string_arg("op", "pose / set_pose / set_pose_zero / other_object", "pose")
    .add_string_arg("reference", "基準 Container", "map");
std::optional<std::string> cpp_part_try(CppPart* part, const std::string& op, const std::string& reference) {
    try {
        if (op == "pose") {
            juiz::pose(part, reference);
        } else if (op == "set_pose") {
            juiz::set_pose(part, reference, juiz::make_pose(1.0, 0.0, 0.0));
        } else if (op == "set_pose_zero") {
            // 長さ 0 の四元数
            juiz::set_pose(part, reference, juiz::make_pose(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0));
        } else if (op == "other_object") {
            // 今呼ばれている ContainerProcess の Container ではない
            CppPart other;
            juiz::pose(&other, reference);
        }
        return std::string("ok");
    } catch (juiz::TransformationsNotAttachedError&) {
        return std::string("TransformationsNotAttachedError");
    } catch (juiz::NoCommonAnchorError&) {
        return std::string("NoCommonAnchorError");
    } catch (juiz::PoseNotWritableError&) {
        return std::string("PoseNotWritableError");
    } catch (juiz::InvalidPoseValueError&) {
        return std::string("InvalidPoseValueError");
    } catch (juiz::TransformationsError& e) {
        return std::string("TransformationsError: ") + e.what();
    }
}
COMPONENT_CONTAINER_PROCESS_FACTORY(CppPart, cpp_part_try_manif, cpp_part_try);

// 例外を外に出す (entry point がエラーコードにして juiz に返す)
static auto cpp_part_pose_unchecked_manif = ProcessManifest("cpp_part_pose_unchecked")
    .add_string_arg("reference", "基準 Container", "map");
std::optional<juiz::Value> cpp_part_pose_unchecked(CppPart* part, const std::string& reference) {
    return juiz::pose_to_value(juiz::pose(part, reference).pose);
}
COMPONENT_CONTAINER_PROCESS_FACTORY(CppPart, cpp_part_pose_unchecked_manif, cpp_part_pose_unchecked);

// readonly の ContainerProcess (#377)。Container を const のポインタで受け取り、manifest に readonly() を付ける。
// juiz は Container を読み出しロックで呼ぶので、同じ Container の readonly 同士は並行に走る。
// sleep_ms だけ待ってから、基準 reference から見た自分の ContainerPose を返す
static auto cpp_part_peek_manif = ProcessManifest("cpp_part_peek")
    .readonly()
    .add_string_arg("reference", "基準 Container", "map")
    .add_int_arg("sleep_ms", "待つ時間 [ms]", 0);
std::optional<juiz::Value> cpp_part_peek(const CppPart* part, const std::string& reference, const int64_t sleep_ms) {
    std::this_thread::sleep_for(std::chrono::milliseconds(sleep_ms));
    return juiz::pose_to_value(juiz::pose(part, reference), reference);
}
COMPONENT_CONTAINER_PROCESS_FACTORY(CppPart, cpp_part_peek_manif, cpp_part_peek);

// readonly でない ContainerProcess。sleep_ms だけ待つ (並行に走らないことの比較用)
static auto cpp_part_sleep_manif = ProcessManifest("cpp_part_sleep")
    .add_int_arg("sleep_ms", "待つ時間 [ms]", 0);
std::optional<int64_t> cpp_part_sleep(CppPart* part, const int64_t sleep_ms) {
    std::this_thread::sleep_for(std::chrono::milliseconds(sleep_ms));
    part->id += 1;
    return part->id;
}
COMPONENT_CONTAINER_PROCESS_FACTORY(CppPart, cpp_part_sleep_manif, cpp_part_sleep);

// readonly の中から書こうとする。juiz::set_pose は const の Container では static_assert で止まるので、
// libjuiz_sdk の C の関数を直接呼ぶ。juiz が渡す口が JUIZ_POSE_NOT_WRITABLE_ERROR を返す
static auto cpp_part_try_readonly_manif = ProcessManifest("cpp_part_try_readonly")
    .readonly()
    .add_string_arg("reference", "基準 Container", "map");
std::optional<std::string> cpp_part_try_readonly(const CppPart* part, const std::string& reference) {
    capsule_map* cm = juiz::detail::require_pose_call(part);
    juiz_pose p = juiz::make_pose(1.0, 0.0, 0.0);
    juiz_pose_stamped out{};
    int64_t r = juiz_container_pose_set(cm, reference.c_str(), &p, &out);
    if (r == JUIZ_POSE_NOT_WRITABLE_ERROR) {
        return std::string("PoseNotWritableError");
    }
    return std::string("returned ") + std::to_string(r);
}
COMPONENT_CONTAINER_PROCESS_FACTORY(CppPart, cpp_part_try_readonly_manif, cpp_part_try_readonly);

// 自分以外の Container target を基準 reference から見た ContainerPose (#409、tf の lookupTransform)。
// 読み出しだけなので readonly の ContainerProcess から呼べる
static auto cpp_part_lookup_manif = ProcessManifest("cpp_part_lookup")
    .readonly()
    .add_string_arg("target", "読みたい Container", "base0")
    .add_string_arg("reference", "基準 Container", "map");
std::optional<juiz::Value> cpp_part_lookup(const CppPart* part, const std::string& target, const std::string& reference) {
    try {
        return juiz::pose_to_value(juiz::lookup(part, target, reference), reference);
    } catch (juiz::NoCommonAnchorError&) {
        return juiz::Value("NoCommonAnchorError");
    }
}
COMPONENT_CONTAINER_PROCESS_FACTORY(CppPart, cpp_part_lookup_manif, cpp_part_lookup);

// Transform (source → destination) の値を平面の (x, y, yaw) にする (#410)。書いた後の TransformStamped の辞書を返す。
// 端点でも writers でもない・Transform が無いときはエラーの種類 (例外のクラス名) を返す
static auto cpp_part_set_transform_manif = ProcessManifest("cpp_part_set_transform")
    .add_string_arg("source", "始点", "map")
    .add_string_arg("destination", "終点", "base0")
    .add_float_arg("x", "", 0.0).add_float_arg("y", "", 0.0).add_float_arg("yaw", "", 0.0);
std::optional<juiz::Value> cpp_part_set_transform(CppPart* part, const std::string& source, const std::string& destination,
                                                  const double x, const double y, const double yaw) {
    try {
        auto t = juiz::set_transform(part, source, destination, juiz::make_transform_2d(x, y, yaw));
        return juiz::transform_to_value(t, source, destination);
    } catch (juiz::TransformNotWritableError&) {
        return juiz::Value("TransformNotWritableError");
    } catch (juiz::TransformNotFoundError&) {
        return juiz::Value("TransformNotFoundError");
    }
}
COMPONENT_CONTAINER_PROCESS_FACTORY(CppPart, cpp_part_set_transform_manif, cpp_part_set_transform);

// readonly の中から Transform を書こうとする (#410)。juiz::set_transform は const の Container では static_assert で
// 止まるので、libjuiz_sdk の C の関数を直接呼ぶ。juiz が渡す口が JUIZ_TRANSFORM_NOT_WRITABLE_ERROR を返す
static auto cpp_part_try_readonly_transform_manif = ProcessManifest("cpp_part_try_readonly_transform")
    .readonly()
    .add_string_arg("source", "始点", "map");
std::optional<std::string> cpp_part_try_readonly_transform(const CppPart* part, const std::string& source) {
    capsule_map* cm = juiz::detail::require_pose_call(part);
    juiz_transform t = juiz::make_transform(1.0, 0.0, 0.0);
    juiz_transform_stamped out{};
    int64_t r = juiz_container_transform_set(cm, source.c_str(), "base0", &t, &out);
    if (r == JUIZ_TRANSFORM_NOT_WRITABLE_ERROR) {
        return std::string("TransformNotWritableError");
    }
    return std::string("returned ") + std::to_string(r);
}
COMPONENT_CONTAINER_PROCESS_FACTORY(CppPart, cpp_part_try_readonly_transform_manif, cpp_part_try_readonly_transform);

// 四元数と角度の変換 (#412)。roll・pitch・yaw から作った四元数、それを戻した roll・pitch・yaw、
// make_pose_2d(x, y, yaw) とその yaw を返す (Container は要らない)
static auto cpp_pose_angles_manif = ProcessManifest("cpp_pose_angles")
    .add_float_arg("roll", "", 0.0).add_float_arg("pitch", "", 0.0).add_float_arg("yaw", "", 0.0)
    .add_float_arg("x", "", 0.0).add_float_arg("y", "", 0.0);
std::optional<juiz::Value> cpp_pose_angles(const double roll, const double pitch, const double yaw,
                                           const double x, const double y) {
    juiz_quaternion q = juiz::from_rpy(roll, pitch, yaw);
    juiz::Rpy rpy = juiz::to_rpy(q);
    juiz_pose p2 = juiz::make_pose_2d(x, y, yaw);
    return juiz::Value{
        {"quaternion", juiz::Value{{"x", q.x}, {"y", q.y}, {"z", q.z}, {"w", q.w}}},
        {"rpy", juiz::Value{{"roll", rpy.roll}, {"pitch", rpy.pitch}, {"yaw", rpy.yaw}}},
        {"pose_2d", juiz::pose_to_value(p2)},
        {"yaw_2d", juiz::yaw(p2)},
        {"rpy_of_pose", juiz::Value{{"roll", juiz::to_rpy(juiz::make_pose(0, 0, 0, q)).roll}}},
    };
}
COMPONENT_PROCESS_FACTORY(cpp_pose_angles_manif, cpp_pose_angles);

auto component_manifest() {
    return ComponentManifest("cpp_pose_component")
      .add_process(cpp_pose_angles_manifest())
      .add_container(cpp_part_manifest()
        .add_process(cpp_part_pose_manifest())
        .add_process(cpp_part_set_pose_manifest())
        .add_process(cpp_part_try_manifest())
        .add_process(cpp_part_pose_unchecked_manifest())
        .add_process(cpp_part_peek_manifest())
        .add_process(cpp_part_sleep_manifest())
        .add_process(cpp_part_try_readonly_manifest())
        .add_process(cpp_part_lookup_manifest())
        .add_process(cpp_part_set_transform_manifest())
        .add_process(cpp_part_try_readonly_transform_manifest())
      );
}
DEFINE_COMPONENT_MANIFEST_ENTRY_POINT(component_manifest)
