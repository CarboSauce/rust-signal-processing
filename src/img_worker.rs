use crate::AppWindow;
use plotters::prelude::*;
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};
use std::sync::mpsc::{self, Sender};
use std::thread;

/// Pixel data can be moved safely between the Slint UI thread and the worker.
pub type PixelBuffer = SharedPixelBuffer<Rgba8Pixel>;

fn calculate_kmeans(input: PixelBuffer, k: usize, max_iterations: usize) -> Option<PixelBuffer> {
    if k == 0 || max_iterations == 0 {
        return None;
    }

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

    Some(output)
}

fn calculate_histogram(input: PixelBuffer) -> Option<PixelBuffer> {
    const WIDTH: u32 = 700;
    const HEIGHT: u32 = 600;

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
                .x_desc("RGB intensity")
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

    Some(output)
}

fn calculate_blp(input: PixelBuffer) -> Option<PixelBuffer> {
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

    // left, bottom-left, bottom, bottom-right,
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

    Some(output)
}

enum ImageJob {
    Blp(PixelBuffer),
    Histogram(PixelBuffer),
    KMeans {
        image: PixelBuffer,
        clusters: usize,
        max_iterations: usize,
    },
}

#[derive(Clone)]
pub struct ImgWorker {
    sender: Sender<ImageJob>,
}

impl ImgWorker {
    pub fn new(ui: slint::Weak<AppWindow>) -> Self {
        let (sender, receiver) = mpsc::channel::<ImageJob>();

        thread::Builder::new()
            .name("image-processing-worker".into())
            .spawn(move || {
                // This loop runs for the lifetime of the worker.
                while let Ok(job) = receiver.recv() {
                    match job {
                        ImageJob::Blp(image) => {
                            if let Some(result) = calculate_blp(image) {
                                let _ = ui.upgrade_in_event_loop(move |ui| {
                                    ui.set_blp_image(Image::from_rgba8(result));
                                });
                            }
                        }

                        ImageJob::Histogram(image) => {
                            if let Some(result) = calculate_histogram(image) {
                                let _ = ui.upgrade_in_event_loop(move |ui| {
                                    ui.set_hist_image(Image::from_rgba8(result));
                                });
                            }
                        }

                        ImageJob::KMeans {
                            image,
                            clusters,
                            max_iterations,
                        } => {
                            if let Some(result) = calculate_kmeans(image, clusters, max_iterations)
                            {
                                let _ = ui.upgrade_in_event_loop(move |ui| {
                                    ui.set_kmeans_image(Image::from_rgba8(result));
                                });
                            }
                        }
                    }
                }
            })
            .expect("Failed to start image processing worker");

        Self { sender }
    }

    pub fn calculate_blp(&self, image: PixelBuffer) {
        self.send(ImageJob::Blp(image));
    }

    pub fn calculate_histogram(&self, image: PixelBuffer) {
        self.send(ImageJob::Histogram(image));
    }

    pub fn calculate_kmeans(&self, image: PixelBuffer, clusters: usize, max_iterations: usize) {
        self.send(ImageJob::KMeans {
            image,
            clusters,
            max_iterations,
        });
    }

    fn send(&self, job: ImageJob) {
        if self.sender.send(job).is_err() {
            eprintln!("Image worker is no longer running");
        }
    }
}
