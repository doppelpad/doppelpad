use std::{env, fs, path::MAIN_SEPARATOR, path::PathBuf};

fn main() {
    tauri_build::build();
    shadow_gcc_default_manifest();
}

/// MinGW gcc links its `default-manifest.o` into every exe. ld drops it in favour of Tauri's
/// manifest only when it is language-neutral; WinLibs ships it as en-US, so ld warns
/// ("multiple non-default manifests") and keeps both. Put an empty object of that name first
/// on gcc's search path (`-B`) so only Tauri's manifest (Common Controls v6) is linked.
fn shadow_gcc_default_manifest() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows")
        || env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("gnu")
    {
        return;
    }
    let machine: u16 = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86_64") => 0x8664,
        Ok("x86") => 0x014c,
        _ => return,
    };
    let dir = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("gcc-startfiles");
    fs::create_dir_all(&dir).unwrap();
    // A bare COFF file header: no sections, no symbols.
    let mut object = [0u8; 20];
    object[..2].copy_from_slice(&machine.to_le_bytes());
    fs::write(dir.join("default-manifest.o"), object).unwrap();
    println!(
        "cargo:rustc-link-arg-bins=-B{}{MAIN_SEPARATOR}",
        dir.display()
    );
}
