use crate::{u16_le, u32_le, FormatError};
use encoding_rs::SHIFT_JIS;
use serde::Serialize;
use specta::Type;

const CONTENT_OPCODES: &[u16] = &[0x01, 0x8c, 0x8d, 0x8e, 0x95, 0xa3];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EvsScript {
    pub commands: Vec<EvsCommand>,
    pub diagnostics: Vec<FormatDiagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EvsCommand {
    pub index: u32,
    pub offset: u32,
    pub opcode: u16,
    pub opcode_hex: String,
    pub name: String,
    pub category: EvsCommandCategory,
    pub description: String,
    pub parameters: Vec<u32>,
    pub parameter_names: Vec<String>,
    pub content: Option<String>,
    pub options: Vec<String>,
    pub content_bytes: u32,
    pub raw_payload: Vec<u8>,
    pub supported: bool,
    pub diagnostics: Vec<FormatDiagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum EvsCommandCategory {
    Dialogue,
    Flow,
    Visual,
    Audio,
    Choice,
    Timing,
    Event,
    Extension,
    State,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FormatDiagnostic {
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub offset: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

impl EvsScript {
    pub fn parse(data: &[u8]) -> Result<Self, FormatError> {
        if data.get(..4) != Some(b".EVS") {
            return Err(invalid("missing .EVS signature"));
        }
        let count = u32_le(data, 4)? as usize;
        if count > 1_000_000 || 8usize.saturating_add(count.saturating_mul(4)) > data.len() {
            return Err(invalid("invalid command count or offset table"));
        }
        let mut offsets = Vec::with_capacity(count);
        for index in 0..count {
            offsets.push(u32_le(data, 8 + index * 4)? as usize);
        }
        let mut commands = Vec::with_capacity(count);
        let mut diagnostics = Vec::new();
        for (index, offset) in offsets.iter().copied().enumerate() {
            let next_offset = offsets.get(index + 1).copied().unwrap_or(data.len());
            match parse_command(data, index, offset, next_offset) {
                Ok(command) => commands.push(command),
                Err(message) => {
                    diagnostics.push(FormatDiagnostic {
                        severity: DiagnosticSeverity::Error,
                        message: message.clone(),
                        offset: offset as u32,
                    });
                    commands.push(EvsCommand {
                        index: index as u32,
                        offset: offset as u32,
                        opcode: data
                            .get(offset..offset + 2)
                            .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
                            .unwrap_or(0xffff),
                        opcode_hex: data
                            .get(offset..offset + 2)
                            .map(|bytes| {
                                format!("0x{:02X}", u16::from_le_bytes([bytes[0], bytes[1]]))
                            })
                            .unwrap_or_else(|| "--".into()),
                        name: "损坏命令".into(),
                        category: EvsCommandCategory::Unknown,
                        description: "命令记录损坏，无法确定运行时语义".into(),
                        parameters: Vec::new(),
                        parameter_names: Vec::new(),
                        content: None,
                        options: Vec::new(),
                        content_bytes: 0,
                        raw_payload: data
                            .get(offset..next_offset.min(data.len()))
                            .unwrap_or(&[])
                            .to_vec(),
                        supported: false,
                        diagnostics: vec![FormatDiagnostic {
                            severity: DiagnosticSeverity::Error,
                            message,
                            offset: offset as u32,
                        }],
                    });
                }
            }
        }
        Ok(Self {
            commands,
            diagnostics,
        })
    }
}

fn parse_command(
    data: &[u8],
    index: usize,
    offset: usize,
    next_offset: usize,
) -> Result<EvsCommand, String> {
    if offset < 8 || offset + 4 > data.len() || next_offset < offset + 4 || next_offset > data.len()
    {
        return Err("命令 offset 超出文件范围".into());
    }
    let opcode = u16_le(data, offset).map_err(|error| error.to_string())?;
    let declared_size = u16_le(data, offset + 2).map_err(|error| error.to_string())? as usize;
    let available = next_offset - (offset + 4);
    if declared_size > available {
        return Err(format!(
            "命令声明 {declared_size} 字节，但在下一 offset 前只有 {available} 字节"
        ));
    }
    let payload = &data[offset + 4..offset + 4 + declared_size];
    let parameter_count = parameter_count(opcode);
    let supported = parameter_count.is_some();
    let mut diagnostics = Vec::new();
    let mut parameters = Vec::new();
    let mut content = None;
    let mut options = Vec::new();
    let mut content_bytes = 0u32;

    if let Some(parameter_count) = parameter_count {
        let parameter_bytes = usize::from(parameter_count) * 4;
        if parameter_bytes > payload.len() {
            diagnostics.push(FormatDiagnostic {
                severity: DiagnosticSeverity::Error,
                message: format!(
                    "参数需要 {parameter_bytes} 字节，payload 仅 {} 字节",
                    payload.len()
                ),
                offset: offset as u32,
            });
        } else {
            for parameter in 0..usize::from(parameter_count) {
                parameters.push(u32_le(payload, parameter * 4).map_err(|error| error.to_string())?);
            }
            let trailing = &payload[parameter_bytes..];
            if CONTENT_OPCODES.contains(&opcode) {
                let raw_text = trailing
                    .strip_suffix(&[0])
                    .unwrap_or(trailing)
                    .iter()
                    .copied()
                    .collect::<Vec<_>>();
                let raw_text = raw_text
                    .iter()
                    .rposition(|byte| *byte != 0)
                    .map(|last| &raw_text[..=last])
                    .unwrap_or(&[]);
                let (decoded, _, had_errors) = SHIFT_JIS.decode(raw_text);
                content = Some(decoded.into_owned());
                if opcode == 0x95 {
                    options = content
                        .as_deref()
                        .unwrap_or_default()
                        .split('／')
                        .map(str::trim)
                        .filter(|option| !option.is_empty())
                        .map(str::to_owned)
                        .collect();
                }
                content_bytes = raw_text.len() as u32;
                if had_errors {
                    diagnostics.push(FormatDiagnostic {
                        severity: DiagnosticSeverity::Warning,
                        message: "文本包含无法解码的 EVA-SJIS 字节，已用替代字符显示".into(),
                        offset: (offset + 4 + parameter_bytes) as u32,
                    });
                }
            } else if !trailing.is_empty() {
                diagnostics.push(FormatDiagnostic {
                    severity: DiagnosticSeverity::Warning,
                    message: format!("已知 opcode 带有 {} 字节未说明 payload", trailing.len()),
                    offset: (offset + 4 + parameter_bytes) as u32,
                });
            }
        }
    } else {
        diagnostics.push(FormatDiagnostic {
            severity: DiagnosticSeverity::Warning,
            message: "未知 opcode，已保留原始 payload 并继续解析".into(),
            offset: offset as u32,
        });
    }

    Ok(EvsCommand {
        index: index as u32,
        offset: offset as u32,
        opcode,
        opcode_hex: format!("0x{opcode:02X}"),
        name: opcode_name(opcode).into(),
        category: opcode_category(opcode),
        description: opcode_description(opcode).into(),
        parameters,
        parameter_names: parameter_names(opcode)
            .iter()
            .map(|name| (*name).into())
            .collect(),
        content,
        options,
        content_bytes,
        raw_payload: payload.to_vec(),
        supported,
        diagnostics,
    })
}

pub fn parameter_count(opcode: u16) -> Option<u8> {
    let row: &[Option<u8>] = match opcode >> 4 {
        0x0 => &[
            None,
            Some(3),
            Some(1),
            Some(1),
            Some(1),
            Some(1),
            Some(1),
            Some(1),
            Some(1),
            Some(1),
            Some(3),
            Some(1),
            Some(1),
            Some(1),
            Some(1),
            Some(1),
        ],
        0x1 => &[
            Some(2),
            Some(1),
            Some(1),
            Some(1),
            Some(1),
            Some(3),
            Some(3),
            Some(3),
            Some(3),
            Some(2),
            Some(2),
            Some(2),
            Some(2),
            Some(2),
            Some(2),
            Some(2),
        ],
        0x2 => &[
            Some(4),
            Some(4),
            Some(2),
            Some(2),
            Some(2),
            Some(3),
            Some(4),
            Some(4),
            Some(3),
            Some(4),
            Some(4),
            Some(3),
            Some(4),
            Some(3),
            Some(3),
            Some(2),
        ],
        0x3 => &[
            Some(2),
            Some(2),
            Some(2),
            Some(3),
            Some(3),
            Some(3),
            Some(3),
            Some(3),
            Some(4),
            Some(3),
            Some(2),
            Some(2),
            Some(2),
            Some(2),
            Some(2),
            Some(2),
        ],
        0x4 => &[Some(2); 16],
        0x5 => &[
            Some(2),
            Some(2),
            Some(2),
            Some(2),
            Some(2),
            Some(3),
            Some(2),
            Some(1),
            Some(1),
            Some(1),
            Some(1),
            Some(1),
            Some(1),
            Some(1),
            Some(1),
            Some(1),
        ],
        0x6 => &[
            Some(1),
            Some(1),
            Some(1),
            Some(2),
            Some(2),
            Some(1),
            Some(1),
            Some(1),
            Some(1),
            Some(0),
            Some(0),
            Some(0),
            Some(0),
            Some(0),
            Some(0),
            Some(0),
        ],
        0x7 => &[
            Some(0),
            Some(0),
            Some(0),
            Some(0),
            Some(0),
            Some(0),
            Some(0),
            Some(0),
            Some(2),
            Some(1),
            Some(0),
            Some(0),
            Some(0),
            Some(1),
            Some(1),
            Some(1),
        ],
        0x8 => &[
            Some(0),
            Some(0),
            Some(1),
            Some(1),
            Some(1),
            Some(1),
            Some(0),
            Some(1),
            Some(7),
            Some(2),
            Some(6),
            Some(0),
            Some(1),
            Some(1),
            Some(1),
            Some(1),
        ],
        0x9 => &[
            Some(1),
            Some(1),
            Some(1),
            Some(1),
            Some(1),
            Some(0),
            Some(0),
            Some(0),
            Some(1),
            Some(2),
            Some(2),
            Some(2),
            Some(2),
            Some(1),
            Some(1),
            Some(2),
        ],
        0xa => &[
            Some(1),
            Some(1),
            Some(1),
            Some(1),
            Some(1),
            Some(1),
            Some(2),
            Some(3),
            Some(3),
            Some(1),
            Some(1),
            Some(1),
            Some(1),
            Some(1),
            Some(1),
            Some(2),
        ],
        0xb => &[
            Some(1),
            Some(2),
            Some(3),
            Some(3),
            Some(3),
            Some(1),
            Some(2),
            Some(3),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
        _ => return None,
    };
    row[(opcode & 0x0f) as usize]
}

pub fn opcode_name(opcode: u16) -> &'static str {
    match opcode {
        0x01 => "DIALOGUE",
        0x02 => "DIALOGUE_BLOCK_CURRENT",
        0x03 => "DIALOGUE_BLOCK_OTHERS",
        0x04 => "DIALOGUE_BLOCK_ALL",
        0x05..=0x08 => "FILTERED_DIALOGUE_BLOCK",
        0x09 => "RANDOM_DIALOGUE_BLOCK",
        0x0a => "DIALOGUE_SEQUENCE",
        0x14 => "JUMP",
        0x15..=0x56 => "BRANCH",
        0x7b => "END_EVENT",
        0x7d..=0x86 => "RETURN_EVENT",
        0x87 => "EXTENSION",
        0x8c => "SET_BACKGROUND",
        0x8d => "SET_PICTURE",
        0x8e => "SET_TELOP",
        0x8f => "EVENT_EFFECT",
        0x90 => "WAIT_MS",
        0x91 => "STOP_SOUND_EFFECT",
        0x92 => "PLAY_SOUND_EFFECT",
        0x93 => "PLAY_MUSIC",
        0x94 => "SET_MUSIC_VOLUME",
        0x95 => "CHOICE",
        0xa3 => "RESOURCE_HINT",
        _ if parameter_count(opcode).is_some() => "COMMAND",
        _ => "UNKNOWN",
    }
}

pub fn opcode_category(opcode: u16) -> EvsCommandCategory {
    match opcode {
        0x01..=0x0a => EvsCommandCategory::Dialogue,
        0x14..=0x56 => EvsCommandCategory::Flow,
        0x7b..=0x86 => EvsCommandCategory::Event,
        0x87 => EvsCommandCategory::Extension,
        0x8c..=0x8f => EvsCommandCategory::Visual,
        0x90 => EvsCommandCategory::Timing,
        0x91..=0x94 => EvsCommandCategory::Audio,
        0x95 => EvsCommandCategory::Choice,
        _ if parameter_count(opcode).is_some() => EvsCommandCategory::State,
        _ => EvsCommandCategory::Unknown,
    }
}

pub fn opcode_description(opcode: u16) -> &'static str {
    match opcode {
        0x01 => "显示对话；三个参数分别控制角色/头像标志、表情标志和首个语音 ID",
        0x02 => "扫描随后 N 条命令，只执行属于当前角色的有效对话",
        0x03 => "扫描随后 N 条命令，只执行不属于当前角色的有效对话",
        0x04 => "扫描随后 N 条命令，执行所有有效角色对话",
        0x05..=0x08 => "按角色有效性和当前角色过滤随后 N 条对话命令",
        0x09 => "从随后 N 条有效角色对话中选择一条执行",
        0x0a => "当指定角色有效时，执行随后 N 条对话或视觉命令",
        0x14 => "将命令索引按有符号相对偏移跳转",
        0x15..=0x56 => "根据角色、事件或游戏状态在两个相对偏移之间分支",
        0x7b => "设置事件停止标志并以默认结果退出脚本",
        0x7d..=0x86 => "设置事件返回结构和停止标志，退出脚本",
        0x87 => "调用按 ID 分派的游戏专用扩展；包含教程 HUD、菜单和存档等操作",
        0x8c => "替换背景层资源；空名称会清除背景，参数是转场标志",
        0x8d => "替换图片/CG 层资源；空名称会清除图片，参数是转场标志",
        0x8e => "替换 telop 覆盖层资源；空名称会清除覆盖层",
        0x8f => "启用、切换或释放事件专用视觉效果",
        0x90 => "等待指定毫秒数；运行时按 60 Hz 换算为帧数",
        0x91 => "停止指定音效；-1 会停止所有已跟踪音效",
        0x92 => "播放指定音效 ID，并在运行时跟踪其播放句柄",
        0x93 => "播放音乐 ID；非正值会停止当前音乐",
        0x94 => "设置音乐音量；运行时还识别 0x8000 和 0x8001 两个预设值",
        0x95 => "显示选择菜单；正文以全角斜线分隔，运行时最多读取四个选项",
        0xa3 => "资源提示记录；当前版本的运行时 handler 不读取 payload，仅前进命令索引",
        _ if parameter_count(opcode).is_some() => {
            "运行时存在 handler 且参数长度已确认，具体游戏状态语义尚未命名"
        }
        _ => "分派表中没有可安全解析的参数布局",
    }
}

pub fn parameter_names(opcode: u16) -> &'static [&'static str] {
    match opcode {
        0x01 => &["speakerFlags", "expressionFlags", "voiceId"],
        0x02..=0x09 => &["entryCount"],
        0x0a => &["entryCount", "firstCharacterId", "secondCharacterId"],
        0x14 => &["relativeOffset"],
        0x15..=0x18 => &["trueOffset", "falseOffset", "conditionValue"],
        0x19 => &["trueOffset", "falseOffset"],
        0x7d..=0x7f | 0x85 => &["resultCode"],
        0x87 => &["extensionId"],
        0x8c | 0x8d => &["transitionFlags"],
        0x8e => &["displayFlags"],
        0x8f => &["effectMode"],
        0x90 => &["milliseconds"],
        0x91 | 0x92 => &["soundId"],
        0x93 => &["musicId"],
        0x94 => &["volume"],
        0xa3 => &["unused"],
        _ => &[],
    }
}

fn invalid(message: &str) -> FormatError {
    FormatError::Invalid {
        format: "EVS",
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(opcode: u16, payload: &[u8]) -> Vec<u8> {
        let mut value = opcode.to_le_bytes().to_vec();
        value.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        value.extend_from_slice(payload);
        while value.len() % 4 != 0 {
            value.push(0);
        }
        value
    }

    #[test]
    fn unknown_opcode_does_not_stop_following_commands() {
        let unknown = command(0xfe, &[1, 2, 3]);
        let mut say_payload = Vec::new();
        say_payload.extend_from_slice(&1u32.to_le_bytes());
        say_payload.extend_from_slice(&2u32.to_le_bytes());
        say_payload.extend_from_slice(&3u32.to_le_bytes());
        say_payload.extend_from_slice(b"hello\0");
        let say = command(1, &say_payload);
        let first_offset = 16u32;
        let second_offset = first_offset + unknown.len() as u32;
        let mut data = b".EVS".to_vec();
        data.extend_from_slice(&2u32.to_le_bytes());
        data.extend_from_slice(&first_offset.to_le_bytes());
        data.extend_from_slice(&second_offset.to_le_bytes());
        data.extend_from_slice(&unknown);
        data.extend_from_slice(&say);

        let script = EvsScript::parse(&data).unwrap();
        assert_eq!(script.commands.len(), 2);
        assert!(!script.commands[0].supported);
        assert_eq!(script.commands[0].raw_payload, [1, 2, 3]);
        assert_eq!(script.commands[1].content.as_deref(), Some("hello"));
    }

    #[test]
    fn choice_text_is_split_into_runtime_options() {
        let (encoded, _, had_errors) = SHIFT_JIS.encode("ONE／TWO／THREE");
        assert!(!had_errors);
        let mut payload = encoded.into_owned();
        payload.push(0);
        let choice = command(0x95, &payload);
        let first_offset = 12u32;
        let mut data = b".EVS".to_vec();
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&first_offset.to_le_bytes());
        data.extend_from_slice(&choice);

        let script = EvsScript::parse(&data).unwrap();
        let parsed = &script.commands[0];
        assert_eq!(parsed.name, "CHOICE");
        assert_eq!(parsed.category, EvsCommandCategory::Choice);
        assert!(parsed.parameters.is_empty());
        assert_eq!(parsed.options, ["ONE", "TWO", "THREE"]);
    }

    #[test]
    fn parameter_layout_covers_the_runtime_dispatch_range() {
        for opcode in 0x01..=0xb7 {
            assert!(
                parameter_count(opcode).is_some(),
                "missing payload layout for opcode 0x{opcode:02X}"
            );
        }
        assert_eq!(parameter_count(0xb8), None);
    }

    #[test]
    fn ida_confirmed_commands_expose_semantic_metadata() {
        assert_eq!(opcode_name(0x8c), "SET_BACKGROUND");
        assert_eq!(opcode_category(0x8c), EvsCommandCategory::Visual);
        assert_eq!(parameter_names(0x8c), ["transitionFlags"]);

        assert_eq!(opcode_name(0x92), "PLAY_SOUND_EFFECT");
        assert_eq!(opcode_category(0x92), EvsCommandCategory::Audio);
        assert_eq!(parameter_names(0x92), ["soundId"]);

        assert_eq!(opcode_name(0xa3), "RESOURCE_HINT");
        assert!(opcode_description(0xa3).contains("不读取 payload"));
    }
}
