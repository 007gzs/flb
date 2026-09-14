use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let dist = manifest.join("../../frontend/dist");
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("www");
    let _ = fs::remove_dir_all(&out);
    fs::create_dir_all(&out).expect("create embedded www dir");

    if dist.join("index.html").is_file() {
        copy_dir(&dist, &out).expect("copy frontend dist into embed dir");
    } else {
        fs::write(
            out.join("index.html"),
            r#"<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>FLB</title>
  </head>
  <body>
    <p>Admin UI was not embedded. Build <code>frontend/dist</code> before compiling, or pass <code>--www-dir</code>.</p>
  </body>
</html>
"#,
        )
        .expect("write placeholder index.html");
    }

    println!("cargo:rerun-if-changed={}", dist.display());
}

fn copy_dir(src: &Path, dst: &Path) -> io::Result<()> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            fs::create_dir_all(&to)?;
            copy_dir(&entry.path(), &to)?;
        } else if ty.is_file() {
            fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}
