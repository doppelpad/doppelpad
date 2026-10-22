use std::{env, fs, path::MAIN_SEPARATOR, path::PathBuf};

/// The commands main.rs registers. Tauri makes an `allow-<command>` permission of each, which
/// the capabilities grant per window: the editor all of them (permissions/editor.toml), the
/// overlays only what their pages call. A test checks this list against main.rs.
const COMMANDS: &[&str] = &[
    "catalog",
    "library_scan",
    "game_create",
    "game_update",
    "game_delete",
    "game_duplicate",
    "running_programs",
    "game_set_picture",
    "game_picture",
    "foreground_program",
    "installed_games",
    "game_create_from_store",
    "game_create_template",
    "config_from_template",
    "open_driver_page",
    "game_export",
    "game_import",
    "preset_list",
    "preset_save",
    "preset_load",
    "sheet_save",
    "config_load",
    "config_save",
    "config_check",
    "config_versions",
    "config_create",
    "config_duplicate",
    "config_delete",
    "config_import",
    "reveal",
    "apply_to_slot",
    "select_slot",
    "clear_slot",
    "own_slots",
    "set_remap",
    "set_auto_switch",
    "set_lights",
    "set_blocked",
    "set_groups",
    "set_power_off_after",
    "power_off",
    "set_layout",
    "forget_layout",
    "set_dsu",
    "set_runtime_options",
    "set_http",
    "new_http_token",
    "set_preferred_device",
    "refresh_drivers",
    "rescan_devices",
    "hotkeys_get",
    "hotkeys_set",
    "hotkeys_pause",
    "test_turn",
    "gyro_calibrate",
    "gyro_auto",
    "gyro_reset",
    "rewasd_status",
    "rewasd_quit",
    "live_subscribe",
    "settings_get",
    "settings_set",
    "autostart_get",
    "autostart_set",
    "window_show",
    "library_place",
    "library_target",
    "library_move",
    "log_file",
    "take_notices",
    "take_open_files",
    "overlay_ready",
    "osd_test",
    "osd_monitors",
];

fn main() {
    let app = tauri_build::AppManifest::new()
        .commands(COMMANDS)
        .permissions_path_pattern("./permissions/*.toml");
    if let Err(e) = tauri_build::try_build(tauri_build::Attributes::new().app_manifest(app)) {
        panic!("{e:#}");
    }
    shadow_gcc_default_manifest();
    list_catalogs();
}

/// The frontend's catalogs (`src/lib/i18n/<language>.json`) as `(language, text)` pairs for the
/// tray's texts (tray.rs): a new language there needs no change here.
fn list_catalogs() {
    let dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../src/lib/i18n");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut catalogs: Vec<(String, PathBuf)> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.unwrap().path();
            let lang = path.file_stem()?.to_str()?.to_owned();
            (path.extension()? == "json").then_some((lang, path))
        })
        .collect();
    catalogs.sort();
    let entries: String = catalogs
        .iter()
        .map(|(lang, path)| {
            format!(
                "    ({lang:?}, include_str!({:?})),\n",
                path.display().to_string()
            )
        })
        .collect();
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("catalogs.rs");
    fs::write(out, format!("&[\n{entries}]\n")).unwrap();
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
