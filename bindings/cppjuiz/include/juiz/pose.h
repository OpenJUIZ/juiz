#pragma once

// ContainerProcess の中から、呼び出し元 Container の位置姿勢 (ContainerPose) を読み書きする
// (docs/spec/transformations.md §12.3)。
//
//   std::optional<juiz::Value> update(MobileBase* base, double dx) {
//       juiz_pose_stamped p = juiz::pose(base, "map");       // map から見た base の位置姿勢
//       p.pose.position.x += dx;
//       juiz::set_pose(base, "map", p.pose);                 // 親 → base の Transform を書き換える
//       return juiz::pose_to_value(p);
//   }
//
// 値は C の構造体 juiz_pose / juiz_pose_stamped (juiz_sdk.h)。四元数の並びは (x, y, z, w)。
// 読み書きできるのは、その Container の ContainerProcess が呼ばれている間だけ。それ以外 (別スレッド、
// 呼び出しの後) では TransformationsNotAttachedError を投げる。
// エラーは TransformationsError の子の例外で投げ、CONTAINER_PROCESS_FACTORY などの entry point が
// コード (JUIZ_*_ERROR) にして juiz に返す。

#include "juiz_sdk.h"
#include "value.h"

#include <cstdint>
#include <stdexcept>
#include <string>
#include <vector>

namespace juiz {

    // ContainerPose の読み書きの失敗。code() は juiz_sdk.h の JUIZ_*_ERROR (-901〜-905)
    class TransformationsError : public std::runtime_error {
    private:
        int64_t code_;
    public:
        TransformationsError(int64_t code, const std::string& message) : std::runtime_error(message), code_(code) {}
        int64_t code() const { return code_; }
    };

    class TransformationsNotAttachedError : public TransformationsError {
    public:
        explicit TransformationsNotAttachedError(const std::string& m) : TransformationsError(JUIZ_TRANSFORMATIONS_NOT_ATTACHED_ERROR, m) {}
    };

    class NoCommonAnchorError : public TransformationsError {
    public:
        explicit NoCommonAnchorError(const std::string& m) : TransformationsError(JUIZ_NO_COMMON_ANCHOR_ERROR, m) {}
    };

    class PoseNotWritableError : public TransformationsError {
    public:
        explicit PoseNotWritableError(const std::string& m) : TransformationsError(JUIZ_POSE_NOT_WRITABLE_ERROR, m) {}
    };

    class InvalidPoseValueError : public TransformationsError {
    public:
        explicit InvalidPoseValueError(const std::string& m) : TransformationsError(JUIZ_INVALID_POSE_VALUE_ERROR, m) {}
    };

    namespace detail {
        struct PoseCall {
            const void* container;
            capsule_map* cm;
        };

        // 今このスレッドで呼ばれている ContainerProcess の (Container, 引数)。入れ子に呼ばれることがある
        // (Pose 通知の受け手が同じスレッドで呼ばれる) ので積む
        inline std::vector<PoseCall>& pose_calls() {
            thread_local std::vector<PoseCall> calls;
            return calls;
        }

        // ContainerProcess の entry point が、呼び出しの間だけ (Container, 引数) を積む
        class PoseCallScope {
        public:
            PoseCallScope(const void* container, capsule_map* cm) { pose_calls().push_back(PoseCall{container, cm}); }
            ~PoseCallScope() { pose_calls().pop_back(); }
            PoseCallScope(const PoseCallScope&) = delete;
            PoseCallScope& operator=(const PoseCallScope&) = delete;
        };

        inline capsule_map* pose_call_of(const void* container) {
            auto& calls = pose_calls();
            for (auto it = calls.rbegin(); it != calls.rend(); ++it) {
                if (it->container == container) {
                    return it->cm;
                }
            }
            return nullptr;
        }

        [[noreturn]] inline void throw_transformations_error(int64_t code) {
            const char* m = juiz_container_pose_last_error();
            std::string message = (m && *m) ? std::string(m) : std::string("ContainerPose error (") + std::to_string(code) + ")";
            switch (code) {
            case JUIZ_TRANSFORMATIONS_NOT_ATTACHED_ERROR: throw TransformationsNotAttachedError(message);
            case JUIZ_NO_COMMON_ANCHOR_ERROR: throw NoCommonAnchorError(message);
            case JUIZ_POSE_NOT_WRITABLE_ERROR: throw PoseNotWritableError(message);
            case JUIZ_INVALID_POSE_VALUE_ERROR: throw InvalidPoseValueError(message);
            default: throw TransformationsError(code, message);
            }
        }

        inline capsule_map* require_pose_call(const void* container) {
            capsule_map* cm = pose_call_of(container);
            if (!cm) {
                throw TransformationsNotAttachedError(
                    "juiz::pose / juiz::set_pose can be used only while a container process of the container is running "
                    "(on the thread that juiz called it)");
            }
            return cm;
        }
    }

    // 位置 (x, y, z) と四元数 (qx, qy, qz, qw) から juiz_pose を作る
    inline juiz_pose make_pose(double x, double y, double z, double qx = 0, double qy = 0, double qz = 0, double qw = 1) {
        juiz_pose p;
        p.position.x = x; p.position.y = y; p.position.z = z;
        p.orientation.x = qx; p.orientation.y = qy; p.orientation.z = qz; p.orientation.w = qw;
        return p;
    }

    // 基準 reference (Container の名前か ID) から見た container の ContainerPose (§7.1)
    template<typename U>
    juiz_pose_stamped pose(const U* container, const std::string& reference) {
        capsule_map* cm = detail::require_pose_call(static_cast<const void*>(container));
        juiz_pose_stamped out{};
        int64_t r = juiz_container_pose_get(cm, reference.c_str(), &out);
        if (r != JUIZ_OK) {
            detail::throw_transformations_error(r);
        }
        return out;
    }

    // container を、基準 reference から見て p の位置姿勢にする (§7.2)。書いた後の値を返す
    template<typename U>
    juiz_pose_stamped set_pose(U* container, const std::string& reference, const juiz_pose& p) {
        capsule_map* cm = detail::require_pose_call(static_cast<const void*>(container));
        juiz_pose_stamped out{};
        int64_t r = juiz_container_pose_set(cm, reference.c_str(), &p, &out);
        if (r != JUIZ_OK) {
            detail::throw_transformations_error(r);
        }
        return out;
    }

    // juiz_std.geometry.Pose の辞書にする
    inline juiz::Value pose_to_value(const juiz_pose& p) {
        return juiz::Value{
            {"position", juiz::Value{{"x", p.position.x}, {"y", p.position.y}, {"z", p.position.z}}},
            {"orientation", juiz::Value{{"x", p.orientation.x}, {"y", p.orientation.y}, {"z", p.orientation.z}, {"w", p.orientation.w}}},
        };
    }

    // juiz_std.geometry.PoseStamped の辞書にする (header.frame_id は reference)
    inline juiz::Value pose_to_value(const juiz_pose_stamped& p, const std::string& reference) {
        return juiz::Value{
            {"header", juiz::Value{
                {"stamp", juiz::Value{{"sec", p.stamp.sec}, {"nanosec", (int64_t)p.stamp.nanosec}}},
                {"frame_id", reference},
            }},
            {"pose", pose_to_value(p.pose)},
        };
    }
}
