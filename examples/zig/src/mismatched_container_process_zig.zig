//! 状態を example_container_zig と違う型 (f64 を持つ struct) で受け取る ContainerProcess
//! (mismatched_container_process_zig、issue #574)。型の違う Container や別の言語の Container に結びつけても、
//! juiz が落ちずにエラー (-1104) になることを確かめる、juiz のテスト用。
const juiz = @import("juiz");

const OtherState = struct { value: f64 };

fn getDouble(self: *const OtherState) f64 {
    return self.value;
}

comptime {
    juiz.exportContainerProcess(getDouble, .{
        .type_name = "mismatched_container_process_zig",
        .container_type = "example_container_zig",
        .description = "Expects a container whose state is OtherState (for tests).",
    });
}
