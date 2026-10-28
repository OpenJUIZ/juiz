#pragma once

// 画像の引数と戻り値 (issue #253、docs/spec/large-data.md §10)。
//
// juiz::Image は画素を持つ画像。Process の引数に `juiz::Image` か `const juiz::Image&` で書くと、
// 届いた画像 (Rust の Capsule::Image) を 1 度だけ複製して受け取る。戻り値を `std::optional<juiz::Image>` にすると、
// juiz の画像に複製して返す。IPC や qmp のブローカー越しには、生バイトのフレームで渡る (large-data.md §3.4)。
// 複製せずに読みたいときは juiz::with_image で、callback の間だけ借りる。
//
// 形式は large-data.md §3.4 の表と同じ 10 種と、送るときだけ使える BGR8 / BGRA8 (juiz はチャンネルを並べ替えて
// RGB8 / RGBA8 で持つ。OpenCV の cv::Mat から送るため)。16 bit・f32 の形式の画素はホストのバイト順。
// OpenCV との変換は juiz/opencv.h。

#include <cstdint>
#include <cstring>
#include <functional>
#include <memory>
#include <string>
#include <vector>

#include "core.h"
#include "capsule_map.h"

namespace juiz {

    enum class PixelFormat : uint32_t {
        L8 = JUIZ_IMAGE_FORMAT_L8,
        LA8 = JUIZ_IMAGE_FORMAT_LA8,
        RGB8 = JUIZ_IMAGE_FORMAT_RGB8,
        RGBA8 = JUIZ_IMAGE_FORMAT_RGBA8,
        L16 = JUIZ_IMAGE_FORMAT_L16,
        LA16 = JUIZ_IMAGE_FORMAT_LA16,
        RGB16 = JUIZ_IMAGE_FORMAT_RGB16,
        RGBA16 = JUIZ_IMAGE_FORMAT_RGBA16,
        RGB32F = JUIZ_IMAGE_FORMAT_RGB32F,
        RGBA32F = JUIZ_IMAGE_FORMAT_RGBA32F,
        // 送るときだけ使える
        BGR8 = JUIZ_IMAGE_FORMAT_BGR8,
        BGRA8 = JUIZ_IMAGE_FORMAT_BGRA8,
    };

    // 画像の形式・大きさが正しくない。Process の関数から投げると JUIZ_IMAGE_FORMAT_ERROR を返す
    class ImageFormatError : public std::exception {
    public:
        explicit ImageFormatError(const std::string& message) : message_(message) {}
        const char* what() const noexcept override { return message_.c_str(); }
    private:
        std::string message_;
    };

    // 1 画素のバイト数
    inline size_t bytes_per_pixel(PixelFormat f) {
        switch (f) {
        case PixelFormat::L8: return 1;
        case PixelFormat::LA8: case PixelFormat::L16: return 2;
        case PixelFormat::RGB8: case PixelFormat::BGR8: return 3;
        case PixelFormat::RGBA8: case PixelFormat::BGRA8: case PixelFormat::LA16: return 4;
        case PixelFormat::RGB16: return 6;
        case PixelFormat::RGBA16: return 8;
        case PixelFormat::RGB32F: return 12;
        case PixelFormat::RGBA32F: return 16;
        }
        throw ImageFormatError("unknown pixel format " + std::to_string(static_cast<uint32_t>(f)));
    }

    // 借りた画像 (with_image の callback の間だけ有効)
    struct ImageView {
        uint32_t width = 0;
        uint32_t height = 0;
        PixelFormat format = PixelFormat::RGB8;
        size_t stride = 0;              // 1 行のバイト数
        const uint8_t* data = nullptr;  // 先頭の行

        size_t row_bytes() const { return width * bytes_per_pixel(format); }
        const uint8_t* row(uint32_t y) const { return data + stride * y; }
    };

    // 画素を持つ画像。行の余白は持たない (stride == width × 1 画素のバイト数)
    class Image {
    public:
        Image() {}

        // 0 で埋めた画像
        Image(uint32_t width, uint32_t height, PixelFormat format)
            : width_(width), height_(height), format_(format),
              data_(static_cast<size_t>(width) * height * juiz::bytes_per_pixel(format)) {}

        // data から複製する。stride は 1 行のバイト数 (0 なら余白なし)。行の余白は取り除く
        Image(uint32_t width, uint32_t height, PixelFormat format, const void* data, size_t stride = 0)
            : Image(width, height, format) {
            const size_t rb = row_bytes();
            if (stride == 0) stride = rb;
            if (stride < rb) {
                throw ImageFormatError("stride (" + std::to_string(stride) + ") must be width x bytes per pixel ("
                    + std::to_string(rb) + ") or more");
            }
            const uint8_t* src = static_cast<const uint8_t*>(data);
            for (uint32_t y = 0; y < height_; ++y) {
                std::memcpy(data_.data() + rb * y, src + stride * y, rb);
            }
        }

        explicit Image(const ImageView& v) : Image(v.width, v.height, v.format, v.data, v.stride) {}

        uint32_t width() const { return width_; }
        uint32_t height() const { return height_; }
        PixelFormat format() const { return format_; }
        size_t bytes_per_pixel() const { return juiz::bytes_per_pixel(format_); }
        size_t row_bytes() const { return width_ * bytes_per_pixel(); }
        size_t stride() const { return row_bytes(); }
        size_t size() const { return data_.size(); }
        bool empty() const { return data_.empty(); }

        const uint8_t* data() const { return data_.data(); }
        uint8_t* data() { return data_.data(); }
        const uint8_t* row(uint32_t y) const { return data_.data() + row_bytes() * y; }
        uint8_t* row(uint32_t y) { return data_.data() + row_bytes() * y; }

        ImageView view() const { return ImageView{width_, height_, format_, row_bytes(), data_.data()}; }

        // RGB8 ⇔ BGR8、RGBA8 ⇔ BGRA8 (赤と青を入れ替えた複製)。ほかの形式は ImageFormatError
        Image swap_red_blue() const {
            PixelFormat to;
            switch (format_) {
            case PixelFormat::RGB8: to = PixelFormat::BGR8; break;
            case PixelFormat::BGR8: to = PixelFormat::RGB8; break;
            case PixelFormat::RGBA8: to = PixelFormat::BGRA8; break;
            case PixelFormat::BGRA8: to = PixelFormat::RGBA8; break;
            default: throw ImageFormatError("swap_red_blue() needs an 8-bit RGB / BGR image");
            }
            Image out(*this);
            out.format_ = to;
            const size_t n = bytes_per_pixel();
            for (size_t i = 0; i + n <= out.data_.size(); i += n) {
                std::swap(out.data_[i], out.data_[i + 2]);
            }
            return out;
        }

    private:
        uint32_t width_ = 0;
        uint32_t height_ = 0;
        PixelFormat format_ = PixelFormat::RGB8;
        std::vector<uint8_t> data_;
    };

    namespace detail {
        inline int64_t image_view_callback(void* arg, const juiz_image* img) {
            auto f = static_cast<std::function<void(const ImageView&)>*>(arg);
            ImageView v{img->width, img->height, static_cast<PixelFormat>(img->format), img->stride, img->data};
            try {
                (*f)(v);
            } catch (...) {
                // C の呼び出し元に例外を出さない。with_image が投げ直す
                return JUIZ_VALUE_CONVERTER_ERROR;
            }
            return JUIZ_OK;
        }
    }

    // capsule_ptr の画像を、複製せずに f の間だけ借りる (その間は juiz の値がロックされる)
    inline void with_image(capsule_ptr* ptr, const std::function<void(const ImageView&)>& f) {
        std::exception_ptr failure;
        std::function<void(const ImageView&)> guarded = [&](const ImageView& v) {
            try {
                f(v);
            } catch (...) {
                failure = std::current_exception();
                throw;
            }
        };
        const int64_t r = capsule_ptr_lock_as_image(ptr, detail::image_view_callback, &guarded);
        if (failure) std::rethrow_exception(failure);
        if (r == JUIZ_IMAGE_FORMAT_ERROR) throw ImageFormatError("unsupported image format");
        if (r != JUIZ_OK) throw ValueConvertError();
    }

    // 引数 name の画像を、複製せずに f の間だけ借りる。引数が無ければ ValueNotFoundError、画像でなければ ValueConvertError
    inline void with_image(const CapsuleMap& cm, const std::string& name, const std::function<void(const ImageView&)>& f) {
        capsule_ptr* ptr = nullptr;
        if (capsule_map_get_capsule(cm.handle(), name.c_str(), &ptr) != JUIZ_OK) {
            throw ValueNotFoundError();
        }
        with_image(ptr, f);
    }

    // 引数 name の画像を複製して受け取る
    inline Image get_image(const CapsuleMap& cm, const std::string& name) {
        Image out;
        with_image(cm, name, [&](const ImageView& v) { out = Image(v); });
        return out;
    }

    // 画像を capsule (Process の戻り値) に複製して入れる
    inline int64_t capsule_set_image(capsule* cp, const ImageView& v) {
        juiz_image c{v.width, v.height, static_cast<uint32_t>(v.format), 0, v.stride, v.data};
        return ::capsule_set_image(cp, &c);
    }

    template<typename A>
    inline constexpr bool is_image_arg_v = std::is_same_v<std::remove_cv_t<std::remove_reference_t<A>>, Image>;

} // namespace juiz

// 画像の戻り値
inline int64_t serialize(capsule* cp, const juiz::Image& retval) {
    return juiz::capsule_set_image(cp, retval.view());
}
