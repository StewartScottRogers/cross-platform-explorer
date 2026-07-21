//! Manual test for the instant-search query core (CPE-831, `cpe_server::index_query`). Walks a folder
//! tree, then filters + ranks every file against a query — the same grammar the future global search
//! overlay will use.
//!
//! Run:  cargo run -p cpe-server --example search_demo -- <root-dir> <query...>
//!
//! Query grammar: name terms (substring or a `*`/`?`/`{a,b}` glob), plus `ext:png,jpg` and `path:foo`
//! filters. All terms AND together. Try, e.g.:
//!   ... -- . cargo                 (files whose name contains "cargo")
//!   ... -- . ext:rs main           (a file named like *main* with a .rs extension)
//!   ... -- . *.toml path:server    (a .toml under a path containing "server")

use cpe_server::index_query::{parse, rank, Candidate};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: search_demo <root-dir> <query...>");
        std::process::exit(2);
    }
    let root = &args[1];
    let query_str = args[2..].join(" ");
    let query = parse(&query_str);

    // Collect (name, path, ext) for every file under root.
    let mut files: Vec<(String, String, String)> = Vec::new();
    walk(std::path::Path::new(root), &mut files);

    let candidates: Vec<Candidate> = files
        .iter()
        .map(|(name, path, ext)| Candidate {
            name: name.as_str(),
            path: path.as_str(),
            ext: ext.as_str(),
        })
        .collect();

    let hits = rank(&query, &candidates);

    println!("query {query_str:?} parsed as {query:?}");
    println!("{} of {} files match (best first):\n", hits.len(), candidates.len());
    for (i, c) in hits.iter().take(50).enumerate() {
        println!("{:>3}. {}", i + 1, c.path);
    }
    if hits.len() > 50 {
        println!("    … and {} more", hits.len() - 50);
    }
}

fn walk(dir: &std::path::Path, out: &mut Vec<(String, String, String)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else {
            let name = entry.file_name().to_string_lossy().to_string();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            out.push((name, path.to_string_lossy().to_string(), ext));
        }
    }
}
