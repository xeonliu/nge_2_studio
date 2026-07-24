//! Linear EVS storyboard generation and explainable resource association.
//!
//! This module intentionally does not emulate branches or the complete game VM.

use nge2_formats::evs::{DiagnosticSeverity, EvsCommand, EvsScript, FormatDiagnostic};
use nge2_formats::hgar::HgarArchive;
use nge2_formats::voice::voice_ordinal;
use serde::{Deserialize, Serialize};
use specta::Type;

const VARIABLE_TOKENS: &[&str] = &["$w", "$x", "$y", "$d", "$e", "$f"];
const NO_AVATAR: u32 = 0x1000;
const NO_MESSAGE_BOX: u32 = 0x2000;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize, Type)]
#[serde(transparent)]
pub struct SessionId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize, Type)]
#[serde(transparent)]
pub struct GamePath(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ContainerMember {
    pub index: u32,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ResourceRef {
    pub session_id: SessionId,
    pub iso_path: GamePath,
    pub members: Vec<ContainerMember>,
}

impl ResourceRef {
    pub fn child(&self, index: u32, name: impl Into<String>) -> Self {
        let mut members = self.members.clone();
        members.push(ContainerMember {
            index,
            name: name.into(),
        });
        Self {
            session_id: self.session_id.clone(),
            iso_path: self.iso_path.clone(),
            members,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "status", content = "value")]
pub enum Resolution {
    Exact(ResourceRef),
    Variant(Vec<ResourceRef>),
    Missing,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedAsset {
    pub resource: ResourceRef,
    pub name: String,
    pub compressed: bool,
    pub offset: u32,
    pub size: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VisualReference {
    pub command_index: u32,
    pub opcode: u16,
    pub requested: String,
    pub resolution: Resolution,
    pub evidence: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PortraitReference {
    pub archive_path: String,
    pub static_member: String,
    pub atlas_member: String,
    pub resolution: Resolution,
    pub runtime_hidden: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DialogueAudioTrack {
    pub page_index: u32,
    pub voice_id: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DialogueFrame {
    pub command_index: u32,
    pub text: String,
    pub text_bytes: u32,
    pub pages: Vec<String>,
    pub speaker_id: u32,
    pub speaker_name: String,
    pub expression_id: u32,
    pub expression_name: String,
    pub audio_tracks: Vec<DialogueAudioTrack>,
    pub portrait: Option<PortraitReference>,
    pub visuals: Vec<VisualReference>,
    pub diagnostics: Vec<FormatDiagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Storyboard {
    pub frames: Vec<DialogueFrame>,
    pub visual_references: Vec<VisualReference>,
    pub diagnostics: Vec<FormatDiagnostic>,
}

pub fn build_storyboard(
    script: &EvsScript,
    archive: &HgarArchive,
    archive_ref: &ResourceRef,
) -> Storyboard {
    let mut active_visuals: Vec<VisualReference> = Vec::new();
    let mut all_visuals = Vec::new();
    let mut frames = Vec::new();
    let mut diagnostics = script.diagnostics.clone();

    for command in &script.commands {
        match command.opcode {
            0x8c | 0x8d | 0x8e => {
                let visual = resolve_visual(command, archive, archive_ref);
                if matches!(visual.resolution, Resolution::Missing | Resolution::Unsupported) {
                    diagnostics.push(FormatDiagnostic {
                        severity: DiagnosticSeverity::Warning,
                        message: format!("无法关联视觉资源 {}", visual.requested),
                        offset: command.offset,
                    });
                }
                if let Some(slot) = active_visuals
                    .iter_mut()
                    .find(|reference| reference.opcode == command.opcode)
                {
                    *slot = visual.clone();
                } else {
                    active_visuals.push(visual.clone());
                }
                all_visuals.push(visual);
            }
            0x01 => frames.push(dialogue_frame(command, &active_visuals)),
            _ => {}
        }
    }

    Storyboard {
        frames,
        visual_references: all_visuals,
        diagnostics,
    }
}

pub fn select_variant(reference: &mut VisualReference, selected: ResourceRef) -> bool {
    match &reference.resolution {
        Resolution::Variant(candidates) if candidates.contains(&selected) => {
            reference.evidence = "用户从同一 HGAR 的变量候选中选择".into();
            reference.resolution = Resolution::Exact(selected);
            true
        }
        _ => false,
    }
}

fn resolve_visual(
    command: &EvsCommand,
    archive: &HgarArchive,
    archive_ref: &ResourceRef,
) -> VisualReference {
    let requested = command.content.clone().unwrap_or_default().trim().to_owned();
    if requested.is_empty() {
        return VisualReference {
            command_index: command.index,
            opcode: command.opcode,
            requested,
            resolution: Resolution::Unsupported,
            evidence: "命令没有可解析的资源名".into(),
        };
    }

    let exact = archive.entries.iter().find(|entry| {
        candidate_names(entry).iter().any(|candidate| {
            candidate.eq_ignore_ascii_case(&requested)
                || strip_known_extension(candidate).eq_ignore_ascii_case(strip_known_extension(&requested))
        })
    });
    if let Some(entry) = exact {
        return VisualReference {
            command_index: command.index,
            opcode: command.opcode,
            requested,
            resolution: Resolution::Exact(archive_ref.child(entry.index, &entry.display_name)),
            evidence: "在 EVS 所属 HGAR 内精确匹配文件名".into(),
        };
    }

    if VARIABLE_TOKENS.iter().any(|token| requested.contains(token)) {
        let candidates = archive
            .entries
            .iter()
            .filter(|entry| {
                candidate_names(entry)
                    .iter()
                    .any(|candidate| variable_match(&requested, candidate))
            })
            .map(|entry| archive_ref.child(entry.index, &entry.display_name))
            .collect::<Vec<_>>();
        if !candidates.is_empty() {
            return VisualReference {
                command_index: command.index,
                opcode: command.opcode,
                requested,
                resolution: Resolution::Variant(candidates),
                evidence: "仅在 EVS 所属 HGAR 内展开 $w/$x/$y/$d/$e/$f 候选".into(),
            };
        }
    }

    VisualReference {
        command_index: command.index,
        opcode: command.opcode,
        requested,
        resolution: Resolution::Missing,
        evidence: "所属 HGAR 内没有精确文件名或合法变量候选".into(),
    }
}

fn dialogue_frame(command: &EvsCommand, visuals: &[VisualReference]) -> DialogueFrame {
    let avatar_parameter = command.parameters.first().copied().unwrap_or_default();
    let expression_parameter = command.parameters.get(1).copied().unwrap_or_default();
    let audio_parameter = command.parameters.get(2).copied().unwrap_or_default();
    let speaker_id = avatar_parameter & 0x0fff;
    let expression_id = expression_parameter & 0x0fff;
    let runtime_hidden = avatar_parameter & NO_AVATAR != 0;
    let mut frame_diagnostics = command.diagnostics.clone();
    if avatar_parameter & NO_MESSAGE_BOX != 0 {
        frame_diagnostics.push(FormatDiagnostic {
            severity: DiagnosticSeverity::Info,
            message: "运行时隐藏头像和消息框".into(),
            offset: command.offset,
        });
    } else if runtime_hidden {
        frame_diagnostics.push(FormatDiagnostic {
            severity: DiagnosticSeverity::Info,
            message: "保留头像关联，但运行时隐藏头像".into(),
            offset: command.offset,
        });
    }
    let portrait = (speaker_id != 0).then(|| PortraitReference {
        archive_path: format!("/PSP_GAME/USRDIR/face/f{speaker_id:02}_{expression_id:02}.har"),
        static_member: format!("f{speaker_id:02}_{expression_id:02}_1.hpt"),
        atlas_member: format!("f{speaker_id:02}_{expression_id:02}_2.hpt"),
        resolution: Resolution::Missing,
        runtime_hidden,
    });
    let text = command.content.clone().unwrap_or_default();
    let pages = text.split('▽').map(str::to_owned).collect::<Vec<_>>();
    let audio_tracks = pages
        .iter()
        .enumerate()
        .map(|(page_index, _)| {
            let voice_id = (audio_parameter > 0)
                .then(|| audio_parameter.checked_add(page_index as u32))
                .flatten()
                .filter(|voice_id| voice_ordinal(*voice_id).is_some());
            DialogueAudioTrack {
                page_index: page_index as u32,
                voice_id,
            }
        })
        .collect();
    DialogueFrame {
        command_index: command.index,
        pages,
        text,
        text_bytes: command.content_bytes,
        speaker_id,
        speaker_name: speaker_name(speaker_id).into(),
        expression_id,
        expression_name: format!("表情 {expression_id}"),
        audio_tracks,
        portrait,
        visuals: visuals.to_vec(),
        diagnostics: frame_diagnostics,
    }
}

fn candidate_names(entry: &nge2_formats::hgar::HgarEntry) -> Vec<&str> {
    let mut names = vec![entry.display_name.as_str(), entry.short_name.as_str()];
    if let Some(long_name) = entry.long_name.as_deref() {
        names.push(long_name);
    }
    names
}

fn strip_known_extension(name: &str) -> &str {
    for extension in [".hpt", ".zpt", ".har", ".evs"] {
        if name.to_ascii_lowercase().ends_with(extension) {
            return &name[..name.len() - extension.len()];
        }
    }
    name
}

fn variable_match(pattern: &str, candidate: &str) -> bool {
    let pattern = strip_known_extension(pattern).as_bytes();
    let candidate = strip_known_extension(candidate).as_bytes();
    let mut pattern_index = 0;
    let mut candidate_index = 0;
    while pattern_index < pattern.len() {
        let is_variable = pattern[pattern_index] == b'$'
            && pattern_index + 1 < pattern.len()
            && matches!(pattern[pattern_index + 1], b'w' | b'x' | b'y' | b'd' | b'e' | b'f');
        if is_variable {
            if candidate_index >= candidate.len() || !candidate[candidate_index].is_ascii_alphanumeric() {
                return false;
            }
            pattern_index += 2;
            candidate_index += 1;
        } else {
            if candidate_index >= candidate.len()
                || !pattern[pattern_index].eq_ignore_ascii_case(&candidate[candidate_index])
            {
                return false;
            }
            pattern_index += 1;
            candidate_index += 1;
        }
    }
    candidate_index == candidate.len()
}

fn speaker_name(id: u32) -> &'static str {
    match id {
        1 => "碇真嗣",
        2 => "惣流·明日香·兰格雷",
        3 => "绫波丽",
        4 => "葛城美里",
        5 => "碇源堂",
        6 => "冬月耕造",
        7 => "赤木律子",
        8 => "伊吹摩耶",
        9 => "日向诚",
        10 => "青叶茂",
        11 => "加持良治",
        12 => "洞木光",
        13 => "铃原东治",
        14 => "相田剑介",
        15 => "渚薰",
        16 => "Pen Pen",
        17 => "NERV 男职员",
        18 => "NERV 女职员",
        19 => "店员",
        62 => "碇真嗣（剪影）",
        63 => "惣流·明日香·兰格雷（剪影）",
        64 => "绫波丽（剪影）",
        65 => "铃原东治（剪影）",
        66 => "渚薰（剪影）",
        0 => "无角色",
        _ => "未知角色",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nge2_formats::hgar::HgarEntry;

    fn archive(names: &[&str]) -> HgarArchive {
        HgarArchive {
            version: 1,
            entries: names
                .iter()
                .enumerate()
                .map(|(index, name)| HgarEntry {
                    index: index as u32,
                    identifier: index as u32,
                    encoded_identifier: index as u32,
                    short_name: (*name).into(),
                    long_name: None,
                    display_name: (*name).into(),
                    compressed: false,
                    size: 100,
                    content_offset: 32,
                    unknown_first: None,
                    unknown_last: None,
                })
                .collect(),
        }
    }

    fn resource() -> ResourceRef {
        ResourceRef {
            session_id: SessionId("test".into()),
            iso_path: GamePath("/event/a000.har".into()),
            members: Vec::new(),
        }
    }

    fn visual(content: &str) -> EvsCommand {
        EvsCommand {
            index: 0,
            offset: 16,
            opcode: 0x8c,
            opcode_hex: "0x8C".into(),
            name: "VISUAL 0x8C".into(),
            parameters: vec![0],
            content: Some(content.into()),
            content_bytes: content.len() as u32,
            raw_payload: Vec::new(),
            supported: true,
            diagnostics: Vec::new(),
        }
    }

    fn say(content: &str, voice_id: u32) -> EvsCommand {
        EvsCommand {
            index: 0,
            offset: 16,
            opcode: 0x01,
            opcode_hex: "0x01".into(),
            name: "SAY".into(),
            parameters: vec![1, 1, voice_id],
            content: Some(content.into()),
            content_bytes: content.len() as u32,
            raw_payload: Vec::new(),
            supported: true,
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn resolves_exact_before_variants() {
        let archive = archive(&["bg_1.hpt", "bg_2.zpt"]);
        let exact = resolve_visual(&visual("bg_1"), &archive, &resource());
        assert!(matches!(exact.resolution, Resolution::Exact(_)));
        let variants = resolve_visual(&visual("bg_$w"), &archive, &resource());
        assert!(matches!(variants.resolution, Resolution::Variant(ref values) if values.len() == 2));
    }

    #[test]
    fn does_not_expand_unknown_variables() {
        let archive = archive(&["bg_1.hpt"]);
        let result = resolve_visual(&visual("bg_$q"), &archive, &resource());
        assert!(matches!(result.resolution, Resolution::Missing));
    }

    #[test]
    fn maps_each_dialogue_page_to_the_next_voice_id() {
        let frame = dialogue_frame(&say("そうね。▽この子達の為にも。", 28_439), &[]);
        assert_eq!(frame.pages, ["そうね。", "この子達の為にも。"]);
        assert_eq!(
            frame.audio_tracks,
            [
                DialogueAudioTrack { page_index: 0, voice_id: Some(28_439) },
                DialogueAudioTrack { page_index: 1, voice_id: Some(28_440) },
            ]
        );
    }

    #[test]
    fn preserves_silent_voice_table_holes_without_shifting_later_pages() {
        let frame = dialogue_frame(&say("first▽silent▽last", 32_026), &[]);
        assert_eq!(
            frame.audio_tracks,
            [
                DialogueAudioTrack { page_index: 0, voice_id: Some(32_026) },
                DialogueAudioTrack { page_index: 1, voice_id: None },
                DialogueAudioTrack { page_index: 2, voice_id: Some(32_028) },
            ]
        );
    }

    #[test]
    fn maps_speaker_ids_from_the_evs_avatar_table() {
        let expected = [
            (0, "无角色"),
            (1, "碇真嗣"),
            (2, "惣流·明日香·兰格雷"),
            (3, "绫波丽"),
            (4, "葛城美里"),
            (5, "碇源堂"),
            (6, "冬月耕造"),
            (7, "赤木律子"),
            (8, "伊吹摩耶"),
            (9, "日向诚"),
            (10, "青叶茂"),
            (11, "加持良治"),
            (12, "洞木光"),
            (13, "铃原东治"),
            (14, "相田剑介"),
            (15, "渚薰"),
            (16, "Pen Pen"),
            (17, "NERV 男职员"),
            (18, "NERV 女职员"),
            (19, "店员"),
            (62, "碇真嗣（剪影）"),
            (63, "惣流·明日香·兰格雷（剪影）"),
            (64, "绫波丽（剪影）"),
            (65, "铃原东治（剪影）"),
            (66, "渚薰（剪影）"),
        ];

        for (id, name) in expected {
            assert_eq!(speaker_name(id), name, "speaker id {id}");
        }
        assert_eq!(speaker_name(20), "未知角色");
    }
}
