fn main() {
    // `tauri-build` only emits `rerun-if-changed` rules for the Tauri config,
    // capabilities, resources and icons — not for `frontendDist`.
    //
    // Without these two lines a `cargo build`/`cargo test` that ran before
    // `dist/` existed (or before a frontend rebuild) leaves the previously
    // embedded bundle in the binary: cargo sees no reason to recompile `rmccc`,
    // so `tauri::generate_context!` keeps the old asset map. The window then
    // opens with nothing painted, which looks like a transparent window rather
    // than a build error.
    println!("cargo:rerun-if-changed=../dist");
    println!("cargo:rerun-if-changed=../dist/index.html");

    tauri_build::build();
}
