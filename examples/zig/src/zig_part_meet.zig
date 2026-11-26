//! readonly の ContainerProcess (zig_part_meet、issue #625)。2 つずつ組にして待ち合わせる。
//! もう 1 つの呼び出しが入ってくるのを timeout_ms まで待ち、来たら true、来なければ false を返す。
//! その後 linger_ms 待ってから ContainerPose を読む (読めなければエラー)。
//! 同じ Container の readonly 同士が並行に走っていれば、組の 2 つはどちらも true を返す。
//! かかった時間で比べないので、混んだ CI のランナーでも揺れない (Rust の rust_part_meet、#534 と同じ)
const std = @import("std");
const juiz = @import("juiz");
const state = @import("zig_part_state.zig");

/// これまでにこの ContainerProcess に入った呼び出しの数
var arrived = std.atomic.Value(u32).init(0);

fn meet(self: *const state.Part, args: struct { reference: []const u8 = "map", timeout_ms: u32 = 10000, linger_ms: u32 = 0 }) !bool {
    const n = arrived.fetchAdd(1, .seq_cst) + 1;
    // 1・2 回目が 1 組目、3・4 回目が 2 組目、…
    const wanted = (n + 1) / 2 * 2;
    // 1ms ずつ待つ回数で数える (待ちすぎる分には構わない)
    var waited: u32 = 0;
    const met = while (true) : (waited += 1) {
        if (arrived.load(.seq_cst) >= wanted) break true;
        if (waited >= args.timeout_ms) break false;
        state.sleepMs(1);
    };
    state.sleepMs(args.linger_ms);
    _ = try juiz.pose(self, args.reference);
    return met;
}

comptime {
    juiz.exportContainerProcess(meet, .{
        .type_name = "zig_part_meet",
        .container_type = "zig_part",
        .args = .{
            .reference = "reference container",
            .timeout_ms = "milliseconds to wait for the other call",
            .linger_ms = "milliseconds to wait after meeting",
        },
    });
}
