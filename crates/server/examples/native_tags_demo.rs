//! Manual test for the native-metadata bridge (CPE-717: native_meta + native_tags + finder_tags +
//! native_bridge). Pushes tags from CPE's internal store out to a real file's native metadata (an NTFS
//! alternate data stream on Windows / a POSIX xattr on Unix / Finder tags on macOS), then pulls them
//! back — so you can confirm the tags survive *on the file itself*, outside `tags.json`.
//!
//! Run:  cargo run -p cpe-server --example native_tags_demo -- <file> [tag1 tag2 ...]
//!
//! Then verify the metadata with your OS's own tools (the demo prints the exact command).

use std::path::Path;

use cpe_server::native_bridge;
use cpe_server::tags::{tag_store_set, TagStore};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: native_tags_demo <file> [tag1 tag2 ...]");
        std::process::exit(2);
    }
    let path = Path::new(&args[1]);
    let tags: Vec<String> = if args.len() > 2 {
        args[2..].to_vec()
    } else {
        vec!["work".to_string(), "urgent".to_string()]
    };

    println!("native attribute this OS writes tags under: {}", native_bridge::native_name());

    // PUSH: internal tags -> the file's native metadata.
    let key = path.to_string_lossy().to_string();
    let mut store = TagStore::new();
    tag_store_set(&mut store, &key, tags.clone(), "red".to_string());
    if let Err(e) = native_bridge::push(&store, path) {
        eprintln!("push failed: {e}");
        std::process::exit(1);
    }
    println!("PUSHED tags {tags:?} + label \"red\" onto {}", path.display());

    // PULL: read the file's native metadata back into a fresh store.
    let mut fresh = TagStore::new();
    match native_bridge::pull(&mut fresh, path) {
        Ok(changed) => match fresh.get(&key) {
            Some(entry) => println!(
                "PULLED back (changed={changed}): tags {:?}, label {:?}",
                entry.tags(),
                entry.label()
            ),
            None => println!("PULLED back nothing (changed={changed})"),
        },
        Err(e) => {
            eprintln!("pull failed: {e}");
            std::process::exit(1);
        }
    }

    println!("\n--- verify it yourself, independently of this program ---");
    #[cfg(windows)]
    {
        println!("PowerShell:  Get-Item -Path '{p}' -Stream *", p = path.display());
        println!("             Get-Content -Path '{p}' -Stream cpe.tags", p = path.display());
        println!("cmd:         dir /r \"{p}\"   (look for the :cpe.tags:$DATA stream)", p = path.display());
    }
    #[cfg(target_os = "macos")]
    println!("Terminal:    xattr -l '{p}'    (and the tags appear in Finder's Get Info)", p = path.display());
    #[cfg(all(unix, not(target_os = "macos")))]
    println!("Terminal:    getfattr -d '{p}'", p = path.display());
}
