// juiz/opencv.h で juiz::Image と cv::Mat を変換する例とテスト用の Component (issue #253)。
// テストは juiz_core/tests/integration/plugin_image_test.rs (OpenCV が無くてビルドされていなければ飛ばす)。
#include "juiz/juiz.h"
#include "juiz/opencv.h"

// 左右を反転する。to_mat で BGR の Mat にし、from_mat で BGR8 の Image に戻す (juiz には RGB8 で入る)
static auto image_flip_opencv_cpp_manif = ProcessManifest("image_flip_opencv_cpp").add_image_arg("img", "");
std::optional<juiz::Image> image_flip_opencv_cpp(const juiz::Image& img) {
    cv::Mat m = juiz::to_mat(img);
    cv::flip(m, m, 1);
    return juiz::from_mat(m);
}
COMPONENT_PROCESS_FACTORY(image_flip_opencv_cpp_manif, image_flip_opencv_cpp);

// 周りを 1 画素ずつ切り取る。ROI の Mat は行の終わりに余白がある (step > 幅 × 画素のバイト数)
static auto image_crop_opencv_cpp_manif = ProcessManifest("image_crop_opencv_cpp").add_image_arg("img", "");
std::optional<juiz::Image> image_crop_opencv_cpp(const juiz::Image& img) {
    cv::Mat m = juiz::to_mat(img);
    return juiz::from_mat(m(cv::Rect(1, 1, m.cols - 2, m.rows - 2)));
}
COMPONENT_PROCESS_FACTORY(image_crop_opencv_cpp_manif, image_crop_opencv_cpp);

auto component_manifest() {
    return ComponentManifest("cpp_image_opencv_component")
        .add_process(image_flip_opencv_cpp_manifest())
        .add_process(image_crop_opencv_cpp_manifest());
}
DEFINE_COMPONENT_MANIFEST_ENTRY_POINT(component_manifest)
