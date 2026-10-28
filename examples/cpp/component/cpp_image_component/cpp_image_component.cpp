// 画像 (juiz::Image) の引数と戻り値の例とテスト用の Component (issue #253、docs/spec/large-data.md §10)。
// テストは juiz_core/tests/integration/cpp_image_test.rs。
#include <string>
#include "juiz/juiz.h"

static std::string format_name(juiz::PixelFormat f) {
    switch (f) {
    case juiz::PixelFormat::L8: return "l8";
    case juiz::PixelFormat::LA8: return "la8";
    case juiz::PixelFormat::RGB8: return "rgb8";
    case juiz::PixelFormat::RGBA8: return "rgba8";
    case juiz::PixelFormat::L16: return "l16";
    case juiz::PixelFormat::LA16: return "la16";
    case juiz::PixelFormat::RGB16: return "rgb16";
    case juiz::PixelFormat::RGBA16: return "rgba16";
    case juiz::PixelFormat::RGB32F: return "rgb32f";
    case juiz::PixelFormat::RGBA32F: return "rgba32f";
    case juiz::PixelFormat::BGR8: return "bgr8";
    case juiz::PixelFormat::BGRA8: return "bgra8";
    }
    return "?";
}

// 8 bit の形式は画素を反転した画像、それ以外は同じ画像を返す
static auto image_invert_cpp_manif = ProcessManifest("image_invert_cpp").add_image_arg("img", "image to invert");
std::optional<juiz::Image> image_invert_cpp(const juiz::Image& img) {
    juiz::Image out = img;
    switch (img.format()) {
    case juiz::PixelFormat::L8: case juiz::PixelFormat::LA8:
    case juiz::PixelFormat::RGB8: case juiz::PixelFormat::RGBA8:
        for (size_t i = 0; i < out.size(); ++i) out.data()[i] = 255 - out.data()[i];
        break;
    default:
        break;
    }
    return out;
}
COMPONENT_PROCESS_FACTORY(image_invert_cpp_manif, image_invert_cpp);

// 画素 (x, y) が B = x, G = y, R = 7 の BGR8 の画像を返す (juiz には RGB8 で入る)
static auto image_bgr_cpp_manif = ProcessManifest("image_bgr_cpp")
    .add_int_arg("width", "", 4)
    .add_int_arg("height", "", 3);
std::optional<juiz::Image> image_bgr_cpp(int64_t width, int64_t height) {
    juiz::Image img(static_cast<uint32_t>(width), static_cast<uint32_t>(height), juiz::PixelFormat::BGR8);
    for (uint32_t y = 0; y < img.height(); ++y) {
        uint8_t* row = img.row(y);
        for (uint32_t x = 0; x < img.width(); ++x) {
            row[x * 3 + 0] = static_cast<uint8_t>(x);
            row[x * 3 + 1] = static_cast<uint8_t>(y);
            row[x * 3 + 2] = 7;
        }
    }
    return img;
}
COMPONENT_PROCESS_FACTORY(image_bgr_cpp_manif, image_bgr_cpp);

// 画像の大きさと形式と、画像の後ろの引数を文字列にする (画像の引数の後に引数が続く場合)
static auto image_info_cpp_manif = ProcessManifest("image_info_cpp")
    .add_image_arg("img", "")
    .add_string_arg("label", "", "");
std::optional<std::string> image_info_cpp(const juiz::Image& img, const std::string& label) {
    return label + ":" + std::to_string(img.width()) + "x" + std::to_string(img.height()) + ":" + format_name(img.format());
}
COMPONENT_PROCESS_FACTORY(image_info_cpp_manif, image_info_cpp);

// 最後に受け取った画像を持つ Container
class ImageStore {
public:
    juiz::Image last;
    int64_t count = 0;
};

static auto image_store_cpp_manif = ContainerManifest("image_store_cpp");
ImageStore* image_store_cpp(juiz::CapsuleMap) {
    return new ImageStore();
}
COMPONENT_CONTAINER_FACTORY(image_store_cpp_manif, image_store_cpp);

static auto image_store_cpp_put_manif = ProcessManifest("image_store_cpp_put").add_image_arg("img", "");
std::optional<int64_t> image_store_cpp_put(ImageStore* store, juiz::Image img) {
    store->last = std::move(img);
    return ++store->count;
}
COMPONENT_CONTAINER_PROCESS_FACTORY(ImageStore, image_store_cpp_put_manif, image_store_cpp_put);

static auto image_store_cpp_get_manif = ProcessManifest("image_store_cpp_get");
std::optional<juiz::Image> image_store_cpp_get(ImageStore* store) {
    if (store->last.empty()) return std::nullopt;
    return store->last;
}
COMPONENT_CONTAINER_PROCESS_FACTORY(ImageStore, image_store_cpp_get_manif, image_store_cpp_get);

auto component_manifest() {
    return ComponentManifest("cpp_image_component")
        .add_process(image_invert_cpp_manifest())
        .add_process(image_bgr_cpp_manifest())
        .add_process(image_info_cpp_manifest())
        .add_container(image_store_cpp_manifest()
            .add_process(image_store_cpp_put_manifest())
            .add_process(image_store_cpp_get_manifest()));
}
DEFINE_COMPONENT_MANIFEST_ENTRY_POINT(component_manifest)
