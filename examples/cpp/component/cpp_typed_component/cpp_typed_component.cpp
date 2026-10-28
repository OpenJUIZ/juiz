// 型付きの引数・戻り値とネイティブ値 (遅延評価 L3) のテスト用コンポーネント
// (juiz_core/tests/integration/cpp_typed_test.rs、issue #195・#177、docs/spec/datatype.md §21)
//
// 型は juiz-idl -g cpp が juiz_idl/test_package/idl/geometry.jidl から作ったもの (juiz_idl.h) と、
// 手で書いて JUIZ_TYPE で登録したもの (odometry.h) を使う。

#include "juiz/juiz.h"
#include "juiz_idl.h"
#include "odometry.h"

using robot::geometry::Everything;
using robot::geometry::MotorMode;
using robot::geometry::Point2D;
using robot::geometry::Pose2D;

static auto cpp_pose_make_manif = ProcessManifest("cpp_pose_make")
    .add_float_arg("x", "", 0.0)
    .returns<Pose2D>();
std::optional<Pose2D> cpp_pose_make(double x) {
    Pose2D pose;
    pose.position = Point2D{x, 0.0};
    return pose;
}
COMPONENT_PROCESS_FACTORY(cpp_pose_make_manif, cpp_pose_make);

static auto cpp_pose_move_manif = ProcessManifest("cpp_pose_move")
    .add_typed_arg<Pose2D>("pose", "", Pose2D{})
    .add_float_arg("dx", "", 1.0)
    .returns<Pose2D>();
std::optional<Pose2D> cpp_pose_move(const Pose2D& pose, double dx) {
    Pose2D moved = pose;
    moved.position.x += dx;
    return moved;
}
COMPONENT_PROCESS_FACTORY(cpp_pose_move_manif, cpp_pose_move);

static auto cpp_pose_x_manif = ProcessManifest("cpp_pose_x")
    .add_typed_arg<Pose2D>("pose", "")
    .add_typed_arg<Pose2D>("offset", "", Pose2D{});
std::optional<double> cpp_pose_x(Pose2D pose, const Pose2D& offset) {
    return pose.position.x + offset.position.x;
}
COMPONENT_PROCESS_FACTORY(cpp_pose_x_manif, cpp_pose_x);

// Pose2D の frame と comment (既定値のあるフィールド) を文字列で返す
static auto cpp_pose_frame_manif = ProcessManifest("cpp_pose_frame")
    .add_typed_arg<Pose2D>("pose", "");
std::optional<std::string> cpp_pose_frame(const Pose2D& pose) {
    return pose.frame + "/" + pose.comment.value_or("none");
}
COMPONENT_PROCESS_FACTORY(cpp_pose_frame_manif, cpp_pose_frame);

// いろいろな型のフィールドを C++ の値にして、少し変えて返す
static auto cpp_everything_echo_manif = ProcessManifest("cpp_everything_echo")
    .add_typed_arg<Everything>("value", "")
    .returns<Everything>();
std::optional<Everything> cpp_everything_echo(const Everything& value) {
    Everything e = value;
    e.i += 1;
    e.l += 1;
    e.data.push_back(uint8_t(255));
    e.list.push_back(int64_t(e.list.size()));
    e.table["b"] = 2.5;
    e.maybe = std::nullopt;
    e.mode = MotorMode::Position;
    e.header.frame_id += "!";
    e.type += "?";
    return e;
}
COMPONENT_PROCESS_FACTORY(cpp_everything_echo_manif, cpp_everything_echo);

// 列挙の引数と戻り値 (辞書の記号の文字列で渡る)
static auto cpp_mode_next_manif = ProcessManifest("cpp_mode_next")
    .add_typed_arg<MotorMode>("mode", "", MotorMode::Idle)
    .returns<MotorMode>();
std::optional<MotorMode> cpp_mode_next(MotorMode mode) {
    switch (mode) {
    case MotorMode::Idle: return MotorMode::Velocity;
    case MotorMode::Velocity: return MotorMode::Position;
    default: return MotorMode::Idle;
    }
}
COMPONENT_PROCESS_FACTORY(cpp_mode_next_manif, cpp_mode_next);

static auto cpp_odometry_make_manif = ProcessManifest("cpp_odometry_make")
    .add_float_arg("distance", "", 0.0)
    .returns<counted::Odometry>();
std::optional<counted::Odometry> cpp_odometry_make(double distance) {
    return counted::Odometry{distance, 0};
}
COMPONENT_PROCESS_FACTORY(cpp_odometry_make_manif, cpp_odometry_make);

static auto cpp_odometry_step_manif = ProcessManifest("cpp_odometry_step")
    .add_typed_arg<counted::Odometry>("odometry", "")
    .returns<counted::Odometry>();
std::optional<counted::Odometry> cpp_odometry_step(const counted::Odometry& odometry) {
    return counted::Odometry{odometry.distance + 1.0, odometry.steps + 1};
}
COMPONENT_PROCESS_FACTORY(cpp_odometry_step_manif, cpp_odometry_step);

static auto cpp_odometry_stats_manif = ProcessManifest("cpp_odometry_stats");
std::optional<juiz::Value> cpp_odometry_stats() {
    return counted::take_stats();
}
COMPONENT_PROCESS_FACTORY(cpp_odometry_stats_manif, cpp_odometry_stats);

// 型付きの引数を持つ ContainerProcess。受け取った Pose2D を覚え、x を scale 倍して返す
struct PoseHolder {
    Pose2D last;
};

static auto cpp_pose_holder_manif = ContainerManifest("cpp_pose_holder");
PoseHolder* cpp_pose_holder(juiz::CapsuleMap) {
    return new PoseHolder();
}
COMPONENT_CONTAINER_FACTORY(cpp_pose_holder_manif, cpp_pose_holder);

static auto cpp_pose_holder_scale_manif = ProcessManifest("cpp_pose_holder_scale")
    .container_type("cpp_pose_holder")
    .add_typed_arg<Pose2D>("pose", "")
    .add_float_arg("scale", "", 2.0)
    .returns<Pose2D>();
std::optional<Pose2D> cpp_pose_holder_scale(PoseHolder* holder, const Pose2D& pose, double scale) {
    holder->last = pose;
    Pose2D scaled = pose;
    scaled.position.x *= scale;
    return scaled;
}
COMPONENT_CONTAINER_PROCESS_FACTORY(PoseHolder, cpp_pose_holder_scale_manif, cpp_pose_holder_scale);

auto component_manifest() {
    return ComponentManifest("cpp_typed_component")
        .add_process(cpp_pose_make_manifest())
        .add_process(cpp_pose_move_manifest())
        .add_process(cpp_pose_x_manifest())
        .add_process(cpp_pose_frame_manifest())
        .add_process(cpp_everything_echo_manifest())
        .add_process(cpp_mode_next_manifest())
        .add_process(cpp_odometry_make_manifest())
        .add_process(cpp_odometry_step_manifest())
        .add_process(cpp_odometry_stats_manifest())
        .add_container(cpp_pose_holder_manifest()
            .add_process(cpp_pose_holder_scale_manifest()));
}
DEFINE_COMPONENT_MANIFEST_ENTRY_POINT(component_manifest)
