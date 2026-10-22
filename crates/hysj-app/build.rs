//! Builds the web UI (`ui/`) before Tauri embeds it, so plain `cargo run`
//! works. Needs Node.js (npm) on the PATH; HYSJ_SKIP_UI=1 skips it.

use std::path::Path;
use std::process::Command;

fn npm(args: &[&str]) {
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let status = Command::new(npm)
        .args(args)
        .current_dir("ui")
        .status()
        .unwrap_or_else(|e| {
            panic!(
                "Hysj's UI needs Node.js and npm to build ({e}). Install Node, then build again."
            )
        });
    assert!(
        status.success(),
        "`npm {}` failed in crates/hysj-app/ui",
        args.join(" ")
    );
}

fn main() {
    for p in [
        "src",
        "public",
        "index.html",
        "package.json",
        "package-lock.json",
        "vite.config.ts",
        "tsconfig.json",
    ] {
        println!("cargo:rerun-if-changed=ui/{p}");
    }
    println!("cargo:rerun-if-env-changed=HYSJ_SKIP_UI");
    if std::env::var_os("HYSJ_SKIP_UI").is_none() {
        if !Path::new("ui/node_modules").exists() {
            npm(&["ci"]);
        }
        npm(&["run", "build"]);
    }
    tauri_build::build();
}
