use std::env;
use std::path::Path;

fn main() {
    // Official builds report the workspace semver from Cargo.toml
    // (e.g. `4.6.1`). Local dev builds set `FRAME_LOCAL_VERSION` to a 4-number
    // string `<last-release>.<local-seq>` (e.g. `4.6.0.3`) via
    // `tools/build-local.sh`, so `framec --version` distinguishes an
    // unofficial local build from a release. The override is honored only when
    // the env var is present; a plain `cargo build` is unaffected.
    let frame_version = env::var("FRAME_LOCAL_VERSION").unwrap_or_else(|_| {
        env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION must be set by Cargo")
    });
    println!("cargo:rustc-env=FRAME_VERSION={}", frame_version);
    println!("cargo:rerun-if-env-changed=FRAME_LOCAL_VERSION");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let project_root = Path::new(&manifest_dir).parent().unwrap();

    println!(
        "cargo:rerun-if-changed={}",
        project_root.join("Cargo.toml").display()
    );
}
