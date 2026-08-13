//! `create-sidecar <name>` — scaffold a new, contract-compliant sidecar crate under
//! `sidecar/<name>/` (CPE-303). Thin CLI over `sidecar_host::scaffold::scaffold`.

use std::io::Write;
use std::path::Path;

use sidecar_host::scaffold::scaffold;

fn main() {
    let name = std::env::args().nth(1).unwrap_or_default();
    if name.is_empty() {
        eprintln!("usage: create-sidecar <name>");
        std::process::exit(2);
    }

    let files = match scaffold(&name) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    // CPE-1705: was `if root.exists()`, which folds every stat failure into `false` — so an existing
    // sidecar crate the scaffolder could not stat read as "nothing there" and the `File::create` +
    // `write_all` loop below overwrote a real crate's Cargo.toml, lib.rs and manifest. Dev-only CLI and
    // low severity, but it is the same shape as the rest of this ticket, and a scaffolder silently
    // flattening someone's in-progress sidecar is not a good afternoon.
    let root = Path::new("sidecar").join(&name);
    match root.try_exists() {
        Ok(false) => {}
        Ok(true) => {
            eprintln!("error: {} already exists", root.display());
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!(
                "error: could not check what is at {}, so nothing was written — refusing to guess \
                 rather than risk overwriting an existing sidecar: {e}",
                root.display()
            );
            std::process::exit(1);
        }
    }

    for (rel, content) in files {
        let path = root.join(&rel);
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!("error: {}: {e}", parent.display());
                std::process::exit(1);
            }
        }
        match std::fs::File::create(&path).and_then(|mut f| f.write_all(content.as_bytes())) {
            Ok(()) => println!("created {}", path.display()),
            Err(e) => {
                eprintln!("error: {}: {e}", path.display());
                std::process::exit(1);
            }
        }
    }

    println!(
        "Done. Add `{name}` to the CI 'sidecar' job, then validate it with the \
         conformance kit."
    );
}
