// cpp_typed_component と同じ型を、ABI タグを変えて (CMakeLists.txt の JUIZ_CPP_ABI_TAG_EXTRA) 受け取る
// テスト用コンポーネント (juiz_core/tests/integration/cpp_typed_test.rs)。ABI タグが違うので、値は辞書を通して届く。

#include "juiz/juiz.h"
#include "juiz_idl.h"
#include "../cpp_typed_component/odometry.h"

using robot::geometry::Pose2D;

static auto cpp_odometry_steps_other_manif = ProcessManifest("cpp_odometry_steps_other")
    .add_typed_arg<counted::Odometry>("odometry", "");
std::optional<int64_t> cpp_odometry_steps_other(const counted::Odometry& odometry) {
    return odometry.steps;
}
COMPONENT_PROCESS_FACTORY(cpp_odometry_steps_other_manif, cpp_odometry_steps_other);

static auto cpp_pose_x_other_manif = ProcessManifest("cpp_pose_x_other")
    .add_typed_arg<Pose2D>("pose", "");
std::optional<double> cpp_pose_x_other(const Pose2D& pose) {
    return pose.position.x;
}
COMPONENT_PROCESS_FACTORY(cpp_pose_x_other_manif, cpp_pose_x_other);

static auto cpp_odometry_stats_other_manif = ProcessManifest("cpp_odometry_stats_other");
std::optional<juiz::Value> cpp_odometry_stats_other() {
    return counted::take_stats();
}
COMPONENT_PROCESS_FACTORY(cpp_odometry_stats_other_manif, cpp_odometry_stats_other);

auto component_manifest() {
    return ComponentManifest("cpp_typed_component_other_abi")
        .add_process(cpp_odometry_steps_other_manifest())
        .add_process(cpp_pose_x_other_manifest())
        .add_process(cpp_odometry_stats_other_manifest());
}
DEFINE_COMPONENT_MANIFEST_ENTRY_POINT(component_manifest)
