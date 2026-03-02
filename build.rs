// build.rs — generates swt.1 at compile time using clap_mangen.
//
// The CLI definition is pulled in via include! so it is available to both
// this build script and the main binary without duplication.

// src/cli.rs depends only on clap + std, both available here.
// It also brings `use std::path::PathBuf` into scope, so we don't repeat it.
include!("src/cli.rs");

fn main() {
    println!("cargo:rerun-if-changed=src/cli.rs");

    let cmd = <Cli as clap::CommandFactory>::command();
    let man = clap_mangen::Man::new(cmd);

    let mut buf = Vec::new();
    man.render(&mut buf).expect("failed to render man page");

    // Write to OUT_DIR — the canonical location Cargo exposes for build outputs.
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(out_dir.join("swt.1"), &buf).expect("failed to write swt.1 to OUT_DIR");

    // Also mirror to target/man/swt.1 so the Makefile can find it at a
    // stable path without needing to parse cargo metadata.
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let man_dir = manifest_dir.join("target").join("man");
    std::fs::create_dir_all(&man_dir).ok();
    std::fs::write(man_dir.join("swt.1"), &buf).ok(); // best-effort
}
