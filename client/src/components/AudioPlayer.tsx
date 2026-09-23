import { useEffect, useRef, useState } from "react";
import { audioSource, releaseAudioSource } from "../api";

/// Plays back a capture's original recording.
///
/// Hand-built rather than `<audio controls>`: the native control is a
/// rounded grey pill that looks like it fell in from another operating
/// system, and this app is a terminal. It also lets the duration come from
/// the file itself, which matters because early captures were stored
/// without one.
///
/// Takes a capture id rather than a URL because in the desktop app the
/// recording has to be fetched with credentials the element itself cannot
/// carry — see `audioSource`. The blob that produces is released when
/// this player goes away.
export function AudioPlayer({ eventId, mime }: { eventId: string; mime: string }) {
  const audio = useRef<HTMLAudioElement | null>(null);
  const [src, setSrc] = useState<string | null>(null);
  const [playing, setPlaying] = useState(false);
  const [position, setPosition] = useState(0);
  const [duration, setDuration] = useState<number | null>(null);
  const [failed, setFailed] = useState(false);

  // A new capture means a new element state; without this the scrubber
  // keeps the previous recording's position when navigating between
  // details.
  useEffect(() => {
    setPlaying(false);
    setPosition(0);
    setDuration(null);
    setFailed(false);
    setSrc(null);

    // Guards against the answers arriving out of order when the user
    // moves between captures faster than the recordings load.
    let current = true;
    let loaded: string | null = null;

    audioSource(eventId, mime)
      .then((url) => {
        if (!current) {
          releaseAudioSource(url);
          return;
        }
        loaded = url;
        setSrc(url);
      })
      .catch(() => {
        if (current) setFailed(true);
      });

    return () => {
      current = false;
      if (loaded) releaseAudioSource(loaded);
    };
  }, [eventId, mime]);

  function toggle() {
    const el = audio.current;
    if (!el) return;
    if (el.paused) void el.play();
    else el.pause();
  }

  function seekTo(seconds: number) {
    const el = audio.current;
    if (!el || !duration) return;
    el.currentTime = Math.min(Math.max(seconds, 0), duration);
  }

  if (failed) {
    return <p className="dim audio-player-note">The recording could not be loaded.</p>;
  }

  const progress = duration ? Math.min(position / duration, 1) : 0;

  return (
    <div className="audio-player">
      <button
        type="button"
        className="icon-btn icon-btn-bordered audio-player-button"
        aria-label={playing ? "Pause the recording" : "Play the recording"}
        onClick={toggle}
      >
        {playing ? (
          <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
            <rect x="6" y="5" width="4" height="14" rx="1.2" />
            <rect x="14" y="5" width="4" height="14" rx="1.2" />
          </svg>
        ) : (
          <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
            <path d="M8 5.5v13a1 1 0 0 0 1.5.86l10.2-6.5a1 1 0 0 0 0-1.72L9.5 4.64A1 1 0 0 0 8 5.5Z" />
          </svg>
        )}
      </button>
      {/* A real range input: it can be dragged, reached with Tab and moved
          with the arrow keys, which the clickable bar it replaces could not. */}
      <input
        type="range"
        className="audio-player-track"
        aria-label="Position in the recording"
        min={0}
        max={duration ?? 0}
        step={0.1}
        value={position}
        disabled={!duration}
        style={{ "--progress": `${progress * 100}%` } as React.CSSProperties}
        onChange={(event) => seekTo(Number(event.currentTarget.value))}
      />
      <span className="meta audio-player-time">
        {clock(position)} / {duration === null ? "--:--" : clock(duration)}
      </span>
      <audio
        ref={audio}
        src={src ?? undefined}
        preload="metadata"
        onPlay={() => setPlaying(true)}
        onPause={() => setPlaying(false)}
        onEnded={() => setPlaying(false)}
        onTimeUpdate={(e) => setPosition(e.currentTarget.currentTime)}
        onLoadedMetadata={(e) => {
          const seconds = e.currentTarget.duration;
          // A WAV written without a length header reports Infinity.
          setDuration(Number.isFinite(seconds) ? seconds : null);
        }}
        onError={() => setFailed(true)}
      />
    </div>
  );
}

function clock(seconds: number): string {
  const whole = Math.max(0, Math.floor(seconds));
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, "0")}`;
}
