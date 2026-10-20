use std::env;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_version_info();
    }
}

/// The version information Explorer shows under Properties → Details; Tauri embeds the same
/// for doppelpad-gui.exe. Publisher and copyright match `bundle` in
/// gui/src-tauri/tauri.conf.json, the version comes from the workspace.
fn embed_version_info() {
    tauri_winres::WindowsResource::new()
        .set("ProductName", "Doppelpad")
        .set("FileDescription", "Doppelpad command-line tool")
        .set("CompanyName", "Doppelpad contributors")
        .set("LegalCopyright", "Copyright (c) 2026 The Doppelpad contributors")
        .set("OriginalFilename", "doppelpad.exe")
        .compile()
        .expect("compile the version resource");
}
