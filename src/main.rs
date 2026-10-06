use image::DynamicImage;
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

    ui.run()?;

    Ok(())
}
