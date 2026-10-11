mod cv_cvt_color;
mod filesystem;
mod filesystem_imwrite;
mod video_capture;
mod video_capture_get;
mod video_capture_read;
mod video_capture_readandget;
mod window;
mod window_show;

pub use juiz_sdk::prelude::*;

use cv_cvt_color::*;
use filesystem::*;
use filesystem_imwrite::*;
use video_capture::*;
use video_capture_get::*;
use video_capture_read::*;
use video_capture_readandget::*;
use window::*;
use window_show::*;

juiz_component_manifest!(
    component_name = "juiz_opencv"
    containers = {
        video_capture = [
            video_capture_get,
            video_capture_read,
            video_capture_readandget
        ],
        window = [
            window_show,
        ],
        filesystem = [
            filesystem_imwrite,
        ]
    },
    processes = [
        cv_cvt_color
    ]
);
