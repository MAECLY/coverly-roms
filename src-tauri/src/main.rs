// Hide the Windows console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Silences panics caught inside archive decoders (S1); others still print.
    coverly_core::archive::guard::install_panic_hook();
    coverly_roms_lib::run();
}
