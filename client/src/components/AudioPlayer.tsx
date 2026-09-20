import { useEffect, useRef, useState } from "react";

/// Plays back a capture's original recording.
///
/// Hand-built rather than `<audio controls>`: the native control is a
/// rounded grey pill that looks like it fell in from another operating
/// system, and this app is a terminal. It also lets the duration come from
/// the file itself, which matters because early captures were stored
/// without one.
export function AudioPlayer({ src }: { src: string }) {
  const audio = useRef<HTMLAudioElement | null>(null);
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
  }, [src]);

  function toggle() {
    const el = audio.current;
    if (!el) return;
    if (el.paused) void el.play();
    else el.pause();
  }

  function seek(event: React.MouseEvent<HTMLDivElement>) {
    const el = audio.current;
    if (!el || !duration) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    el.currentTime = ((event.clientX - bounds.left) / bounds.width) * duration;
  }

  if (failed) {
    return <p className="dim audio-player-note">The recording could not be loaded.</p>;
  }

  const progress = duration ? Math.min(position / duration, 1) : 0;

  return (
    <div className="audio-player">
      <span className="audio-player-button" onClick={toggle}>
        [ {playing ? "▮▮" : "▶"} ]
      </span>
      <div className="audio-player-track" onClick={seek}>
        <div className="audio-player-fill" style={{ width: `${progress * 100}%` }} />
      </div>
      <span className="dim audio-player-time">
        {clock(position)} / {duration === null ? "--:--" : clock(duration)}
      </span>
      <audio
        ref={audio}
        src={src}
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
