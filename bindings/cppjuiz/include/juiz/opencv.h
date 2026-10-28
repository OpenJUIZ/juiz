#pragma once

// juiz::Image と OpenCV の cv::Mat の変換 (issue #253、docs/spec/large-data.md §10)。
// OpenCV の core モジュールだけを使う。このヘッダを include するときは OpenCV をリンクする。
//
// juiz の画像の色の並びは RGB、OpenCV の慣習は BGR なので、
// - to_mat は BGR の並びの Mat に複製する (3・4 チャンネルは赤と青を入れ替える)
// - from_mat は BGR の並びの Mat を受け取る。8 bit は BGR8 / BGRA8 の Image にし (返すときに libjuiz_sdk が RGB に並べ替える)、
//   16 bit・f32 はここで RGB に並べ替える
// - wrap_mat は複製せずに包む (色の並びは juiz のまま。元の Image が生きている間だけ使える)

#include <opencv2/core.hpp>

#include "image.h"

namespace juiz {

    // 形式に当たる cv::Mat の型 (CV_8UC3 など)
    inline int cv_type_of(PixelFormat f) {
        switch (f) {
        case PixelFormat::L8: return CV_8UC1;
        case PixelFormat::LA8: return CV_8UC2;
        case PixelFormat::RGB8: case PixelFormat::BGR8: return CV_8UC3;
        case PixelFormat::RGBA8: case PixelFormat::BGRA8: return CV_8UC4;
        case PixelFormat::L16: return CV_16UC1;
        case PixelFormat::LA16: return CV_16UC2;
        case PixelFormat::RGB16: return CV_16UC3;
        case PixelFormat::RGBA16: return CV_16UC4;
        case PixelFormat::RGB32F: return CV_32FC3;
        case PixelFormat::RGBA32F: return CV_32FC4;
        }
        throw ImageFormatError("unknown pixel format");
    }

    // 複製せずに包む。色の並びは juiz のまま (RGB8 の画像は RGB の並びの Mat)。v の画素が生きている間だけ使える
    inline cv::Mat wrap_mat(const ImageView& v) {
        return cv::Mat(static_cast<int>(v.height), static_cast<int>(v.width), cv_type_of(v.format),
                       const_cast<uint8_t*>(v.data), v.stride);
    }

    inline cv::Mat wrap_mat(const Image& img) {
        return wrap_mat(img.view());
    }

    namespace detail {
        // 3・4 チャンネルの 0 番目と 2 番目を入れ替えた複製
        inline cv::Mat swap_red_blue(const cv::Mat& src) {
            cv::Mat dst(src.rows, src.cols, src.type());
            if (src.channels() == 3) {
                const int from_to[] = {0, 2, 1, 1, 2, 0};
                cv::mixChannels(&src, 1, &dst, 1, from_to, 3);
            } else {
                const int from_to[] = {0, 2, 1, 1, 2, 0, 3, 3};
                cv::mixChannels(&src, 1, &dst, 1, from_to, 4);
            }
            return dst;
        }

        inline bool is_rgb_order(PixelFormat f) {
            switch (f) {
            case PixelFormat::RGB8: case PixelFormat::RGBA8:
            case PixelFormat::RGB16: case PixelFormat::RGBA16:
            case PixelFormat::RGB32F: case PixelFormat::RGBA32F:
                return true;
            default:
                return false;
            }
        }
    }

    // OpenCV の並び (BGR) の Mat に複製する
    inline cv::Mat to_mat(const ImageView& v) {
        cv::Mat m = wrap_mat(v);
        return detail::is_rgb_order(v.format) ? detail::swap_red_blue(m) : m.clone();
    }

    inline cv::Mat to_mat(const Image& img) {
        return to_mat(img.view());
    }

    // OpenCV の並び (BGR) の Mat から Image に複製する。
    // CV_8UC1/2/3/4、CV_16UC1/2/3/4、CV_32FC3/4 を受け付け、それ以外は ImageFormatError
    inline Image from_mat(const cv::Mat& m) {
        if (m.dims != 2) throw ImageFormatError("from_mat() needs a 2-dimensional cv::Mat");
        PixelFormat f;
        bool swap = false;
        switch (m.type()) {
        case CV_8UC1: f = PixelFormat::L8; break;
        case CV_8UC2: f = PixelFormat::LA8; break;
        case CV_8UC3: f = PixelFormat::BGR8; break;
        case CV_8UC4: f = PixelFormat::BGRA8; break;
        case CV_16UC1: f = PixelFormat::L16; break;
        case CV_16UC2: f = PixelFormat::LA16; break;
        case CV_16UC3: f = PixelFormat::RGB16; swap = true; break;
        case CV_16UC4: f = PixelFormat::RGBA16; swap = true; break;
        case CV_32FC3: f = PixelFormat::RGB32F; swap = true; break;
        case CV_32FC4: f = PixelFormat::RGBA32F; swap = true; break;
        default:
            throw ImageFormatError("from_mat() does not support the cv::Mat type " + std::to_string(m.type()));
        }
        const cv::Mat src = swap ? detail::swap_red_blue(m) : m;
        return Image(static_cast<uint32_t>(src.cols), static_cast<uint32_t>(src.rows), f, src.data, src.step[0]);
    }

} // namespace juiz
