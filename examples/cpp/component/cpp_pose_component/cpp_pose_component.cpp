// C++ の ContainerProcess から ContainerPose を読み書きする例 (Transformations Step 5、issue #372)。
// juiz のテスト juiz_core/tests/integration/cpp_transformations_test.rs が使う。
// 仕様は docs/spec/transformations.md §12.3。
#include <string>
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

auto component_manifest() {
    return ComponentManifest("cpp_pose_component")
      .add_container(cpp_part_manifest()
        .add_process(cpp_part_pose_manifest())
        .add_process(cpp_part_set_pose_manifest())
        .add_process(cpp_part_try_manifest())
        .add_process(cpp_part_pose_unchecked_manifest())
      );
}
DEFINE_COMPONENT_MANIFEST_ENTRY_POINT(component_manifest)
