//! ContainerPose を持つ部品の Container (zig_part、issue #574)。状態は zig_part_state.Part
const juiz = @import("juiz");
const state = @import("zig_part_state.zig");

fn create() state.Part {
    return .{};
}

comptime {
    juiz.exportContainer(create, .{
        .type_name = "zig_part",
        .description = "A part with ContainerPose (Zig).",
    });
}
