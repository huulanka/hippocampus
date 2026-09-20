# Hippocampus — Product Scope

Stand: 2026-09-20. Ergebnis einer Scoping-Session. Dieses Dokument schlägt
ADRs und Code nicht in der Autorität, es geht ihnen voraus: es sagt, *warum*
etwas gebaut wird. Architektur steht in `docs/adr/`, das Datenmodell in
`docs/memory-model.md`, die Reihenfolge in `docs/roadmap.md`.

## Product Vision

Ein persönliches Gedächtnissystem, das Gedanken mit minimaler Reibung
aufnimmt, sie unverändert aufbewahrt und sie in dem Moment wieder
hervorholt, in dem sie relevant sind — ohne dass man sich erinnern muss,
danach zu fragen.

## Problem

Gedanken entstehen laufend und gehen laufend verloren. Notiz-Apps lösen das
nicht, weil sie drei Kosten haben, die zusammen jede Gewohnheit töten:

1. **Erfassungskosten** — App öffnen, Ort wählen, tippen, benennen, ablegen.
2. **Ordnungskosten** — beim Erfassen schon wissen, wohin es gehört.
3. **Abrufkosten** — sich daran erinnern, dass man es notiert hat.

Kosten 3 ist die eigentliche Todesursache. Ein Archiv, das nur auf Nachfrage
antwortet, hilft genau dann nicht, wenn man es am nötigsten hätte.

## Target User

Zunächst ausschließlich der Autor: Einzelperson, ganztägig im Homeoffice am
Mac, denkt viel in Projekten, Personen und wiederkehrenden Themen, spricht
lieber als zu tippen, will Datenhoheit und möglichst keine laufenden Kosten.

Verallgemeinerbar auf: Menschen mit hoher Gedankenfrequenz und schlechter
Ablagedisziplin, die bestehende PKM-Systeme (Obsidian, Notion) nicht wegen
fehlender Features aufgegeben haben, sondern wegen des Pflegeaufwands.

## Core Loop

```
SPRECHEN  →  ECHO  →  (gelegentlich) SUCHEN  →  (später) GRAPH
```

**Sprechen** — Hotkey, sprechen, loslassen. Keine Kategorie, kein Titel,
kein Speichern-Dialog. Die Aufnahme gelingt immer, auch wenn das Backend
nicht erreichbar ist.

**Echo** — unmittelbar nach dem Erfassen zeigt das System 2–3 frühere
Captures, die semantisch nahe sind. Wörtlich, mit Datum, ohne
Zusammenfassung.

Echo ist der Kern. Es verlangt keine neue Gewohnheit, weil es in der
Schleife passiert, in der der Nutzer ohnehin ist. Es liefert ab Capture
Nr. 20 Wert statt ab Nr. 2000. Und es kann nicht halluzinieren, weil es nur
eigene Worte zeigt.

**Suchen** — bewusste Abfrage über alle Captures, hybrid (Volltext +
Semantik), mit Zeitfiltern.

**Graph** — Personen, Projekte, Themen und ihre Beziehungen. Erklärtes
langfristiges Produktziel, aber *nachgelagert*: er braucht Capture-Volumen
und eine funktionierende Entitäts-Zusammenführung, bevor er etwas Sinnvolles
zeigen kann. Er muss sich seinen Platz verdienen.

## Getroffene Produktentscheidungen

| # | Entscheidung | Begründung |
|---|---|---|
| P1 | Verknüpfung ist der Produktkern, nicht nur Wiederfinden | Ein durchsuchbares Sprachtagebuch wäre für den Autor ein Scheitern |
| P2 | Echo vor Graph | Echo ist Verknüpfung ohne Entitäts-Voraussetzungen und ist technisch fast fertig |
| P3 | Capture-Pfad hat absolute Priorität | Ohne echte, unordentliche Sprachdaten ist keine nachgelagerte Qualität beurteilbar |
| P4 | Mac zuerst, iOS danach | Der Nutzer ist ganztägig am Mac; das Telefon deckt Unterwegs-Fälle ab |
| P5 | Planungsannahme 10–20 Captures/Tag | ~5.000/Jahr; macht Entitäts-Fragmentierung zu einem realen, nicht theoretischen Problem |
| P6 | Audio ist das Original, das Transkript ist Interpretation | Siehe ADR 0004 |
| P7 | Korrekturen als Event, Original bleibt | Siehe ADR 0005 |
| P8 | Redaktion mit Tombstone möglich | Siehe ADR 0005 |
| P9 | Unsichere Entitäts-Zusammenführungen gehen in eine Review-Queue | Eine falsche Verschmelzung ist in einem Gedächtnissystem Vertrauensverlust, nicht nur ein Fehler |
| P10 | Strukturierung über OpenRouter mit ZDR, bewusst akzeptiert | Die NAS (Celeron, keine GPU) kann kein LLM; das Event Log erlaubt späteren Wechsel auf lokal |
| P11 | Kein Morning Brief, keine Push-Benachrichtigungen | Jede Benachrichtigung ist eine Gewohnheit, die extra aufgebaut werden muss |

## Functional Scope (MVP)

Ein MVP ist erreicht, wenn der Autor das System **freiwillig eine Woche lang
täglich benutzt**, ohne dass ihn jemand daran erinnert.

1. Globaler Hotkey auf dem Mac, Push-to-talk, lokale Transkription.
2. Audio wird dauerhaft gespeichert, Transkript wird daraus abgeleitet.
3. Lokale Warteschlange: Erfassen gelingt offline, Sync läuft im Hintergrund.
4. Echo: 2–3 semantisch nahe frühere Captures direkt nach dem Erfassen.
5. Suche über alle Captures, hybrid, mit Zeitfilter.
6. Timeline: chronologisches Durchblättern.
7. Korrektur eines Transkripts (Original bleibt erhalten).
8. Backend ist authentifiziert und von außen erreichbar.
9. Ein getesteter Backup-/Restore-Durchlauf.

Punkte 8 und 9 sind keine Features, sondern die Bedingung dafür, dass man
dem System persönliche Inhalte überhaupt anvertrauen darf.

## Out of Scope für V1

| Nicht bauen | Grund |
|---|---|
| Chat / RAG über die Captures | Ein Gedächtnissystem darf über die eigene Vergangenheit nicht konfabulieren — man kann eine erfundene Erinnerung nicht von einer echten unterscheiden. Zitate mit Datum und Link zum Original sind hier besser als jede Zusammenfassung. `ChatScreen.tsx` bleibt bis auf Weiteres ein Mock. |
| Kryptografische Hash-Kette | Kein Bedrohungsmodell. Wer Schreibzugriff auf die DB hat, schreibt die Kette mit. Ohne externe Verankerung ist es Theater. Ein geprüfter Restore ist mehr wert. |
| Dreaming-/Consolidation-Agent | Ein Prozess, der unbeaufsichtigt Beziehungen im eigenen Gedächtnis erfindet, ist ein Halluzinationsgenerator mit Schreibrechten. Frühestens nach 1.000 Captures und nur mit Review. |
| Dokumente, PDFs, E-Mails, Screenshots | Anderes Produkt (Dokumentenextraktion). Löst nicht das Problem oben. |
| Eigene Hardware (ESP32 etc.) | Der Mac ist bereits das Ambient-Gerät. |
| MCP-Anbindung an ChatGPT | Erst wenn das Archiv Substanz hat. |
| Decay / Vergessen als gespeicherter Zustand | In einem persönlichen Archiv löscht man nichts. Allenfalls ein Ranking-Faktor. |
| Confidence-/Belief-Modellierung über Zeit | Konzeptionell reizvoll, aber ohne Daten nicht kalibrierbar. Nach Phase 3 neu bewerten. |

## Non-Functional Requirements

- **Erfassung schlägt nie fehl.** Lokale Persistenz vor Netzwerk. Ein
  einziger verlorener Gedanke kostet das Vertrauen dauerhaft.
- **Erfassung ist unter 2 Sekunden startklar.** Hotkey bis Aufnahme.
- **Datenhoheit.** Rohdaten (Audio + Transkript) verlassen die eigene
  Hardware nur für die Strukturierung, als Text, an ZDR-Anbieter.
- **Keine laufenden Kosten über ~5 €/Monat.** Bei 5.000 Extraktionen/Jahr
  mit einem günstigen Modell realistisch erreichbar.
- **Format-Langlebigkeit.** Die Rohdaten müssen ohne dieses Programm lesbar
  bleiben. Audio als Standardformat auf der Platte, Captures zusätzlich als
  JSONL-Spiegel.
- **Authentifizierung**, sobald das Backend das LAN verlässt.

## Offene Fragen

- Ab welcher Menge lohnt der Graph? Vorschlag: nach 1.000 Captures neu
  bewerten, nicht vorher.
- Wie viele Echo-Treffer sind zu viele? Startwert 3, Schwelle empirisch.
- Braucht iOS eine eigene App (lokales Whisper) oder reicht der Kurzbefehl
  mit Apple-Diktat? Erst nach dem Mac-Pfad entscheiden.
- Bleibt `current_summary` als Feld bestehen oder wird es zur Lesezeit
  berechnet? Siehe `docs/memory-model.md`.
