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
    pub parameters: Vec<u32>,
    pub content: Option<String>,
    pub content_bytes: u32,
    pub raw_payload: Vec<u8>,
    pub supported: bool,
    pub diagnostics: Vec<FormatDiagnostic>,
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
                            .map(|bytes| format!("0x{:02X}", u16::from_le_bytes([bytes[0], bytes[1]])))
                            .unwrap_or_else(|| "--".into()),
                        name: "损坏命令".into(),
                        parameters: Vec::new(),
                        content: None,
                        content_bytes: 0,
                        raw_payload: data.get(offset..next_offset.min(data.len())).unwrap_or(&[]).to_vec(),
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
    if offset < 8 || offset + 4 > data.len() || next_offset < offset + 4 || next_offset > data.len() {
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
    let mut content_bytes = 0u32;

    if let Some(parameter_count) = parameter_count {
        let parameter_bytes = usize::from(parameter_count) * 4;
        if parameter_bytes > payload.len() {
            diagnostics.push(FormatDiagnostic {
                severity: DiagnosticSeverity::Error,
                message: format!("参数需要 {parameter_bytes} 字节，payload 仅 {} 字节", payload.len()),
                offset: offset as u32,
            });
        } else {
            for parameter in 0..usize::from(parameter_count) {
                parameters.push(
                    u32_le(payload, parameter * 4).map_err(|error| error.to_string())?,
                );
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
        parameters,
        content,
        content_bytes,
        raw_payload: payload.to_vec(),
        supported,
        diagnostics,
    })
}

pub fn parameter_count(opcode: u16) -> Option<u8> {
    let row: &[Option<u8>] = match opcode >> 4 {
        0x0 => &[None, Some(3), Some(1), None, Some(1), Some(1), Some(1), Some(1), Some(1), Some(1), Some(3), None, Some(1), Some(1), Some(1), Some(1)],
        0x1 => &[Some(2), Some(1), Some(1), Some(1), Some(1), Some(3), Some(3), Some(3), Some(3), Some(2), Some(2), None, Some(2), None, Some(2), Some(2)],
        0x2 => &[Some(4), Some(4), Some(2), Some(2), Some(2), None, Some(4), Some(4), Some(3), Some(4), Some(4), Some(3), Some(4), Some(3), Some(3), Some(2)],
        0x3 => &[Some(2), Some(2), Some(2), Some(3), Some(3), Some(3), Some(3), Some(3), Some(4), Some(3), Some(2), Some(2), None, Some(2), Some(2), Some(2)],
        0x4 => &[Some(2); 16],
        0x5 => &[Some(2), Some(2), Some(2), Some(2), Some(2), Some(3), Some(2), Some(1), Some(1), Some(1), Some(1), Some(1), Some(1), Some(1), Some(1), Some(1)],
        0x6 => &[Some(1), Some(1), Some(1), Some(2), Some(2), Some(1), None, Some(1), Some(1), Some(0), Some(0), Some(0), Some(0), Some(0), Some(0), Some(0)],
        0x7 => &[Some(0), Some(0), Some(0), Some(0), Some(0), Some(0), Some(0), Some(0), Some(2), Some(1), Some(0), Some(0), Some(0), Some(1), Some(1), Some(1)],
        0x8 => &[Some(0), Some(0), None, None, None, Some(1), Some(0), Some(1), None, None, None, None, Some(1), Some(1), Some(1), Some(1)],
        0x9 => &[Some(1), Some(1), Some(1), Some(1), Some(1), Some(0), None, Some(0), Some(1), Some(2), Some(2), Some(2), Some(2), Some(1), Some(1), Some(2)],
        0xa => &[Some(1), Some(1), Some(1), Some(1), Some(1), Some(1), Some(2), Some(3), Some(3), Some(1), Some(1), Some(1), Some(1), Some(1), Some(1), Some(2)],
        0xb => &[Some(1), None, Some(3), Some(3), None, Some(1), None, Some(3), None, None, None, None, None, None, None, None],
        _ => return None,
    };
    row[(opcode & 0x0f) as usize]
}

pub fn opcode_name(opcode: u16) -> &'static str {
    match opcode {
        0x01 => "SAY",
        0x69..=0x78 => "CONTROL",
        0x87 => "EXTENSION",
        0x8c => "VISUAL 0x8C",
        0x8d => "VISUAL 0x8D",
        0x8e => "VISUAL 0x8E",
        0x90 => "WAIT",
        0x95 => "AUDIO",
        0xa3 => "CONTENT 0xA3",
        _ if parameter_count(opcode).is_some() => "COMMAND",
        _ => "UNKNOWN",
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
}
