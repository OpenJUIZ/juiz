//! readonly でない ContainerProcess (zig_part_sleep、issue #574)。sleep_ms だけ待って数を 1 増やす (並行に走らないことの比較用)
const juiz = @import("juiz");
const state = @import("zig_part_state.zig");

fn sleep(self: *state.Part, args: struct { sleep_ms: u32 = 0 }) i64 {
    state.sleepMs(args.sleep_ms);
    self.count += 1;
    return self.count;
}

comptime {
    juiz.exportContainerProcess(sleep, .{
        .type_name = "zig_part_sleep",
        .container_type = "zig_part",
        .args = .{ .sleep_ms = "milliseconds to wait" },
    });
}
