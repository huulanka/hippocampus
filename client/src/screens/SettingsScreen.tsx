import { useState } from "react";
import { useTheme } from "../theme";

// Nothing here persists yet — there's no Tauri-side settings store wired up
// (global hotkey, tray, ASR model choice, storage path). Local state only,
// so the layout and interactions are ready once that lands.
export function SettingsScreen() {
  const { theme, setTheme } = useTheme();
  const [trayIcon, setTrayIcon] = useState(true);
  const [asrMode, setAsrMode] = useState<"on-device" | "cloud">("on-device");
  const [deleteAudio, setDeleteAudio] = useState(true);

  return (
    <>
      <h4 className="section-label">APPEARANCE</h4>
      <div className="theme-switch">
        <span
          className={`btn${theme === "dark" ? " btn-accent" : ""}`}
          onClick={() => setTheme("dark")}
        >
          [ Dark ]
        </span>
        <span
          className={`btn${theme === "light" ? " btn-accent" : ""}`}
          onClick={() => setTheme("light")}
        >
          [ Light ]
        </span>
      </div>

      <h4 className="section-label">GLOBAL HOTKEY</h4>
      <div className="settings-row">
        <div className="hotkey-display">⌃ ⌥ Space</div>
        <span className="btn">[ Change ]</span>
      </div>

      <label className="settings-checkbox">
        <input type="checkbox" checked={trayIcon} onChange={(e) => setTrayIcon(e.currentTarget.checked)} />
        Show tray icon — capture from anywhere without opening the window
      </label>

      <h4 className="section-label">SPEECH RECOGNITION</h4>
      <div className="settings-row">
        <label className="settings-radio">
          <input
            type="radio"
            checked={asrMode === "on-device"}
            onChange={() => setAsrMode("on-device")}
          />
          On-device
        </label>
        <label className="settings-radio">
          <input type="radio" checked={asrMode === "cloud"} onChange={() => setAsrMode("cloud")} />
          Cloud
        </label>
      </div>

      <h4 className="section-label">STORAGE LOCATION</h4>
      <div className="settings-row">
        <div className="storage-path dim">/Volumes/nas/hippocampus</div>
        <span className="btn">[ Change ]</span>
      </div>

      <h4 className="section-label">PRIVACY</h4>
      <label className="settings-checkbox">
        <input type="checkbox" checked={deleteAudio} onChange={(e) => setDeleteAudio(e.currentTarget.checked)} />
        Delete audio after transcription
      </label>
    </>
  );
}
