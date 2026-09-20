# Roadmap

Reihenfolge, nicht Termine. Jede Phase hat ein Abbruch-/Weitermachen-Kriterium.

## Phase 0 — Vertrauenswürdig machen (Voraussetzung)

Ohne das darf nichts Persönliches ins System.

- Cloudflare-Access-JWT im Backend verifizieren (`CF_ACCESS_AUD` ist
  dokumentiert, aber nirgends im Code geprüft).
- `CorsLayer::permissive()` auf konkrete Origins einschränken.
- Backup **und getesteten Restore** einmal vollständig durchspielen.

*Weiter, wenn:* ein Restore aus dem Backup nachweislich funktioniert hat.

## Phase 1 — Capture & Echo (der eigentliche MVP)

- Audio-Aufnahme und -Speicherung nach ADR 0004, Event-Umbau
  (`capture.recorded` ohne Text, `transcript.derived`).
- `transcribe-rs` in den Tauri-Client, globaler Hotkey, Push-to-talk.
- Lokale Outbox: Erfassen gelingt offline, Sync mit Retry im Hintergrund.
- `GET /captures/{id}/echo` und Anzeige direkt nach dem Erfassen.
- Suche reparieren: Reciprocal Rank Fusion statt gewichteter Score-Addition,
  Zeitfilter, `entity_type`-Parameter entweder implementieren oder entfernen.

*Weiter, wenn:* eine Woche freiwillige tägliche Nutzung ohne Erinnerung.
*Sonst:* Ursache klären, bevor irgendetwas anderes gebaut wird.

## Phase 2 — Vertrauen im Alltag

- Transkript-Korrektur (ADR 0005), inkl. Neu-Einbettung und Neu-Extraktion.
- Redaktion mit Tombstone.
- Re-Derivation: `structure --all` über den gesamten Bestand. Das ist der
  Punkt, an dem Event Sourcing seine Existenz rechtfertigt.
- JSONL-Spiegel der Rohdaten für Format-Langlebigkeit.

## Phase 3 — Der Graph verdient sich seinen Platz

Erst ab ~1.000 Captures beginnen.

- Entitäts-Zusammenführung mit Kandidatensuche (`pg_trgm` + Embedding).
- Review-Queue, eine Oberfläche für Merges und Korrekturen.
- Relationen über Capture-Grenzen hinweg (heute unmöglich,
  `structuring.rs:65-73`).
- `entities.current_summary` klären: entfernen oder echt konsolidieren.

*Weiter, wenn:* der Graph mindestens einmal etwas gezeigt hat, das Echo
nicht gezeigt hätte.

## Phase 4 — Ausbreitung

- iOS: Kurzbefehl mit Apple-Diktat, oder eigene App mit lokalem Whisper.
- Wöchentliche Rückschau, gekoppelt an den ohnehin nötigen Review-Termin.
- Neu bewerten: Chat, MCP, Dokumente. Alle drei mit der Frage, ob sie ein
  Problem lösen, das dann tatsächlich existiert.
- Konsolidierungslauf: nicht mehr offen, sondern geschnitten. Zuschnitt,
  Auslöser, Events und Reihenfolge in
  [`docs/consolidation.md`](consolidation.md). Schritt 1 daraus
  (Zeitkontext im Extraktions-Prompt) gehört vorgezogen — er verbessert
  jede neue Notiz sofort.
