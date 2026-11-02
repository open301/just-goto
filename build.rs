//! Detects whether the frontend has been built so the backend can compile without it.

use std::path::Path;

/// Frontend files embedded by `src/api/frontend.rs`.
const ASSETS: [&str; 3] = [
    "web/dist/index.html",
    "web/dist/assets/app.js",
    "web/dist/assets/app.css",
];

/// Sets `missing_frontend` when any asset is absent, which lets API type generation run before the frontend build.
fn main() {
    println!("cargo::rustc-check-cfg=cfg(missing_frontend)");
    for asset in ASSETS {
        println!("cargo::rerun-if-changed={asset}");
    }

    if !ASSETS.iter().all(|asset| Path::new(asset).is_file()) {
        println!("cargo::rustc-cfg=missing_frontend");
        println!(
            "cargo::warning=web/dist is missing; this binary will not include the web interface. Run `npm run build` first."
        );
    }
}
