//! example_container_zig の値を返す ContainerProcess (example_container_zig_get、issue #574)。
//!
//! 状態を *const で受けると readonly の ContainerProcess になる (juiz は Container を読み出しロックで呼ぶ)。
const juiz = @import("juiz");
const state = @import("example_container_state.zig");

fn get(self: *const state.Counter) i64 {
    return self.value;
}

comptime {
    juiz.exportContainerProcess(get, .{
        .type_name = "example_container_zig_get",
        .container_type = "example_container_zig",
        .description = "Returns the value of the container (Zig).",
    });
}
