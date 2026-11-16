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
        .add_filter("Image", &["png", "bmp", "jpg", "jpeg", "avif", "svg"]);

    let Some(file) = dialog.pick_file() else {
        return Err(LoadingError::Canceled);
    };

    slint::Image::load_from_path(&file).map_err(|e| ErrorOpening(e.to_string()))
}

#[derive(Debug)]
enum ImageSaveError {
    Canceled,
    Conversion(String),
    Saving(String),
}

fn save_image(img: slint::Image) -> Result<(), ImageSaveError> {
    let Some(file) = rfd::FileDialog::new()
        .set_title("Save image")
        .add_filter("PNG image", &["png"])
        .add_filter("JPEG image", &["jpg", "jpeg"])
        .add_filter("Bitmap image", &["bmp"])
        .set_file_name("image.png")
        .save_file()
    else {
        return Err(ImageSaveError::Canceled);
    };

    let rgba = img
        .to_rgba8()
        .ok_or_else(|| ImageSaveError::Conversion("Could not convert image to RGBA".into()))?;

    let width = rgba.width();
    let height = rgba.height();

    let buffer: Vec<u8> = rgba
        .as_slice()
        .iter()
        .flat_map(|pixel| [pixel.r, pixel.g, pixel.b, pixel.a])
        .collect();

    let format = match file
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "png" => image::ImageFormat::Png,
        "jpg" | "jpeg" => image::ImageFormat::Jpeg,
        "bmp" => image::ImageFormat::Bmp,
        _ => {
            return Err(ImageSaveError::Saving("Unsupported image format".into()));
        }
    };

    let image = image::RgbaImage::from_raw(width, height, buffer)
        .ok_or_else(|| ImageSaveError::Conversion("Invalid image buffer".into()))?;

    image
        .save_with_format(&file, format)
        .map_err(|e| ImageSaveError::Saving(e.to_string()))?;

    Ok(())
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

use plotters::prelude::*;

fn calculate_histogram(img: slint::Image) -> Option<slint::Image> {
    use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

    const WIDTH: u32 = 700;
    const HEIGHT: u32 = 600;

    let input = img.to_rgba8()?;
    let pixels = input.as_slice();

    // Calculate histograms.
    let mut red = [0u32; 256];
    let mut green = [0u32; 256];
    let mut blue = [0u32; 256];

    for pixel in pixels {
        red[pixel.r as usize] += 1;
        green[pixel.g as usize] += 1;
        blue[pixel.b as usize] += 1;
    }

    let max_count = *red.iter().chain(green.iter()).chain(blue.iter()).max()?;

    if max_count == 0 {
        return None;
    }

    // Plotters uses RGB format (3 bytes per pixel).
    let mut buffer = vec![255u8; (WIDTH * HEIGHT * 3) as usize];

    {
        let root = BitMapBackend::with_buffer(&mut buffer, (WIDTH, HEIGHT)).into_drawing_area();

        root.fill(&WHITE).ok()?;

        let panels = root.split_evenly((3, 1));

        let channels = [
            ("Red channel", &red, RED),
            ("Green channel", &green, GREEN),
            ("Blue channel", &blue, BLUE),
        ];

        for ((title, histogram, color), area) in channels.iter().zip(panels.iter()) {
            let mut chart = ChartBuilder::on(area)
                .caption(*title, 20)
                .margin(10)
                .x_label_area_size(30)
                .y_label_area_size(50)
                .build_cartesian_2d(0u32..255u32, 0u32..max_count)
                .ok()?;

            chart
                .configure_mesh()
                .x_desc("LBP value")
                .y_desc("Pixel count")
                .x_labels(16)
                .draw()
                .ok()?;

            chart
                .draw_series(
                    Histogram::vertical(&chart).style(color.filled()).data(
                        histogram
                            .iter()
                            .enumerate()
                            .map(|(x, count)| (x as u32, *count)),
                    ),
                )
                .ok()?;
        }

        root.present().ok()?;
    }

    // Convert RGB buffer to Slint RGBA image.
    let mut output = SharedPixelBuffer::<Rgba8Pixel>::new(WIDTH, HEIGHT);

    for (rgb, rgba) in buffer
        .as_chunks::<3>()
        .0
        .iter()
        .zip(output.make_mut_slice().iter_mut())
    {
        *rgba = Rgba8Pixel {
            r: rgb[0],
            g: rgb[1],
            b: rgb[2],
            a: 255,
        };
    }

    Some(Image::from_rgba8(output))
}

fn calculate_kmeans(img: slint::Image, k: usize, max_iterations: usize) -> Option<slint::Image> {
    use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

    if k == 0 || max_iterations == 0 {
        return None;
    }

    let input = img.to_rgba8()?;
    let width = input.width();
    let height = input.height();
    let pixels = input.as_slice();

    let n = pixels.len();

    if k > n {
        return None;
    }

    // Each pixel is represented as an RGB point.
    let points: Vec<[f64; 3]> = pixels
        .iter()
        .map(|p| [p.r as f64, p.g as f64, p.b as f64])
        .collect();

    // Squared Euclidean distance.
    fn distance(a: &[f64; 3], b: &[f64; 3]) -> f64 {
        (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
    }

    // Initialize centroids using farthest-point selection.
    let mut centroids: Vec<[f64; 3]> = Vec::with_capacity(k);

    centroids.push(points[0]);

    while centroids.len() < k {
        let mut best_index = 0;
        let mut best_distance = -1.0;

        for (i, point) in points.iter().enumerate() {
            let nearest_distance = centroids
                .iter()
                .map(|c| distance(point, c))
                .fold(f64::INFINITY, f64::min);

            if nearest_distance > best_distance {
                best_distance = nearest_distance;
                best_index = i;
            }
        }

        centroids.push(points[best_index]);
    }

    // Cluster assignment for every pixel.
    let mut labels = vec![usize::MAX; n];

    for _ in 0..max_iterations {
        let mut changed = false;

        // STEP 1: Assign pixels to nearest centroid.
        for (i, point) in points.iter().enumerate() {
            let mut best_cluster = 0;
            let mut min_distance = f64::INFINITY;

            for (cluster, centroid) in centroids.iter().enumerate() {
                let d = distance(point, centroid);

                if d < min_distance {
                    min_distance = d;
                    best_cluster = cluster;
                }
            }

            if labels[i] != best_cluster {
                labels[i] = best_cluster;
                changed = true;
            }
        }

        // STEP 2: Calculate new centroid positions.
        let mut sums = vec![[0.0f64; 3]; k];
        let mut counts = vec![0usize; k];

        for (i, point) in points.iter().enumerate() {
            let cluster = labels[i];

            sums[cluster][0] += point[0];
            sums[cluster][1] += point[1];
            sums[cluster][2] += point[2];

            counts[cluster] += 1;
        }

        // STEP 3: Update centroids.
        for cluster in 0..k {
            if counts[cluster] > 0 {
                let count = counts[cluster] as f64;

                centroids[cluster] = [
                    sums[cluster][0] / count,
                    sums[cluster][1] / count,
                    sums[cluster][2] / count,
                ];
            }
        }

        // Stop when assignments no longer change.
        if !changed {
            break;
        }
    }

    // STEP 4: Generate segmented image.
    let mut output = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);

    let dst = output.make_mut_slice();

    for i in 0..n {
        let centroid = centroids[labels[i]];

        dst[i] = Rgba8Pixel {
            r: centroid[0].round().clamp(0.0, 255.0) as u8,
            g: centroid[1].round().clamp(0.0, 255.0) as u8,
            b: centroid[2].round().clamp(0.0, 255.0) as u8,
            a: pixels[i].a,
        };
    }

    Some(Image::from_rgba8(output))
}

fn calculate_blp(img: slint::Image) -> Option<slint::Image> {
    use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

    let input = img.to_rgba8()?;

    let width = input.width();
    let height = input.height();

    if width < 3 || height < 3 {
        return None;
    }

    let src = input.as_slice();

    let mut output = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
    let dst = output.make_mut_slice();

    dst.fill(Rgba8Pixel {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    });

    // Left, bottom-left, bottom, bottom-right,
    // right, top-right, top, top-left.
    let neighbors: [(i32, i32); 8] = [
        (-1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
        (1, 0),
        (1, -1),
        (0, -1),
        (-1, -1),
    ];

    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let index = (y * width + x) as usize;
            let center = src[index];

            let mut r = 0u8;
            let mut g = 0u8;
            let mut b = 0u8;

            for (bit, (dx, dy)) in neighbors.iter().enumerate() {
                let nx = (x as i32 + dx) as u32;
                let ny = (y as i32 + dy) as u32;

                let neighbor = src[(ny * width + nx) as usize];

                let mask = 1 << (7 - bit);

                if neighbor.r > center.r {
                    r |= mask;
                }

                if neighbor.g > center.g {
                    g |= mask;
                }

                if neighbor.b > center.b {
                    b |= mask;
                }
            }

            dst[index] = Rgba8Pixel { r, g, b, a: 255 };
        }
    }

    Some(Image::from_rgba8(output))
}

fn calculate_image(area: Area, img: slint::Image) -> Option<ImageCalculatedData> {
    if (area.x == 0) || (area.y == 0) || (area.x + area.width == 0) || (area.y + area.height == 0) {
        return None;
    }

    let mut data = img.to_rgba8()?;

    let data = data.make_mut_slice();
    let mut data = extract_rect_channels(
        data,
        img.size().width as i32,
        img.size().height as i32,
        area,
    );

    if data[0].is_empty() || data[1].is_empty() || data[2].is_empty() {
        return None;
    }

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

    let ui_handle = ui.as_weak();
    ui.on_calculate_blp(move |img| {
        let ui = ui_handle.unwrap();
        let Some(data) = calculate_blp(img) else {
            return;
        };
        ui.set_blp_image(data);
    });

    let ui_handle = ui.as_weak();
    ui.on_calculate_kmeans(move |img, clusters, k| {
        let ui = ui_handle.unwrap();
        let Some(data) = calculate_kmeans(img, clusters as usize, k as usize) else {
            return;
        };
        ui.set_kmeans_image(data);
    });

    let ui_handle = ui.as_weak();
    ui.on_calculate_histogram(move |img| {
        let ui = ui_handle.unwrap();
        let Some(data) = calculate_histogram(img) else {
            return;
        };

        ui.set_hist_image(data);
    });

    ui.on_save_histogram(move |img| match save_image(img) {
        Err(ImageSaveError::Conversion(err)) => {
            let dialog = ErrorDialog::new().unwrap();
            dialog.set_error_text(err.into());
            dialog.show().unwrap();
        }
        Err(ImageSaveError::Saving(err)) => {
            let dialog = ErrorDialog::new().unwrap();
            dialog.set_error_text(err.into());
            dialog.show().unwrap();
        }
        Err(ImageSaveError::Canceled) => {
            let dialog = CanceledDialog::new().unwrap();
            dialog.show().unwrap();
        }
        _ => {}
    });

    ui.run()?;

    Ok(())
}
