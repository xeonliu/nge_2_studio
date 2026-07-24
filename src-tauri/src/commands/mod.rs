use crate::session::{IsoSession, PreviewBlob, SessionManager};
use iso_vfs::{IsoEntry, IsoMetadata};
use nge2_formats::audio::decode_atrac3plus;
use nge2_formats::evs::{EvsCommand, EvsScript, FormatDiagnostic};
use nge2_formats::hgar::{HgarArchive, HgarEntry};
use nge2_formats::hgpt::{HgptDivision, HgptImage, HgptPixelFormat};
use nge2_preview::{
    build_storyboard, select_variant, DialogueFrame, GamePath, Resolution, ResourceRef, SessionId,
    Storyboard, VisualReference,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::cmp::min;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tauri::State;

const MAX_PAGE_SIZE: u32 = 500;
const MAX_RANGE_BYTES: u32 = 256 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PageRequest {
    pub offset: u32,
    pub limit: u32,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub offset: u32,
    pub total: u32,
    pub has_more: bool,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OpenIsoResponse {
    pub session_id: SessionId,
    pub metadata: IsoMetadata,
    pub root: Page<IsoEntry>,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HgarListing {
    pub resource: ResourceRef,
    pub version: u16,
    pub entries: Page<HgarEntry>,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EvsDocument {
    pub resource: ResourceRef,
    pub command_count: u32,
    pub frame_count: u32,
    pub diagnostic_count: u32,
    pub diagnostics: Vec<FormatDiagnostic>,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EvsCommandPage {
    pub page: Page<EvsCommand>,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EvsFramePage {
    pub page: Page<DialogueFrame>,
    pub visual_references: Vec<VisualReference>,
    pub diagnostics: Vec<FormatDiagnostic>,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BinaryChunk {
    pub offset: u32,
    pub total: u32,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImagePreview {
    pub token: String,
    pub mime: String,
    pub width: u32,
    pub height: u32,
    pub pixel_format: HgptPixelFormat,
    pub divisions: Vec<HgptDivision>,
    pub approximate: bool,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AudioPreview {
    pub token: String,
    pub mime: String,
    pub voice_id: u32,
    pub archive: u8,
    pub entry: u32,
    pub sample_rate: u32,
    pub channels: u16,
    pub duration_millis: u32,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub destination: String,
    pub bytes_written: u32,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatus {
    pub cache_bytes: u32,
}

#[tauri::command]
#[specta::specta]
pub fn open_iso(path: String, manager: State<'_, SessionManager>) -> Result<OpenIsoResponse, String> {
    let session = manager.open(&path)?;
    let root = session.iso.list_directory("/").map_err(|error| error.to_string())?;
    Ok(OpenIsoResponse {
        session_id: session.id.clone(),
        metadata: session.iso.metadata(),
        root: page(root, None),
    })
}

#[tauri::command]
#[specta::specta]
pub fn close_iso(session_id: SessionId, manager: State<'_, SessionManager>) -> bool {
    manager.close(&session_id)
}

#[tauri::command]
#[specta::specta]
pub fn list_directory(
    session_id: SessionId,
    path: String,
    page_request: Option<PageRequest>,
    manager: State<'_, SessionManager>,
) -> Result<Page<IsoEntry>, String> {
    let session = manager.get(&session_id)?;
    let entries = session.iso.list_directory(&path).map_err(|error| error.to_string())?;
    Ok(page(entries, page_request))
}

#[tauri::command]
#[specta::specta]
pub fn list_event_archives(
    session_id: SessionId,
    page_request: Option<PageRequest>,
    manager: State<'_, SessionManager>,
) -> Result<Page<IsoEntry>, String> {
    let session = manager.get(&session_id)?;
    let entries = session
        .iso
        .list_directory("/PSP_GAME/USRDIR/event")
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|entry| !entry.is_directory && entry.name.to_ascii_lowercase().ends_with(".har"))
        .collect();
    Ok(page(entries, page_request))
}

#[tauri::command]
#[specta::specta]
pub fn list_hgar_entries(
    resource: ResourceRef,
    page_request: Option<PageRequest>,
    manager: State<'_, SessionManager>,
) -> Result<HgarListing, String> {
    let session = manager.get(&resource.session_id)?;
    let data = session.resource_data(&resource)?;
    let archive = HgarArchive::parse(&data).map_err(|error| error.to_string())?;
    Ok(HgarListing {
        resource,
        version: archive.version,
        entries: page(archive.entries, page_request),
    })
}

#[tauri::command]
#[specta::specta]
pub fn open_evs(
    resource: ResourceRef,
    manager: State<'_, SessionManager>,
) -> Result<EvsDocument, String> {
    let session = manager.get(&resource.session_id)?;
    let (script, storyboard) = load_storyboard(&session, &resource)?;
    Ok(EvsDocument {
        resource,
        command_count: script.commands.len() as u32,
        frame_count: storyboard.frames.len() as u32,
        diagnostic_count: (script.diagnostics.len() + storyboard.diagnostics.len()) as u32,
        diagnostics: storyboard.diagnostics,
    })
}

#[tauri::command]
#[specta::specta]
pub fn get_evs_commands(
    resource: ResourceRef,
    page_request: Option<PageRequest>,
    manager: State<'_, SessionManager>,
) -> Result<EvsCommandPage, String> {
    let session = manager.get(&resource.session_id)?;
    let data = session.resource_data(&resource)?;
    let script = EvsScript::parse(&data).map_err(|error| error.to_string())?;
    Ok(EvsCommandPage {
        page: page(script.commands, page_request),
    })
}

#[tauri::command]
#[specta::specta]
pub fn get_evs_frames(
    resource: ResourceRef,
    page_request: Option<PageRequest>,
    manager: State<'_, SessionManager>,
) -> Result<EvsFramePage, String> {
    let session = manager.get(&resource.session_id)?;
    let (_, storyboard) = load_storyboard(&session, &resource)?;
    Ok(EvsFramePage {
        page: page(storyboard.frames, page_request),
        visual_references: storyboard.visual_references,
        diagnostics: storyboard.diagnostics,
    })
}

#[tauri::command]
#[specta::specta]
pub fn select_evs_variant(
    document: ResourceRef,
    command_index: u32,
    selected: ResourceRef,
    manager: State<'_, SessionManager>,
) -> Result<VisualReference, String> {
    let session = manager.get(&document.session_id)?;
    let (_, mut storyboard) = load_storyboard(&session, &document)?;
    let reference = storyboard
        .visual_references
        .iter_mut()
        .find(|reference| reference.command_index == command_index)
        .ok_or_else(|| "找不到对应的视觉命令".to_string())?;
    if !select_variant(reference, selected.clone()) {
        return Err("所选资源不在该命令的候选列表中".into());
    }
    session.select_variant(&document, command_index, selected);
    Ok(reference.clone())
}

#[tauri::command]
#[specta::specta]
pub fn read_resource_range(
    resource: ResourceRef,
    offset: u32,
    length: u32,
    manager: State<'_, SessionManager>,
) -> Result<BinaryChunk, String> {
    if length > MAX_RANGE_BYTES {
        return Err(format!("单次最多读取 {MAX_RANGE_BYTES} 字节"));
    }
    let session = manager.get(&resource.session_id)?;
    let data = session.resource_data(&resource)?;
    let start = offset as usize;
    let end = start
        .checked_add(length as usize)
        .filter(|end| *end <= data.len())
        .ok_or_else(|| "读取范围超出资源大小".to_string())?;
    Ok(BinaryChunk {
        offset,
        total: data.len() as u32,
        bytes: data[start..end].to_vec(),
    })
}

#[tauri::command]
#[specta::specta]
pub fn get_image_preview(
    resource: ResourceRef,
    manager: State<'_, SessionManager>,
) -> Result<ImagePreview, String> {
    let session = manager.get(&resource.session_id)?;
    let data = session.resource_data(&resource)?;
    let image = HgptImage::parse(&data).map_err(|error| error.to_string())?;
    let png = image.encode_png().map_err(|error| error.to_string())?;
    let token = session.store_preview(
        &resource,
        PreviewBlob {
            mime: "image/png",
            bytes: png,
        },
    );
    Ok(ImagePreview {
        token,
        mime: "image/png".into(),
        width: image.width,
        height: image.height,
        pixel_format: image.pixel_format,
        divisions: image.divisions,
        approximate: false,
    })
}

#[tauri::command]
#[specta::specta]
pub fn get_audio_preview(
    document: ResourceRef,
    voice_id: u32,
    manager: State<'_, SessionManager>,
) -> Result<AudioPreview, String> {
    let session = manager.get(&document.session_id)?;
    let clip = session.voice_clip(voice_id)?;
    let decoded = decode_atrac3plus(&clip.bytes).map_err(|error| error.to_string())?;
    let token = session.store_preview(
        &clip.resource,
        PreviewBlob {
            mime: "audio/wav",
            bytes: decoded.wav,
        },
    );
    Ok(AudioPreview {
        token,
        mime: "audio/wav".into(),
        voice_id,
        archive: clip.archive,
        entry: clip.entry,
        sample_rate: decoded.sample_rate,
        channels: decoded.channels,
        duration_millis: decoded.duration_millis,
    })
}

#[tauri::command]
#[specta::specta]
pub fn export_resource(
    resource: ResourceRef,
    destination: String,
    manager: State<'_, SessionManager>,
) -> Result<ExportResult, String> {
    let session = manager.get(&resource.session_id)?;
    let data = session.resource_data(&resource)?;
    let destination_path = Path::new(&destination);
    if destination_path.is_dir() {
        return Err("导出目标必须是文件路径".into());
    }
    fs::write(destination_path, data.as_slice()).map_err(|error| error.to_string())?;
    Ok(ExportResult {
        destination,
        bytes_written: data.len() as u32,
    })
}

#[tauri::command]
#[specta::specta]
pub fn get_session_status(
    session_id: SessionId,
    manager: State<'_, SessionManager>,
) -> Result<SessionStatus, String> {
    let session = manager.get(&session_id)?;
    Ok(SessionStatus {
        cache_bytes: session.cache_bytes(),
    })
}

fn load_storyboard(
    session: &Arc<IsoSession>,
    resource: &ResourceRef,
) -> Result<(EvsScript, Storyboard), String> {
    let evs_data = session.resource_data(resource)?;
    let script = EvsScript::parse(&evs_data).map_err(|error| error.to_string())?;
    if resource.members.is_empty() {
        return Err("EVS 资源必须位于 HGAR 成员中".into());
    }
    let mut archive_ref = resource.clone();
    archive_ref.members.pop();
    let archive_data = session.resource_data(&archive_ref)?;
    let archive = HgarArchive::parse(&archive_data).map_err(|error| error.to_string())?;
    let mut storyboard = build_storyboard(&script, &archive, &archive_ref);
    apply_selections(session, resource, &mut storyboard);
    resolve_portraits(session, &mut storyboard);
    Ok((script, storyboard))
}

fn apply_selections(session: &IsoSession, document: &ResourceRef, storyboard: &mut Storyboard) {
    for reference in &mut storyboard.visual_references {
        if let Some(selected) = session.selected_variant(document, reference.command_index) {
            select_variant(reference, selected);
        }
    }
    for frame in &mut storyboard.frames {
        for reference in &mut frame.visuals {
            if let Some(selected) = session.selected_variant(document, reference.command_index) {
                select_variant(reference, selected);
            }
        }
    }
}

fn resolve_portraits(session: &IsoSession, storyboard: &mut Storyboard) {
    for frame in &mut storyboard.frames {
        let Some(portrait) = frame.portrait.as_mut() else {
            continue;
        };
        let archive_ref = ResourceRef {
            session_id: session.id.clone(),
            iso_path: GamePath(portrait.archive_path.clone()),
            members: Vec::new(),
        };
        let Ok(data) = session.resource_data(&archive_ref) else {
            continue;
        };
        let Ok(archive) = HgarArchive::parse(&data) else {
            portrait.resolution = Resolution::Unsupported;
            continue;
        };
        if let Some(entry) = archive.entries.iter().find(|entry| {
            entry.display_name.eq_ignore_ascii_case(&portrait.static_member)
                || entry.short_name.eq_ignore_ascii_case(&portrait.static_member)
        }) {
            portrait.resolution = Resolution::Exact(archive_ref.child(entry.index, &entry.display_name));
        }
    }
}

fn page<T>(items: Vec<T>, request: Option<PageRequest>) -> Page<T> {
    let total = items.len() as u32;
    let Some(request) = request else {
        return Page {
            items,
            offset: 0,
            total,
            has_more: false,
        };
    };
    let offset = min(request.offset, total);
    let limit = request.limit.clamp(1, MAX_PAGE_SIZE);
    let end = min(total, offset.saturating_add(limit));
    let items = items
        .into_iter()
        .skip(offset as usize)
        .take((end - offset) as usize)
        .collect();
    Page {
        items,
        offset,
        total,
        has_more: end < total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omitted_page_request_returns_the_complete_collection() {
        let result = page((0..668).collect::<Vec<_>>(), None);

        assert_eq!(result.items.len(), 668);
        assert_eq!(result.offset, 0);
        assert_eq!(result.total, 668);
        assert!(!result.has_more);
    }

    #[test]
    fn explicit_page_request_keeps_the_safety_limit() {
        let first = page(
            (0..668).collect::<Vec<_>>(),
            Some(PageRequest {
                offset: 0,
                limit: 1_000,
            }),
        );
        let second = page(
            (0..668).collect::<Vec<_>>(),
            Some(PageRequest {
                offset: 500,
                limit: 500,
            }),
        );

        assert_eq!(first.items.len(), 500);
        assert!(first.has_more);
        assert_eq!(second.items.len(), 168);
        assert!(!second.has_more);
    }
}
