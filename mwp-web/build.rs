use std::{fs::File, io::Write};

fn main() -> Result<(), Box<grass::Error>> {
    println!("cargo:rerun-if-changed=src/static/style.scss");
    let css = grass::from_string(
        include_str!("src/static/styles.scss"),
        &grass::Options::default(),
    )?;
    // NOTE: this doesn't work well with `cargo watch -x run`
    // see: https://github.com/rust-lang/cargo/issues/3076
    let mut file = File::create("static/styles.css")?;
    file.write_all(css.as_bytes())?;

    println!("cargo:rerun-if-changed=src/static/script.js");
    let mut file = File::create("static/script.js")?;
    file.write_all(include_bytes!("src/static/script.js"))?;

    Ok(())
}
