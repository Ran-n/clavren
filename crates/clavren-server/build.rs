// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:17.295544
//+ Revised: 	2026/11/03 04:54:17.295544
// -----------------------------------------------------------------------------

//! Embeds the built dashboard (`web/admin/dist`) in the server binary as a table of
//! `(path, bytes)`. Without a build, the table is empty and the server shows a page saying how to
//! build it; the API works either way.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::{env, fs, io};

fn main() -> io::Result<()> {
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/admin/dist");
    println!("cargo:rerun-if-changed={}", dist.display());
    let mut files = Vec::new();
    if dist.join("index.html").is_file() {
        collect(&dist, &dist, &mut files)?;
    } else {
        println!("cargo:warning=web/admin/dist is missing: run `npm run build` in web/admin to embed the dashboard");
    }
    files.sort();
    let mut table = String::from(
        "/// The dashboard's files, by path relative to `dist`.\npub static ASSETS: &[(&str, &[u8])] = &[\n",
    );
    for (path, absolute) in files {
        let _ = writeln!(table, "    ({path:?}, include_bytes!({absolute:?})),");
    }
    table.push_str("];\n");
    fs::write(PathBuf::from(env::var_os("OUT_DIR").ok_or(io::ErrorKind::NotFound)?).join("dashboard_assets.rs"), table)
}

fn collect(root: &Path, folder: &Path, files: &mut Vec<(String, String)>) -> io::Result<()> {
    for entry in fs::read_dir(folder)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(root, &path, files)?;
        } else {
            let relative = path.strip_prefix(root).map_err(io::Error::other)?;
            let web_path =
                relative.components().map(|part| part.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
            files.push((web_path, fs::canonicalize(&path)?.display().to_string()));
        }
    }
    Ok(())
}
