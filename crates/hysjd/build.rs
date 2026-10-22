//! Google's client secret for "Sign in with Google" (see `provider.rs`): from
//! `HYSJ_GOOGLE_CLIENT_SECRET` (release builds: a GitHub secret), else, on a
//! Mac, from the keychain item `hysj-google-client-secret`. Neither: Gmail is
//! command line only. Never in the repo.
fn main() {
    println!("cargo:rerun-if-env-changed=HYSJ_GOOGLE_CLIENT_SECRET");
    if std::env::var_os("HYSJ_GOOGLE_CLIENT_SECRET").is_some() || !cfg!(target_os = "macos") {
        return;
    }
    let found = std::process::Command::new("security")
        .args([
            "find-generic-password",
            "-s",
            "hysj-google-client-secret",
            "-w",
        ])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(secret) = found {
        println!("cargo:rustc-env=HYSJ_GOOGLE_CLIENT_SECRET={secret}");
    }
}
