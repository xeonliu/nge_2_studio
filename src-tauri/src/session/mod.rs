use iso_vfs::IsoImage;
use lru::LruCache;
use nge2_formats::audio::{index_wave_archive, WaveEntry};
use nge2_formats::hgar::HgarArchive;
use nge2_formats::voice::{voice_ordinal, VOICE_ENTRY_COUNT};
use nge2_formats::zpt;
use nge2_preview::{ContainerMember, GamePath, ResourceRef, SessionId};
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use uuid::Uuid;

const RESOURCE_CACHE_LIMIT: usize = 96 * 1024 * 1024;
const PREVIEW_CACHE_LIMIT: usize = 48 * 1024 * 1024;

#[derive(Clone)]
pub struct SessionManager {
    inner: Arc<ManagerInner>,
}

struct ManagerInner {
    sessions: RwLock<HashMap<String, Arc<IsoSession>>>,
    generation: AtomicU64,
}

pub struct IsoSession {
    pub id: SessionId,
    pub iso: IsoImage,
    resources: Mutex<SizedLru<Arc<Vec<u8>>>>,
    previews: Mutex<SizedLru<Arc<PreviewBlob>>>,
    variants: Mutex<HashMap<(String, u32), ResourceRef>>,
    voice_archives: Mutex<Option<Arc<Vec<VoiceArchiveIndex>>>>,
}

#[derive(Clone, Debug)]
pub struct PreviewBlob {
    pub mime: &'static str,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct VoiceClip {
    pub archive: u8,
    pub entry: u32,
    pub resource: ResourceRef,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct VoiceArchiveIndex {
    path: String,
    entries: Vec<WaveEntry>,
}

struct SizedLru<T> {
    cache: LruCache<String, (usize, T)>,
    bytes: usize,
    byte_limit: usize,
}

impl<T> SizedLru<T> {
    fn new(entry_limit: usize, byte_limit: usize) -> Self {
        Self {
            cache: LruCache::new(NonZeroUsize::new(entry_limit).expect("positive LRU limit")),
            bytes: 0,
            byte_limit,
        }
    }

    fn get(&mut self, key: &str) -> Option<&T> {
        self.cache.get(key).map(|(_, value)| value)
    }

    fn put(&mut self, key: String, size: usize, value: T) {
        if let Some((old_size, _)) = self.cache.pop(&key) {
            self.bytes = self.bytes.saturating_sub(old_size);
        }
        while self.bytes.saturating_add(size) > self.byte_limit {
            if let Some((_, (evicted_size, _))) = self.cache.pop_lru() {
                self.bytes = self.bytes.saturating_sub(evicted_size);
            } else {
                break;
            }
        }
        if size <= self.byte_limit {
            self.bytes += size;
            self.cache.put(key, (size, value));
        }
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self {
            inner: Arc::new(ManagerInner {
                sessions: RwLock::new(HashMap::new()),
                generation: AtomicU64::new(0),
            }),
        }
    }
}

impl SessionManager {
    pub fn open(&self, path: impl AsRef<Path>) -> Result<Arc<IsoSession>, String> {
        let iso = IsoImage::open(path).map_err(|error| error.to_string())?;
        let generation = self.inner.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let id = SessionId(format!("{generation}-{}", Uuid::new_v4()));
        let session = Arc::new(IsoSession {
            id: id.clone(),
            iso,
            resources: Mutex::new(SizedLru::new(128, RESOURCE_CACHE_LIMIT)),
            previews: Mutex::new(SizedLru::new(96, PREVIEW_CACHE_LIMIT)),
            variants: Mutex::new(HashMap::new()),
            voice_archives: Mutex::new(None),
        });
        let mut sessions = self.inner.sessions.write();
        sessions.clear();
        sessions.insert(id.0.clone(), session.clone());
        Ok(session)
    }

    pub fn get(&self, id: &SessionId) -> Result<Arc<IsoSession>, String> {
        self.inner
            .sessions
            .read()
            .get(&id.0)
            .cloned()
            .ok_or_else(|| "ISO session 已关闭或已被新的 ISO 替换".into())
    }

    pub fn close(&self, id: &SessionId) -> bool {
        self.inner.sessions.write().remove(&id.0).is_some()
    }

    pub fn preview(&self, token: &str) -> Option<Arc<PreviewBlob>> {
        for session in self.inner.sessions.read().values() {
            if let Some(blob) = session.previews.lock().get(token).cloned() {
                return Some(blob);
            }
        }
        None
    }
}

impl IsoSession {
    pub fn resource_data(&self, resource: &ResourceRef) -> Result<Arc<Vec<u8>>, String> {
        if resource.session_id != self.id {
            return Err("资源引用不属于当前 ISO session".into());
        }
        let key = resource_key(resource);
        if let Some(data) = self.resources.lock().get(&key).cloned() {
            return Ok(data);
        }
        let mut data = self
            .iso
            .read_file(&resource.iso_path.0)
            .map_err(|error| error.to_string())?;
        data = decode_zpt(&resource.iso_path.0, data)?;
        for member in &resource.members {
            let archive = HgarArchive::parse(&data).map_err(|error| error.to_string())?;
            data = archive
                .entry_data(&data, member.index as usize)
                .map_err(|error| error.to_string())?;
            data = decode_zpt(&member.name, data)?;
        }
        let data = Arc::new(data);
        self.resources
            .lock()
            .put(key, data.len(), data.clone());
        Ok(data)
    }

    pub fn store_preview(&self, resource: &ResourceRef, blob: PreviewBlob) -> String {
        let token = format!("{}-{}", self.id.0, stable_hash(&resource_key(resource)));
        let size = blob.bytes.len();
        self.previews.lock().put(token.clone(), size, Arc::new(blob));
        token
    }

    pub fn voice_clip(&self, voice_id: u32) -> Result<VoiceClip, String> {
        let ordinal = voice_ordinal(voice_id)
            .ok_or_else(|| format!("voice ID 0x{voice_id:X} 没有对应的音频条目"))?;
        let archives = self.voice_archive_indexes()?;
        let mut remaining = ordinal;
        for (archive_index, archive) in archives.iter().enumerate() {
            if remaining >= archive.entries.len() {
                remaining -= archive.entries.len();
                continue;
            }
            let entry = archive.entries[remaining];
            let bytes = self
                .iso
                .read_file_range(&archive.path, entry.offset, entry.size as usize)
                .map_err(|error| error.to_string())?;
            let resource = ResourceRef {
                session_id: self.id.clone(),
                iso_path: GamePath(archive.path.clone()),
                members: vec![ContainerMember {
                    index: remaining as u32,
                    name: format!("voice_{voice_id:04X}.at3"),
                }],
            };
            return Ok(VoiceClip {
                archive: archive_index as u8,
                entry: remaining as u32,
                resource,
                bytes,
            });
        }
        Err(format!("voice ID 0x{voice_id:X} 的序号超出语音归档"))
    }

    fn voice_archive_indexes(&self) -> Result<Arc<Vec<VoiceArchiveIndex>>, String> {
        if let Some(indexes) = self.voice_archives.lock().clone() {
            return Ok(indexes);
        }
        let mut indexes = Vec::with_capacity(3);
        for archive in 0..3u8 {
            let path = format!("/PSP_GAME/USRDIR/voice/na{archive}.bin");
            let size = self.iso.entry(&path).map_err(|error| error.to_string())?.size;
            let mut file = self.iso.open_file(&path).map_err(|error| error.to_string())?;
            let entries = index_wave_archive(&mut file, u64::from(size))
                .map_err(|error| format!("{path}: {error}"))?;
            indexes.push(VoiceArchiveIndex { path, entries });
        }
        let total = indexes.iter().map(|archive| archive.entries.len()).sum::<usize>();
        if total != VOICE_ENTRY_COUNT {
            return Err(format!(
                "语音归档共 {total} 条，预期 {VOICE_ENTRY_COUNT} 条；ISO 版本或文件可能不受支持"
            ));
        }
        let indexes = Arc::new(indexes);
        *self.voice_archives.lock() = Some(indexes.clone());
        Ok(indexes)
    }

    pub fn select_variant(&self, document: &ResourceRef, command_index: u32, selected: ResourceRef) {
        self.variants
            .lock()
            .insert((resource_key(document), command_index), selected);
    }

    pub fn selected_variant(&self, document: &ResourceRef, command_index: u32) -> Option<ResourceRef> {
        self.variants
            .lock()
            .get(&(resource_key(document), command_index))
            .cloned()
    }

    pub fn cache_bytes(&self) -> u32 {
        (self.resources.lock().bytes + self.previews.lock().bytes) as u32
    }
}

fn decode_zpt(name: &str, data: Vec<u8>) -> Result<Vec<u8>, String> {
    if name.to_ascii_lowercase().ends_with(".zpt") && data.get(..4) != Some(b"HGPT") {
        zpt::decompress(&data).map_err(|error| error.to_string())
    } else {
        Ok(data)
    }
}

pub fn resource_key(resource: &ResourceRef) -> String {
    let members = resource
        .members
        .iter()
        .map(|member| member.index.to_string())
        .collect::<Vec<_>>()
        .join("/");
    format!("{}#{members}", resource.iso_path.0)
}

fn stable_hash(value: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{hash:016x}")
}
