//! 画像の引数・戻り値の例とテスト用 Component (image_component_zig、issue #576、docs/spec/large-data.md §10.5)。
//! C++ の cpp_image_component・Haskell の image_component_hs と同じ構成 (テストは juiz_core/tests/integration/plugins/image.rs)。
const std = @import("std");
const juiz = @import("juiz");

/// 8 bit の形式は画素を反転した画像、それ以外は同じ画像を返す
fn invert(ctx: juiz.Context, args: struct { img: juiz.Image }) !juiz.Image {
    const img = args.img;
    switch (img.format) {
        .l8, .la8, .rgb8, .rgba8 => {
            // 受け取った画像は呼び出しの arena にある。書き換えるので複製する
            const buf = try ctx.allocator.dupe(u8, img.data);
            for (buf) |*b| b.* = 255 - b.*;
            return juiz.Image.init(img.width, img.height, img.format, buf);
        },
        else => return img,
    }
}

/// 画素 (x, y) が B = x, G = y, R = 7 の BGR8 の画像を返す (juiz には RGB8 で入る)
fn bgr(ctx: juiz.Context, args: struct { width: i64 = 4, height: i64 = 3 }) !juiz.Image {
    const w: u32 = @intCast(args.width);
    const h: u32 = @intCast(args.height);
    const buf = try ctx.allocator.alloc(u8, @as(usize, w) * h * 3);
    var y: u32 = 0;
    while (y < h) : (y += 1) {
        var x: u32 = 0;
        while (x < w) : (x += 1) {
            const i = (@as(usize, y) * w + x) * 3;
            buf[i] = @truncate(x);
            buf[i + 1] = @truncate(y);
            buf[i + 2] = 7;
        }
    }
    return juiz.Image.init(w, h, .bgr8, buf);
}

/// 画像の大きさと形式と、画像の後ろの引数を文字列にする
fn info(ctx: juiz.Context, args: struct { img: juiz.Image, label: []const u8 = "" }) ![]const u8 {
    return std.fmt.allocPrint(ctx.allocator, "{s}:{d}x{d}:{s}", .{ args.label, args.img.width, args.img.height, @tagName(args.img.format) });
}

/// 最後に受け取った画像を持つ Container。画像は呼び出しの arena のものなので、c_allocator に複製して持つ
const Store = struct {
    count: i64 = 0,
    image: ?juiz.Image = null,

    pub fn deinit(self: *Store) void {
        if (self.image) |img| std.heap.c_allocator.free(img.data);
    }
};

fn createStore() Store {
    return .{};
}

fn storePut(self: *Store, args: struct { img: juiz.Image }) !i64 {
    const copy = try args.img.clone(std.heap.c_allocator);
    if (self.image) |old| std.heap.c_allocator.free(old.data);
    self.image = copy;
    self.count += 1;
    return self.count;
}

fn storeGet(self: *const Store) !juiz.Image {
    return self.image orelse error.NoImageYet;
}

comptime {
    juiz.exportComponent(.{
        .type_name = "image_component_zig",
        .description = "Zig image component (test).",
        .parts = .{
            juiz.processPart(invert, .{ .type_name = "image_invert_zig", .args = .{ .img = "image to invert" } }),
            juiz.processPart(bgr, .{ .type_name = "image_bgr_zig", .args = .{ .width = "", .height = "" } }),
            juiz.processPart(info, .{ .type_name = "image_info_zig", .args = .{ .img = "", .label = "" } }),
            juiz.containerPart(createStore, .{ .type_name = "image_store_zig" }),
            juiz.containerProcessPart(storePut, .{ .type_name = "image_store_zig_put", .container_type = "image_store_zig", .args = .{ .img = "" } }),
            juiz.containerProcessPart(storeGet, .{ .type_name = "image_store_zig_get", .container_type = "image_store_zig" }),
        },
    });
}
