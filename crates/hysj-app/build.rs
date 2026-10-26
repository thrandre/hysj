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

/// Builds `hysj-apple-ai` (Apple's on-device model; hysj-ai/apple) where
/// Tauri bundles it (`binaries/`, see externalBin) and beside the binaries
/// for `cargo run`. Without the macOS 26 SDK it builds a stub that says
/// "unavailable", so the app still builds and just hides the choice; both
/// give sentence vectors (embed.swift).
fn apple_helper() {
    let src = Path::new("../hysj-ai/apple");
    println!(
        "cargo:rerun-if-changed={}",
        src.join("main.swift").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        src.join("stub.swift").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        src.join("embed.swift").display()
    );
    let target = std::env::var("TARGET").expect("cargo sets TARGET");
    std::fs::create_dir_all("binaries").expect("binaries/");
    let out = format!("binaries/hysj-apple-ai-{target}");
    let swiftc = |file: &str, min_os: &str| {
        Command::new("swiftc")
            .args(["-O", "-parse-as-library", "-target"])
            .arg(format!("arm64-apple-macos{min_os}"))
            .arg(src.join(file))
            .arg(src.join("embed.swift"))
            .args(["-o", &out])
            .status()
            .is_ok_and(|s| s.success())
    };
    println!("cargo:rerun-if-env-changed=HYSJ_REQUIRE_APPLE_AI");
    if !swiftc("main.swift", "26.0") {
        // Releases and CI must ship the real one.
        assert!(
            std::env::var_os("HYSJ_REQUIRE_APPLE_AI").is_none(),
            "hysj-apple-ai needs Xcode 26 (Foundation Models); HYSJ_REQUIRE_APPLE_AI is set"
        );
        println!(
            "cargo:warning=hysj-apple-ai: no Foundation Models in this SDK; building the stub"
        );
        assert!(
            swiftc("stub.swift", "14.0"),
            "swiftc couldn't build hysj-apple-ai"
        );
    }
    // target/<profile>/, beside hysj-app and hysj (OUT_DIR is target/<profile>/build/<crate>/out).
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    if let Some(profile) = out_dir.ancestors().nth(3) {
        let _ = std::fs::copy(&out, profile.join("hysj-apple-ai"));
    }
}

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        apple_helper();
    }
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
