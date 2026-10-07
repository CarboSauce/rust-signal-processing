use slint::Image;
use std::error::Error;

use crate::LoadingError::ErrorOpening;

slint::include_modules!();

enum LoadingError {
    Canceled,
    ErrorOpening(String),
}

fn open_file() -> Result<slint::Image, LoadingError> {
    let dialog = rfd::FileDialog::new()
        .set_title("Select image file")
        .add_filter("Image", &["png", "bmp", "jpeg", "avif", "svg"]);

    let Some(file) = dialog.pick_file() else {
        return Err(LoadingError::Canceled);
    };

    slint::Image::load_from_path(&file).map_err(|e| ErrorOpening(e.to_string()))
}

fn calc_average(data: &[u8]) -> f32 {
    data.iter().fold(0.0f32, |c, &x| c + x as f32) / data.len() as f32
}

fn calc_median(data: &mut [u8]) -> u8 {
    *data.select_nth_unstable(data.len() / 2).1
}

fn calc_variance(data: &[u8], average: f32) -> f32 {
    data.iter().fold(0.0f32, |c, &x| {
        c + (x as f32 - average) * (x as f32 - average)
    }) / data.len() as f32
}

fn calc_std_deviation(variance: f32) -> f32 {
    variance.sqrt()
}
fn extract_rect_channels(
    pixels: &[slint::Rgba8Pixel],
    image_width: i32,
    image_height: i32,
    area: Area,
) -> Vec<Vec<u8>> {
    assert!(area.x < image_width);
    assert!(area.y < image_height);
    assert!(area.x + area.width <= image_width);
    assert!(area.y + area.height <= image_height);

    let mut result: Vec<Vec<u8>> = (0..3)
        .map(|_| Vec::with_capacity(area.width as usize * area.height as usize))
        .collect::<Vec<_>>();

    for row in area.y..area.y + area.height {
        let row_start = row * image_width + area.x;

        for col in 0..area.width {
            let pixel_start = row_start + col;
            let pixel_start = pixel_start as usize;

            result[0].push(pixels[pixel_start].r);
            result[1].push(pixels[pixel_start].g);
            result[2].push(pixels[pixel_start].b);
        }
    }

    result
}
fn calculate_image(area: Area, img: slint::Image) -> Option<ImageCalculatedData> {
    let mut data = img.to_rgba8()?;

    let data = data.make_mut_slice();
    let mut data = extract_rect_channels(
        data,
        img.size().width as i32,
        img.size().height as i32,
        area,
    );

    let avg_r = calc_average(data[0].as_slice());
    let avg_g = calc_average(data[1].as_slice());
    let avg_b = calc_average(data[2].as_slice());

    let median_r = calc_median(data[0].as_mut_slice());
    let median_g = calc_median(data[1].as_mut_slice());
    let median_b = calc_median(data[2].as_mut_slice());

    let variance_r = calc_variance(data[0].as_mut_slice(), avg_r);
    let variance_g = calc_variance(data[1].as_mut_slice(), avg_g);
    let variance_b = calc_variance(data[2].as_mut_slice(), avg_b);

    let std_dev_r = calc_std_deviation(variance_r);
    let std_dev_g = calc_std_deviation(variance_g);
    let std_dev_b = calc_std_deviation(variance_b);

    Some(ImageCalculatedData {
        red_data: ImageChannelCalculatedData {
            average: avg_r,
            median: median_r as f32,
            variance: variance_r,
            std_deviation: std_dev_r,
        },
        green_data: ImageChannelCalculatedData {
            average: avg_g,
            median: median_g as f32,
            variance: variance_g,
            std_deviation: std_dev_g,
        },
        blue_data: ImageChannelCalculatedData {
            average: avg_b,
            median: median_b as f32,
            variance: variance_b,
            std_deviation: std_dev_b,
        },
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let ui = AppWindow::new()?;

    let ui_handle = ui.as_weak();
    ui.on_file_open(move || {
        let img = open_file();
        match img {
            Ok(val) => {
                let ui = ui_handle.unwrap();
                ui.set_original_image(val);
            }
            Err(ErrorOpening(err)) => {
                let dialog = ErrorDialog::new().unwrap();
                dialog.set_error_text(err.into());
                dialog.show().unwrap();
            }
            Err(LoadingError::Canceled) => {
                let dialog = CanceledDialog::new().unwrap();
                dialog.show().unwrap();
            }
        }
    });

    let ui_handle = ui.as_weak();
    ui.on_calculate_data(move |rec| {
        let ui = ui_handle.unwrap();
        let img = ui.get_original_image();
        let Some(data) = calculate_image(rec, img) else {
            return;
        };
        let dialog = ImageDataDialog::new().unwrap();
        dialog.set_image_data(data);
        dialog.show().unwrap();
    });

    ui.run()?;

    Ok(())
}
