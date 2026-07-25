import { AlertCircle, LoaderCircle, Pause, Play, Volume2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { ResourceRef, SoundEffectPreview } from "../../ipc/bindings";
import { ipc } from "../../ipc/client";
import { formatHex } from "../../shared/lib/format";

type PlayableSoundEffect = SoundEffectPreview & { url: string };

function formatTime(seconds: number) {
  if (!Number.isFinite(seconds) || seconds < 0) return "0:00";
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${Math.floor(seconds % 60).toString().padStart(2, "0")}`;
}

export function SoundEffectPlayback({ document, soundId }: { document: ResourceRef; soundId: number }) {
  const audioRef = useRef<HTMLAudioElement>(null);
  const [preview, setPreview] = useState<PlayableSoundEffect | null>(null);
  const [loading, setLoading] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [autoplay, setAutoplay] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    audioRef.current?.pause();
    setPreview(null);
    setLoading(false);
    setPlaying(false);
    setAutoplay(false);
    setCurrentTime(0);
    setDuration(0);
    setError(null);
    return () => audioRef.current?.pause();
  }, [document, soundId]);

  useEffect(() => {
    if (!preview || !autoplay || !audioRef.current) return;
    setAutoplay(false);
    void audioRef.current.play().catch((reason) => setError(String(reason)));
  }, [autoplay, preview]);

  const toggle = async () => {
    const audio = audioRef.current;
    if (preview && audio) {
      if (audio.paused) {
        if (audio.ended) audio.currentTime = 0;
        await audio.play().catch((reason) => setError(String(reason)));
      } else {
        audio.pause();
      }
      return;
    }
    setLoading(true);
    setError(null);
    try {
      setPreview(await ipc.getSoundEffectPreview(document, soundId));
      setAutoplay(true);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  };

  const knownDuration = duration || (preview?.durationMillis ?? 0) / 1000;
  const sourcePath = preview
    ? `${preview.source.isoPath}${preview.source.members.map((member) => ` / ${member.name}`).join("")}`
    : null;

  return (
    <section className="property-section sound-effect-section">
      <h2><Volume2 size={13} /> 音效预览</h2>
      <div className="sound-effect-player">
        <button
          type="button"
          className="audio-play-button"
          onClick={() => void toggle()}
          disabled={loading}
          title={playing ? "暂停音效" : "播放音效"}
          aria-label={playing ? "暂停音效" : "播放音效"}
        >
          {loading ? <LoaderCircle className="spin" size={15} /> : playing ? <Pause size={15} /> : <Play size={15} />}
        </button>
        <input
          type="range"
          min={0}
          max={knownDuration || 1}
          step={0.01}
          value={Math.min(currentTime, knownDuration || 1)}
          disabled={!preview || !knownDuration}
          aria-label="音效播放进度"
          onChange={(event) => {
            const value = Number(event.target.value);
            if (audioRef.current) audioRef.current.currentTime = value;
            setCurrentTime(value);
          }}
        />
        <code>{formatTime(currentTime)} / {formatTime(knownDuration)}</code>
      </div>
      {error && <div className="sound-effect-error"><AlertCircle size={14} /><span>{error}</span></div>}
      {preview && (
        <>
          <dl className="property-grid sound-effect-meta">
            <dt>音库</dt><dd>{preview.bankName} (slot {preview.bankSlot})</dd>
            <dt>映射</dt><dd className="mono">bank {preview.logicalBank} / program {preview.program} / note {preview.note}</dd>
            <dt>Packed ID</dt><dd className="mono">{formatHex(preview.packedId, 8)}</dd>
            <dt>采样</dt><dd>{preview.sampleRate} Hz / {preview.channels} ch</dd>
            <dt>跟踪句柄</dt><dd>{preview.tracked ? "是" : "否"}</dd>
          </dl>
          <code className="resource-path">{sourcePath}</code>
          <audio
            ref={audioRef}
            src={preview.url}
            preload="metadata"
            onPlay={() => setPlaying(true)}
            onPause={() => setPlaying(false)}
            onEnded={() => setPlaying(false)}
            onTimeUpdate={(event) => setCurrentTime(event.currentTarget.currentTime)}
            onDurationChange={(event) => setDuration(event.currentTarget.duration)}
            onError={() => setError("PCM WAV 无法由当前 WebView 播放")}
          />
        </>
      )}
    </section>
  );
}
