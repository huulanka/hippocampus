# ADR 0004: Audio ist das Original, das Transkript ist Interpretation

## Status
Accepted (2026-09-20)

## Context
Bisher gilt das Transkript als das unveränderliche Original: `POST /captures`
nimmt `transcript_text` entgegen und legt es als `capture.recorded` ab,
Audio bleibt laut `contracts/src/lib.rs` ausdrücklich auf dem Gerät
(„Audio itself is never uploaded to the backend").

Das widerspricht dem Kernprinzip des Projekts. Ein Transkript ist die
Ausgabe eines ASR-Modells — es ist bereits der erste KI-abgeleitete Artefakt
der Kette, mit allen Fehlern, die solche Modelle machen. Gerade bei
deutschen Eigennamen, also genau dem, was in einem persönlichen
Wissenssystem zählt, irrt sich ASR am häufigsten („Kuh Portal" statt
„HPortal"). Wird nur das Transkript behalten, ist dieser Fehler dauerhaft
und unumkehrbar: Es gibt keine Instanz mehr, gegen die man ihn prüfen könnte.

Speicherkosten sind kein Gegenargument. Bei angenommenen 15 Captures pro Tag
à 30 Sekunden ergeben sich mit Opus-Kompression (~24 kbit/s) etwa 1,5 GB pro
Jahr — auf der vorhandenen NAS irrelevant.

## Decision
- Audio wird dauerhaft aufbewahrt, auf der NAS, als Standardformat (Opus in
  Ogg), unter einem inhaltsadressierten Pfad.
- `capture.recorded` referenziert das Audio und enthält **keinen** Text mehr.
- Das Transkript entsteht als eigenes Event `transcript.derived` mit
  Modellangabe und ist damit ausdrücklich Interpretation.
- Captures ohne Audio (getippt, iOS-Kurzbefehl mit Apple-Diktat) tragen
  `origin: "text"`; dort ist der Text das Original und unveränderlich.

## Consequences
- `CreateCaptureRequest` ändert sich: Audio wird hochgeladen, der Kommentar
  in `contracts/src/lib.rs` wird hinfällig.
- Bessere ASR-Modelle können später auf den gesamten Bestand angewandt
  werden — Neu-Transkription wird zum selben Vorgang wie Neu-Strukturierung.
  Das ist der zweite und stärkere Grund, warum Event Sourcing hier trägt.
- Größere Uploads und ein Blob-Speicherpfad, den es bisher nicht gibt.
- Audio ist die sensibelste Datenform im System: Stimme, Hintergrundgeräusche,
  mitgehörte Dritte. Es verlässt die eigene Hardware niemals — nur Text geht
  an OpenRouter.
