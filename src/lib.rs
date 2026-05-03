use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use opencv::{
    core::{self, Mat, Point, Rect, Scalar, Size, Vector},
    imgcodecs, imgproc,
    prelude::*,
};

const TEMPLATE_MIN_SCORE: f64 = 0.35;

const CANNY_LOW: f64 = 50.0;
const CANNY_HIGH: f64 = 150.0;
const CANNY_APERTURE: i32 = 3;

const CONTOUR_MIN_W: i32 = 20;
const CONTOUR_MIN_H: i32 = 20;
const CONTOUR_MAX_W: i32 = 120;
const CONTOUR_MAX_H: i32 = 120;
const CONTOUR_MIN_AREA: f64 = 300.0;
const CONTOUR_MIN_X: i32 = 20;

const FALLBACK_RESULT: &str = "-1,-1";

#[no_mangle]
pub extern "system" fn FindSliderPosition(
    bg_path: *const c_char,
    slider_path: *const c_char,
) -> *mut c_char {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        find_slider_position(bg_path, slider_path)
    }))
    .ok()
    .and_then(Result::ok)
    .map(|(x, y)| format!("{x},{y}"))
    .unwrap_or_else(|| FALLBACK_RESULT.to_string());

    make_result(&result)
}

#[no_mangle]
pub extern "system" fn FreeResult(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }

    unsafe {
        let _ = CString::from_raw(ptr);
    }
}

fn find_slider_position(
    bg_path: *const c_char,
    slider_path: *const c_char,
) -> Result<(i32, i32), String> {
    let bg_path = cstr_to_string(bg_path)?;
    if bg_path.trim().is_empty() {
        return Err("background path is empty".to_string());
    }

    let slider_path = cstr_to_string(slider_path).unwrap_or_default();
    if slider_path.trim().is_empty() {
        return detect_with_contours(&bg_path);
    }

    match detect_with_template(&bg_path, &slider_path) {
        Ok(pos) => Ok(pos),
        Err(_) => detect_with_contours(&bg_path),
    }
}

fn cstr_to_string(ptr: *const c_char) -> Result<String, String> {
    if ptr.is_null() {
        return Err("null string pointer".to_string());
    }

    unsafe {
        CStr::from_ptr(ptr)
            .to_str()
            .map(|s| s.to_string())
            .map_err(|e| e.to_string())
    }
}

fn make_result(s: &str) -> *mut c_char {
    match CString::new(s) {
        Ok(cstr) => cstr.into_raw(),
        Err(_) => {
            unsafe { CString::from_vec_unchecked(FALLBACK_RESULT.as_bytes().to_vec()) }.into_raw()
        }
    }
}

fn detect_with_template(bg_path: &str, slider_path: &str) -> Result<(i32, i32), String> {
    let bg = read_image(bg_path, imgcodecs::IMREAD_COLOR)?;
    let slider = read_image(slider_path, imgcodecs::IMREAD_UNCHANGED)?;

    if slider.cols() <= 0
        || slider.rows() <= 0
        || slider.cols() > bg.cols()
        || slider.rows() > bg.rows()
    {
        return Err("invalid slider image size".to_string());
    }

    let bg_gray = to_gray(&bg)?;
    let slider_gray = to_gray(&slider)?;

    let mut bg_edges = Mat::default();
    let mut slider_edges = Mat::default();

    // Canny emphasizes the gap/slider contour and reduces texture noise.
    imgproc::canny(
        &bg_gray,
        &mut bg_edges,
        CANNY_LOW,
        CANNY_HIGH,
        CANNY_APERTURE,
        false,
    )
    .map_err(|e| e.to_string())?;
    imgproc::canny(
        &slider_gray,
        &mut slider_edges,
        CANNY_LOW,
        CANNY_HIGH,
        CANNY_APERTURE,
        false,
    )
    .map_err(|e| e.to_string())?;

    let mask = build_template_mask(&slider, &slider_edges)?;
    let mut result = Mat::default();

    // TM_CCORR_NORMED supports a mask and works well for edge templates.
    imgproc::match_template(
        &bg_edges,
        &slider_edges,
        &mut result,
        imgproc::TM_CCORR_NORMED,
        &mask,
    )
    .map_err(|e| e.to_string())?;

    let mut min_val = 0.0;
    let mut max_val = 0.0;
    let mut min_loc = Point::default();
    let mut max_loc = Point::default();
    core::min_max_loc(
        &result,
        Some(&mut min_val),
        Some(&mut max_val),
        Some(&mut min_loc),
        Some(&mut max_loc),
        &Mat::default(),
    )
    .map_err(|e| e.to_string())?;

    if max_val < TEMPLATE_MIN_SCORE {
        return Err(format!("template score too low: {max_val:.3}"));
    }

    Ok((max_loc.x, max_loc.y))
}

fn detect_with_contours(bg_path: &str) -> Result<(i32, i32), String> {
    let bg = read_image(bg_path, imgcodecs::IMREAD_COLOR)?;
    let gray = to_gray(&bg)?;

    let mut blurred = Mat::default();
    // Gaussian blur suppresses small background texture before edge detection.
    imgproc::gaussian_blur(
        &gray,
        &mut blurred,
        Size::new(5, 5),
        0.0,
        0.0,
        core::BORDER_DEFAULT,
        core::AlgorithmHint::ALGO_HINT_DEFAULT,
    )
    .map_err(|e| e.to_string())?;

    let mut edges = Mat::default();
    imgproc::canny(
        &blurred,
        &mut edges,
        CANNY_LOW,
        CANNY_HIGH,
        CANNY_APERTURE,
        false,
    )
    .map_err(|e| e.to_string())?;

    let kernel =
        imgproc::get_structuring_element(imgproc::MORPH_RECT, Size::new(5, 5), Point::new(-1, -1))
            .map_err(|e| e.to_string())?;

    let mut closed = Mat::default();
    // Closing connects broken edge fragments around the notch shadow.
    imgproc::morphology_ex(
        &edges,
        &mut closed,
        imgproc::MORPH_CLOSE,
        &kernel,
        Point::new(-1, -1),
        2,
        core::BORDER_CONSTANT,
        Scalar::default(),
    )
    .map_err(|e| e.to_string())?;

    let mut contours: Vector<Vector<Point>> = Vector::new();
    imgproc::find_contours(
        &closed,
        &mut contours,
        imgproc::RETR_EXTERNAL,
        imgproc::CHAIN_APPROX_SIMPLE,
        Point::new(0, 0),
    )
    .map_err(|e| e.to_string())?;

    let mut best: Option<(Rect, f64)> = None;
    for contour in contours {
        let rect = imgproc::bounding_rect(&contour).map_err(|e| e.to_string())?;
        if !is_contour_candidate(rect) {
            continue;
        }

        let contour_area = imgproc::contour_area(&contour, false).map_err(|e| e.to_string())?;
        if contour_area < CONTOUR_MIN_AREA {
            continue;
        }

        let score = contour_score(rect, contour_area);
        if best.map_or(true, |(_, best_score)| score > best_score) {
            best = Some((rect, score));
        }
    }

    best.map(|(rect, _)| (rect.x, rect.y))
        .ok_or_else(|| "no contour candidate found".to_string())
}

fn read_image(path: &str, flags: i32) -> Result<Mat, String> {
    let image = imgcodecs::imread(path, flags).map_err(|e| e.to_string())?;
    if image.empty() {
        Err(format!("failed to read image: {path}"))
    } else {
        Ok(image)
    }
}

fn to_gray(image: &Mat) -> Result<Mat, String> {
    let channels = image.channels();
    if channels == 1 {
        return image.try_clone().map_err(|e| e.to_string());
    }

    let mut gray = Mat::default();
    let code = match channels {
        3 => imgproc::COLOR_BGR2GRAY,
        4 => imgproc::COLOR_BGRA2GRAY,
        _ => return Err(format!("unsupported channel count: {channels}")),
    };

    imgproc::cvt_color(
        image,
        &mut gray,
        code,
        0,
        core::AlgorithmHint::ALGO_HINT_DEFAULT,
    )
    .map_err(|e| e.to_string())?;
    Ok(gray)
}

fn build_template_mask(slider: &Mat, slider_edges: &Mat) -> Result<Mat, String> {
    if slider.channels() == 4 {
        let mut channels: Vector<Mat> = Vector::new();
        core::split(slider, &mut channels).map_err(|e| e.to_string())?;
        let alpha = channels.get(3).map_err(|e| e.to_string())?;

        let mut mask = Mat::default();
        imgproc::threshold(&alpha, &mut mask, 10.0, 255.0, imgproc::THRESH_BINARY)
            .map_err(|e| e.to_string())?;
        return Ok(mask);
    }

    let mut mask = Mat::default();
    imgproc::threshold(slider_edges, &mut mask, 1.0, 255.0, imgproc::THRESH_BINARY)
        .map_err(|e| e.to_string())?;
    Ok(mask)
}

fn is_contour_candidate(rect: Rect) -> bool {
    rect.x > CONTOUR_MIN_X
        && rect.width >= CONTOUR_MIN_W
        && rect.height >= CONTOUR_MIN_H
        && rect.width <= CONTOUR_MAX_W
        && rect.height <= CONTOUR_MAX_H
}

fn contour_score(rect: Rect, contour_area: f64) -> f64 {
    let box_area = (rect.width * rect.height) as f64;
    if box_area <= 0.0 {
        return 0.0;
    }

    let fill_ratio = contour_area / box_area;
    let aspect = rect.width as f64 / rect.height as f64;
    let aspect_score = 1.0 - (aspect - 1.0).abs().min(1.0);
    let size_score = (box_area / (CONTOUR_MAX_W * CONTOUR_MAX_H) as f64).min(1.0);

    contour_area * 0.55 + fill_ratio * 180.0 + aspect_score * 120.0 + size_score * 80.0
}
