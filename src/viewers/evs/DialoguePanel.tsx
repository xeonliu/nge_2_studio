import { AlertCircle, EyeOff, LoaderCircle, MessageSquareText, Pause, Play, Volume2, VolumeX } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { AudioPreview, DialogueFrame, ResourceRef } from "../../ipc/bindings";
import { ipc } from "../../ipc/client";

type PlayableAudio = AudioPreview & { url: string };

function formatTime(seconds: number) {
  if (!Number.isFinite(seconds) || seconds < 0) return "0:00";
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${Math.floor(seconds % 60).toString().padStart(2, "0")}`;
}

function AudioPlayback({ document, voiceId, active, onPlay }: { document: ResourceRef; voiceId: number; active: boolean; onPlay: () => void }) {
  const audioRef = useRef<HTMLAudioElement>(null);
  const [preview, setPreview] = useState<PlayableAudio | null>(null);
  const [loading, setLoading] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [autoplay, setAutoplay] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const audio = audioRef.current;
    audio?.pause();
    setPreview(null);
    setLoading(false);
    setPlaying(false);
    setAutoplay(false);
    setCurrentTime(0);
    setDuration(0);
    setError(null);
    return () => audioRef.current?.pause();
  }, [document, voiceId]);

  useEffect(() => {
    if (!preview || !autoplay || !audioRef.current) return;
    setAutoplay(false);
    void audioRef.current.play().catch((reason) => setError(String(reason)));
  }, [autoplay, preview]);

  useEffect(() => {
    if (!active) audioRef.current?.pause();
  }, [active]);

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
      setPreview(await ipc.getAudioPreview(document, voiceId));
      setAutoplay(true);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  };

  const knownDuration = duration || (preview?.durationMillis ?? 0) / 1000;
  return (
    <div className="audio-playback">
      <button
        type="button"
        className="audio-play-button"
        onClick={() => void toggle()}
        disabled={loading}
        title={playing ? "暂停语音" : "播放语音"}
        aria-label={playing ? "暂停语音" : "播放语音"}
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
        aria-label="语音播放进度"
        onChange={(event) => {
          const value = Number(event.target.value);
          if (audioRef.current) audioRef.current.currentTime = value;
          setCurrentTime(value);
        }}
      />
      <code>{formatTime(currentTime)} / {formatTime(knownDuration)}</code>
      {error && <span className="audio-error" title={error}><AlertCircle size={14} /></span>}
      {preview && (
        <audio
          ref={audioRef}
          src={preview.url}
          preload="metadata"
          onPlay={() => {
            setPlaying(true);
            onPlay();
          }}
          onPause={() => setPlaying(false)}
          onEnded={() => setPlaying(false)}
          onTimeUpdate={(event) => setCurrentTime(event.currentTarget.currentTime)}
          onDurationChange={(event) => setDuration(event.currentTarget.duration)}
          onError={() => setError("PCM WAV 无法由当前 WebView 播放")}
        />
      )}
    </div>
  );
}

function AudioTrackList({ document, frame }: { document: ResourceRef; frame: DialogueFrame }) {
  const [activeVoiceId, setActiveVoiceId] = useState<number | null>(null);

  useEffect(() => setActiveVoiceId(null), [frame.commandIndex]);

  return (
    <div className="audio-track-list">
      {frame.audioTracks.map((track) => (
        <div className={`audio-track${track.voiceId === null ? " silent" : ""}`} key={track.pageIndex}>
          <span className="audio-track-label">
            {track.voiceId === null ? <VolumeX size={13} /> : <Volume2 size={13} />}
            <b>P{track.pageIndex + 1}</b>
            {track.voiceId === null ? "NO AUDIO" : `Audio ${track.voiceId}`}
          </span>
          {track.voiceId !== null && (
            <AudioPlayback
              document={document}
              voiceId={track.voiceId}
              active={activeVoiceId === null || activeVoiceId === track.voiceId}
              onPlay={() => setActiveVoiceId(track.voiceId)}
            />
          )}
        </div>
      ))}
    </div>
  );
}

export function DialoguePanel({ document, frame }: { document: ResourceRef; frame: DialogueFrame | null }) {
  if (!frame) return <section className="dialogue-panel empty"><MessageSquareText size={20} />选择 SAY 命令查看台词</section>;
  return (
    <section className="dialogue-panel">
      <div className="speaker-block">
        <strong>{frame.speakerName}</strong>
        <span>{frame.expressionName}</span>
        {frame.portrait?.runtimeHidden && <em><EyeOff size={12} />NO_AVATAR</em>}
      </div>
      <div className="dialogue-text">
        {frame.pages.map((page, index) => (
          <span key={index}>{page}{index < frame.pages.length - 1 && <i className="page-break">▽</i>}</span>
        ))}
      </div>
      <div className="dialogue-stats">
        <AudioTrackList document={document} frame={frame} />
        <div className="dialogue-meta"><span>{frame.textBytes} bytes</span><span>{frame.pages.length} page{frame.pages.length > 1 ? "s" : ""}</span></div>
      </div>
    </section>
  );
}
