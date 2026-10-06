use std::fs;
use std::fs::read_dir;
use std::fs::*;
use std::io;
use std::path::Path;

fn visit_dirs(dir: &Path, cb: &impl Fn(&DirEntry)) -> io::Result<()> {
    if dir.is_dir() {
        for entry in read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                visit_dirs(&path, cb)?;
            } else {
                cb(&entry);
            }
        }
    }
    Ok(())
}

fn main() {
    visit_dirs("ui/".as_ref(), &|e: &DirEntry| -> () {
        slint_build::compile(e.path()).expect("Slint build failed")
    })
    .expect("Slint build failed");
}
