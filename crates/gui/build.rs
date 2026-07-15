use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    slint_build::compile("src/appwindow.slint").unwrap();

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let assets_src = manifest_dir.join("../../assets");

    // OUT_DIR = target/<profile>/build/.../out
    // Піднімаємось на 3 рівні вгору: out → <hash> → build → <profile>
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let profile_dir = out_dir
        .parent().unwrap()
        .parent().unwrap()
        .parent().unwrap();

    let assets_dst = profile_dir.join("assets");

    if assets_src.exists() {
        let _ = fs::remove_dir_all(&assets_dst);
        copy_dir_all(&assets_src, &assets_dst).unwrap();
    }

    println!("cargo:rerun-if-changed=../../assets");
}

fn copy_dir_all(
    src: impl AsRef<std::path::Path>,
    dst: impl AsRef<std::path::Path>,
) -> std::io::Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}