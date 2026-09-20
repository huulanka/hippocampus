# ADR 0005: Korrekturen und Redaktionen ohne Bruch der Unantastbarkeit

## Status
Accepted (2026-09-20)

## Context
Zwei reale Anforderungen kollidieren mit „nichts wird je verändert":

1. **Korrektur.** Ein Transkript ist falsch („Kuh Portal" statt „HPortal").
   Bleibt es unverändert, ist die Erinnerung praktisch verloren, weil die
   Suche nach dem richtigen Wort sie nicht findet.
2. **Redaktion.** Irgendwann wird etwas erfasst, das dauerhaft zu speichern
   weder gewollt noch — wenn Dritte darin vorkommen — vertretbar ist.

Gleichzeitig ist `events` per `RULE ... DO INSTEAD NOTHING` gegen UPDATE und
DELETE gesperrt (ADR 0003). Inhalte im Event Log wären damit
unwiderruflich — die Sperre, die Vertrauen schaffen soll, würde das System
zu einem Ort machen, dem man nichts Heikles anvertrauen kann.

## Decision

**Trennung von Struktur und Inhalt.** Das Event Log enthält nur, *was wann
durch wen oder welches Modell geschah*, plus Verweise. Inhalte (Audio-Pfade,
Transkripttexte) liegen in eigenen Tabellen, die per Event-ID referenziert
werden und regulär beschreibbar sind.

**Korrektur** ist additiv: `transcript.corrected` legt eine neue Fassung an
und verweist auf die ersetzte. Alle Fassungen bleiben lesbar und sind im UI
nebeneinander einsehbar. Suche, Embedding und Strukturierung arbeiten mit
der jeweils aktuellen Fassung; bei einer Korrektur werden Embedding und
Extraktion für diesen Capture neu berechnet.

**Redaktion** ist destruktiv, aber protokolliert: Der Inhalt wird in
`capture_content` / `transcript_content` geleert, abgeleitete Observations
und Relations werden entfernt, und `capture.redacted` wird angehängt. Was
bleibt, ist ein Tombstone: Zeitpunkt, Gerät und die Tatsache, dass hier
etwas war und bewusst entfernt wurde.

## Consequences
- Die Aussage „das Event Log wird nie verändert" bleibt wörtlich wahr.
- Redaktion wirkt **nicht** rückwirkend auf bestehende Backups. Das wird
  bewusst akzeptiert; wer vollständig entfernen will, muss zusätzlich alte
  Backups verwerfen.
- Korrekturen verursachen Folgearbeit (Neu-Einbettung, Neu-Extraktion). Bei
  der erwarteten Häufigkeit ist das unkritisch.
- `capture_search` spiegelt künftig die aktuelle Fassung, nicht die erste.
- Derselbe UI-Bereich bedient Transkript-Korrekturen und die Review-Queue
  für Entitäts-Zusammenführungen: in beiden Fällen ist das System unsicher,
  der Mensch entscheidet, die Entscheidung wird als Event festgehalten.
