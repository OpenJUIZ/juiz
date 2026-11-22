//! 画像 `juiz.Image` (docs/spec/large-data.md §10.5、issue #576)。
//!
//! Process の引数を `juiz.Image` にすると、届いた画像 (Capsule::Image) を呼び出しの arena に 1 度複製して受け取る。
//! 戻り値を `juiz.Image` にすると、libjuiz_sdk の capsule_set_image で複製して返す (行の余白は取り除き、
//! BGR8 / BGRA8 は RGB8 / RGBA8 に並べ替える)。画素の並びは libjuiz_sdk の juiz_image と同じ。
const std = @import("std");
const c = @import("c");

/// 画素の形式 (juiz_sdk.h の JUIZ_IMAGE_FORMAT_* と同じ値)。16 bit・f32 の画素はホストのバイト順
pub const PixelFormat = enum(u32) {
    l8 = c.JUIZ_IMAGE_FORMAT_L8,
    la8 = c.JUIZ_IMAGE_FORMAT_LA8,
    rgb8 = c.JUIZ_IMAGE_FORMAT_RGB8,
    rgba8 = c.JUIZ_IMAGE_FORMAT_RGBA8,
    l16 = c.JUIZ_IMAGE_FORMAT_L16,
    la16 = c.JUIZ_IMAGE_FORMAT_LA16,
    rgb16 = c.JUIZ_IMAGE_FORMAT_RGB16,
    rgba16 = c.JUIZ_IMAGE_FORMAT_RGBA16,
    rgb32f = c.JUIZ_IMAGE_FORMAT_RGB32F,
    rgba32f = c.JUIZ_IMAGE_FORMAT_RGBA32F,
    /// 返すときだけ使える (juiz は RGB8 で持つ)
    bgr8 = c.JUIZ_IMAGE_FORMAT_BGR8,
    /// 返すときだけ使える (juiz は RGBA8 で持つ)
    bgra8 = c.JUIZ_IMAGE_FORMAT_BGRA8,

    /// 1 画素のバイト数
    pub fn bytesPerPixel(self: PixelFormat) usize {
        return switch (self) {
            .l8 => 1,
            .la8 => 2,
            .rgb8, .bgr8 => 3,
            .rgba8, .bgra8 => 4,
            .l16 => 2,
            .la16 => 4,
            .rgb16 => 6,
            .rgba16 => 8,
            .rgb32f => 12,
            .rgba32f => 16,
        };
    }
};

pub const ImageError = error{ImageFormat};

/// 画像。data は持ち主を持たない (引数で受け取った画像は呼び出しの arena が持ち、呼び出しが終われば消える。
/// Container に取っておくときは自分のアロケータに複製する)
pub const Image = struct {
    width: u32,
    height: u32,
    format: PixelFormat,
    /// 1 行のバイト数。width × 1 画素のバイト数 以上 (行の終わりに余白があってよい)。受け取った画像は余白なし
    stride: usize,
    /// 先頭の行から。stride × (height - 1) + width × 1 画素のバイト数 以上
    data: []const u8,

    /// 余白なしの画像。data の長さが width × height × 1 画素のバイト数 と違えば error.ImageFormat
    pub fn init(width: u32, height: u32, format: PixelFormat, data: []const u8) ImageError!Image {
        const stride = @as(usize, width) * format.bytesPerPixel();
        if (data.len != stride * height) return error.ImageFormat;
        return .{ .width = width, .height = height, .format = format, .stride = stride, .data = data };
    }

    /// 画素の並びとして読むのに要るバイト数
    pub fn requiredLen(self: Image) usize {
        if (self.height == 0) return 0;
        return self.stride * (self.height - 1) + @as(usize, self.width) * self.format.bytesPerPixel();
    }

    /// 行 y の画素のバイト列 (余白を含まない)
    pub fn row(self: Image, y: u32) []const u8 {
        const start = self.stride * y;
        return self.data[start .. start + @as(usize, self.width) * self.format.bytesPerPixel()];
    }

    /// allocator に複製した画像 (余白なし)
    pub fn clone(self: Image, allocator: std.mem.Allocator) error{OutOfMemory}!Image {
        const line = @as(usize, self.width) * self.format.bytesPerPixel();
        const buf = try allocator.alloc(u8, line * self.height);
        var y: u32 = 0;
        while (y < self.height) : (y += 1) @memcpy(buf[line * y ..][0..line], self.row(y));
        return .{ .width = self.width, .height = self.height, .format = self.format, .stride = line, .data = buf };
    }
};

pub const ReadError = error{ NotImage, ImageFormat, OutOfMemory };

/// 引数の capsule_ptr の画像を allocator に複製して読む
pub fn read(allocator: std.mem.Allocator, cp: ?*c.capsule_ptr) ReadError!Image {
    if (!c.capsule_ptr_is_image(cp)) return error.NotImage;
    var ctx = LockRead{ .allocator = allocator };
    const r = c.capsule_ptr_lock_as_image(cp, LockRead.callback, &ctx);
    if (ctx.err) |e| return e;
    if (r == c.JUIZ_CAPSULE_TYPE_ERROR) return error.NotImage;
    if (r != c.JUIZ_OK) return error.ImageFormat;
    return ctx.result;
}

const LockRead = struct {
    allocator: std.mem.Allocator,
    result: Image = undefined,
    err: ?ReadError = null,

    fn callback(arg: ?*anyopaque, img: [*c]const c.juiz_image) callconv(.c) i64 {
        const self: *LockRead = @ptrCast(@alignCast(arg.?));
        const im = img.*;
        const format = std.enums.fromInt(PixelFormat, im.format) orelse {
            self.err = error.ImageFormat;
            return c.JUIZ_IMAGE_FORMAT_ERROR;
        };
        const borrowed: Image = .{
            .width = im.width,
            .height = im.height,
            .format = format,
            .stride = im.stride,
            .data = if (im.data == null) &.{} else im.data[0 .. im.stride * im.height],
        };
        self.result = borrowed.clone(self.allocator) catch |e| {
            self.err = e;
            return c.JUIZ_IMAGE_FORMAT_ERROR;
        };
        return c.JUIZ_OK;
    }
};

/// 画像を capsule (Process の戻り値) に書く (libjuiz_sdk が複製する)
pub fn write(out: ?*c.capsule, img: Image) ImageError!void {
    if (img.stride < @as(usize, img.width) * img.format.bytesPerPixel() or img.data.len < img.requiredLen()) {
        return error.ImageFormat;
    }
    const im: c.juiz_image = .{
        .width = img.width,
        .height = img.height,
        .format = @backingInt(img.format),
        .reserved = 0,
        .stride = img.stride,
        .data = img.data.ptr,
    };
    if (c.capsule_set_image(out, &im) != c.JUIZ_OK) return error.ImageFormat;
}
