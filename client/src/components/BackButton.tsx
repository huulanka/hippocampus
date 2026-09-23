/// The way out of a screen you arrived at from somewhere else. A real
/// button — the one it replaces was a `<span onClick>` reading "← back",
/// which Tab walked straight past.
export function BackButton({ onBack, label = "Back" }: { onBack: () => void; label?: string }) {
  return (
    <button type="button" className="btn-quiet back-button" onClick={onBack}>
      <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M14.5 5.5 8 12l6.5 6.5" />
      </svg>
      {label}
    </button>
  );
}
