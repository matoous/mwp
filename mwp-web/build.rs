use std::{
    env,
    fs::{create_dir_all, File},
    io::Write,
    path::Path,
};

fn main() -> Result<(), Box<grass::Error>> {
    let out_dir = env::var("OUT_DIR").unwrap();
    let static_files = Path::new(&out_dir).join("static");

    create_dir_all(&static_files)?;

    println!("cargo:rerun-if-changed=src/static/style.scss");
    let css = grass::from_string(
        include_str!("src/static/styles.scss"),
        &grass::Options::default().style(grass::OutputStyle::Compressed),
    )?;
    let css_out = static_files.join("styles.css");
    let mut file = File::create(css_out)?;
    file.write_all(css.as_bytes())?;

    println!("cargo:rerun-if-changed=src/static/script.js");
    let js_out = static_files.join("script.js");
    let mut file = File::create(js_out)?;
    file.write_all(include_bytes!("src/static/script.js"))?;

    Ok(())
}
