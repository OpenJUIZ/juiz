//! カメラや画面を使わずに確かめられる処理のテスト(画像ファイルの読み書き、色変換、画像の変換)
use crate::convert::{image_to_mat, mat_to_image};
use crate::cv_cvt_color::{bgr_to_rgb, cv_cvt_color};
use crate::filesystem_imwrite::write_image;
use juiz_sdk::image::{DynamicImage, GrayImage, Rgb, RgbImage, RgbaImage};
use juiz_sdk::prelude::*;
use opencv::{
    core::{Mat, Rect},
    imgcodecs::{imread, IMREAD_COLOR},
    prelude::*,
};
use std::path::PathBuf;

/// 画素ごとに値が違う RGB のテスト画像
fn sample_rgb(width: u32, height: u32) -> RgbImage {
    RgbImage::from_fn(width, height, |x, y| {
        Rgb([(x * 10) as u8, (y * 20) as u8, (x + y) as u8 + 100])
    })
}

/// テストごとに別の一時ディレクトリを作る
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("juiz_opencv_test_{}_{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn rgb_image_round_trips_through_mat() {
    let img = DynamicImage::ImageRgb8(sample_rgb(7, 5));
    let mat = image_to_mat(&img).unwrap();
    assert_eq!((mat.cols(), mat.rows(), mat.channels()), (7, 5, 3));
    // チャンネルの並びは入れ替えない
    let px = mat.at_2d::<opencv::core::Vec3b>(2, 3).unwrap();
    assert_eq!(px.0, [30, 40, 105]);
    assert_eq!(mat_to_image(&mat).unwrap(), img);
}

#[test]
fn gray_and_rgba_images_round_trip_through_mat() {
    let gray =
        DynamicImage::ImageLuma8(GrayImage::from_fn(4, 3, |x, y| [(x * 3 + y) as u8].into()));
    let mat = image_to_mat(&gray).unwrap();
    assert_eq!(mat.channels(), 1);
    assert_eq!(mat_to_image(&mat).unwrap(), gray);

    let rgba = DynamicImage::ImageRgba8(RgbaImage::from_fn(4, 3, |x, y| {
        [x as u8, y as u8, 9, 200].into()
    }));
    let mat = image_to_mat(&rgba).unwrap();
    assert_eq!(mat.channels(), 4);
    assert_eq!(mat_to_image(&mat).unwrap(), rgba);
}

#[test]
fn non_continuous_mat_is_converted() {
    let img = sample_rgb(8, 6);
    let mat = image_to_mat(&DynamicImage::ImageRgb8(img.clone())).unwrap();
    // ROI は元の Mat のメモリを指すので連続していない
    let roi = Mat::roi(&mat, Rect::new(2, 1, 3, 4)).unwrap();
    assert!(!roi.is_continuous());
    let out = mat_to_image(&roi).unwrap().to_rgb8();
    assert_eq!(out.dimensions(), (3, 4));
    assert_eq!(out.get_pixel(0, 0), img.get_pixel(2, 1));
    assert_eq!(out.get_pixel(2, 3), img.get_pixel(4, 4));
}

#[test]
fn bgr_to_rgb_swaps_red_and_blue() {
    let img = DynamicImage::ImageRgb8(sample_rgb(5, 4));
    let out = bgr_to_rgb(&img).unwrap().to_rgb8();
    for (x, y, p) in sample_rgb(5, 4).enumerate_pixels() {
        assert_eq!(out.get_pixel(x, y).0, [p.0[2], p.0[1], p.0[0]]);
    }
}

#[test]
fn cv_cvt_color_process_converts_image_read_from_file() {
    // 画像ファイルを OpenCV で読み込み(BGR)、プロセス経由で RGB に変換する
    let dir = temp_dir("cvt_color");
    let path = dir.join("input.png");
    let img = sample_rgb(6, 4);
    img.save(&path).unwrap();
    let bgr = imread(path.to_str().unwrap(), IMREAD_COLOR).unwrap();
    assert_eq!((bgr.cols(), bgr.rows()), (6, 4));

    let mut args = CapsuleMap::new();
    args.insert("img".to_owned(), mat_to_image(&bgr).unwrap().into());
    args.insert("code".to_owned(), jvalue!("COLOR_BGR2RGB").into());
    let out = cv_cvt_color(args).unwrap().to_image().unwrap().to_rgb8();
    assert_eq!(out, img);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn write_image_writes_png_that_can_be_read_back() {
    let dir = temp_dir("imwrite");
    let path = dir.join("output.png");
    let img = sample_rgb(9, 7);
    write_image(
        &DynamicImage::ImageRgb8(img.clone()),
        path.to_str().unwrap(),
    )
    .unwrap();

    // imwrite は Mat を BGR として書くので、image クレートで読むと R と B が入れ替わる
    let read = juiz_sdk::image::open(&path).unwrap().to_rgb8();
    let swapped = bgr_to_rgb(&DynamicImage::ImageRgb8(img)).unwrap().to_rgb8();
    assert_eq!(read, swapped);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn write_image_fails_for_unknown_extension() {
    let dir = temp_dir("imwrite_err");
    let path = dir.join("output.unknown_ext");
    let img = DynamicImage::ImageRgb8(sample_rgb(2, 2));
    assert!(write_image(&img, path.to_str().unwrap()).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}
