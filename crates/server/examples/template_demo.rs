//! Manual test for folder templates & scaffolding (CPE-835, `cpe_server::folder_template`). Captures a
//! real folder's structure into a template, prints the template JSON, then stamps it into a new
//! destination with `{token}` substitution — so you can inspect the freshly-scaffolded folder on disk.
//!
//! Run:  cargo run -p cpe-server --example template_demo -- <source-folder> <dest-folder> [name=Value date=...]
//!
//! Tokens in the source's folder/file names and (text) file contents — like `{name}` or `{date}` — are
//! replaced by the `key=value` pairs you pass. Stamping is path-safe and refuses to overwrite files.

use std::collections::BTreeMap;
use std::path::Path;

use cpe_server::folder_template::{capture, stamp};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: template_demo <source-folder> <dest-folder> [name=Value date=2026-07-21 ...]");
        std::process::exit(2);
    }
    let src = Path::new(&args[1]);
    let dest = Path::new(&args[2]);

    let mut vars = BTreeMap::new();
    for kv in &args[3..] {
        if let Some((k, v)) = kv.split_once('=') {
            vars.insert(k.to_string(), v.to_string());
        }
    }
    if vars.is_empty() {
        vars.insert("name".to_string(), "Demo".to_string());
    }

    let template = match capture(src, "demo-template") {
        Ok(t) => t,
        Err(e) => {
            eprintln!("capture failed: {e}");
            std::process::exit(1);
        }
    };
    println!(
        "CAPTURED {} → a template with {} top-level node(s):\n",
        src.display(),
        template.nodes.len()
    );
    println!("{}\n", serde_json::to_string_pretty(&template).unwrap());

    match stamp(&template, dest, &vars) {
        Ok(created) => {
            println!("STAMPED into {} with vars {vars:?}:", dest.display());
            for p in &created {
                println!("  + {}", p.display());
            }
            println!("\nOpen {} to see the scaffolded structure.", dest.display());
        }
        Err(e) => {
            eprintln!("stamp failed: {e}");
            std::process::exit(1);
        }
    }
}
