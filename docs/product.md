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
| P9 | Zusammenführungen richten sich nach Beweisdichte, nicht nach Vorsicht allein | Was ohne Abwägen entscheidbar ist (gleicher Name, uneinheitlicher Typ), wird automatisch zusammengeführt und bleibt per Event umkehrbar. Was ein Urteil braucht, wartet, bis es etwas zu lesen gibt — siehe P12 |
| P10 | Strukturierung über OpenRouter mit ZDR, bewusst akzeptiert | Die NAS (Celeron, keine GPU) kann kein LLM; das Event Log erlaubt späteren Wechsel auf lokal |
| P11 | Kein Morning Brief, keine Push-Benachrichtigungen — mit **zwei** Ausnahmen: die Wochenrückschau meldet sich einmal pro Woche, zu Tag und Uhrzeit, die der Nutzer selbst setzt; und vor einem Termin, zu dem eine offene Absicht passt, kommt ein Banner (davor und danach je einmal). Beide abschaltbar | Jede Benachrichtigung ist eine Gewohnheit, die extra aufgebaut werden muss. Beide Ausnahmen (vom Nutzer am 23.09.2026 gewählt) hängen an einem Termin, den er ohnehin hat, statt einen neuen zu erfinden. Beide sagen nichts über den Inhalt der Notizen — die zweite nennt nur den Termin aus dem eigenen Kalender: Lesen ist hinter der Sperre, ein Mitteilungsbanner nicht. Siehe `docs/prospective-memory.md` |
| P13 | Absichten („muss ich Paul noch fragen") werden beiläufig erkannt und hängen an Menschen und Themen, nicht an Uhrzeiten; sie melden sich bei der nächsten Erwähnung und vor passenden Terminen | Der Moment, in dem Wissen zählt, gehört der Welt, nicht der App. Keine andere Notiz- oder Erinnerungs-App verbindet, was beiläufig gesagt wurde, mit dem Menschen, den man gleich trifft. Erkennung ist ein Modell-Urteil und deshalb sofort sichtbar und mit einem Griff verwerfbar; erledigt wird nur durch den Nutzer (docs/prospective-memory.md, F1–F12) |
| P12 | Die Schwelle für automatische Urteile ist **Beobachtungen pro Entität**, nicht die Gesamtzahl der Captures | Am 22.09.2026 gemessen: bei 45 Captures hatten **70 von 78 Entitäten genau eine Beobachtung**. Ein Modell, das auf je einem Satz pro Seite entscheidet, ob zwei Dinge dasselbe sind, rät — und schreibt das Ergebnis automatisch in den Graphen |

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
| Ein Modell, das **abwägt**, welche Entitäten dasselbe sind | Ein Prozess, der unbeaufsichtigt Beziehungen im eigenen Gedächtnis erfindet, ist ein Halluzinationsgenerator mit Schreibrechten. Die frühere Fassung dieser Zeile nannte 1.000 Captures als Schwelle; ersetzt durch P12, weil die eigentliche Größe die Beweisdichte ist und nicht die Kapitelzahl. **Nicht** out of scope ist dagegen die *deterministische* Konsolidierung — gleicher Name, uneinheitliches Typ-Vokabular —, die nichts abwägt und deshalb auch nichts erfinden kann. |
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

- Ab welcher Menge lohnt der Graph? Nach 1.000 Captures neu bewerten,
  nicht vorher. Für *Konsolidierung* ist diese Zahl seit dem 22.09.2026
  durch P12 ersetzt: dort zählt, wie viel an einer Entität steht, nicht
  wie viel insgesamt erfasst wurde.
- Wie viele Echo-Treffer sind zu viele? Startwert 3, Schwelle empirisch.
- Braucht iOS eine eigene App (lokales Whisper) oder reicht der Kurzbefehl
  mit Apple-Diktat? Erst nach dem Mac-Pfad entscheiden.
- Bleibt `current_summary` als Feld bestehen oder wird es zur Lesezeit
  berechnet? Siehe `docs/memory-model.md`.
