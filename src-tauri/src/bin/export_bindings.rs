//! Typed-binding codegen entry (CPE-812, epic CPE-810). A plain binary — not a `cargo test` — because
//! linking tauri-specta into a libtest binary fails to load on Windows (`STATUS_ENTRYPOINT_NOT_FOUND`, a
//! WebView2 entrypoint skew), while a plain exe loads exactly like the app. Requires both features so the
//! output is the one superset covering the sidecar commands too (CPE-957). Regenerate with:
//!   cargo run --bin export_bindings --features "specta-bindings sidecar-platform"
//! then commit the updated `src/lib/bindings.gen.ts`.

fn main() {
    let out = std::path::Path::new("../src/lib/bindings.gen.ts");
    match app_lib::export_bindings(out) {
        Ok(()) => println!("wrote {}", out.display()),
        Err(e) => {
            eprintln!("export_bindings failed: {e}");
            std::process::exit(1);
        }
    }
}
