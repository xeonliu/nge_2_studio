mod commands;
mod protocol;
mod session;

use commands::*;
use session::SessionManager;
use specta_typescript::Typescript;
use std::path::PathBuf;
use tauri_specta::{collect_commands, Builder};

pub fn command_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new().commands(collect_commands![
        open_iso,
        close_iso,
        list_directory,
        list_event_archives,
        list_hgar_entries,
        open_evs,
        get_evs_commands,
        get_evs_frames,
        select_evs_variant,
        read_resource_range,
        get_image_preview,
        get_audio_preview,
        export_resource,
        get_session_status
    ])
}

pub fn export_bindings() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/ipc/bindings.ts");
    command_builder().export(Typescript::default(), path)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let manager = SessionManager::default();
    let protocol_manager = manager.clone();
    let commands = command_builder();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(manager)
        .register_uri_scheme_protocol("nge2-preview", move |_context, request| {
            protocol::respond(&protocol_manager, &request)
        })
        .invoke_handler(commands.invoke_handler())
        .run(tauri::generate_context!())
        .expect("error while running NGE2 ISO Studio");
}
