# Memory Model

Welche Konzepte das System kennt, welche es bewusst *nicht* kennt, und wie
sie auf Events und Projektionen abgebildet werden.

## Konzepte

Nur diese sechs. Alles andere ist Ableitung oder Darstellung.

| Konzept | Definition | Veränderlich? |
|---|---|---|
| **Capture** | Ein Erfassungsvorgang. Die Aufnahme selbst (Audio) oder ein direkt eingegebener Text. | Nie |
| **Transcript** | Text zu einem Capture. Ergebnis eines ASR-Modells — bereits Interpretation. | Neue Fassungen, alte bleiben |
| **Entity** | Etwas, worüber wiederholt gesprochen wird: Person, Projekt, Thema, Ort, Rezept. | Identität kann korrigiert werden |
| **Observation** | Was ein einzelner Capture über eine Entity aussagt. Immer an Quelle und Modell gebunden. | Nie, nur ergänzt |
| **Relation** | Eine Verbindung zwischen zwei Entities, mit Quelle und Modell. | Vorgeschlagen → bestätigt/abgelehnt |
| **Echo** | Menge semantisch naher früherer Captures zu einem neuen Capture. Nicht gespeichert, zur Laufzeit berechnet. | n/a |

### Bewusst nicht modelliert

- **Knowledge als eigene Ebene.** „Was glaube ich aktuell" ist die Summe der
  Observations einer Entity, geordnet nach Zeit. Eine zusätzliche
  Belief-Ebene mit Confidence-Werten wäre ohne Kalibrierungsdaten geraten.
- **Decay.** Nichts wird schwächer oder verschwindet. Bei Bedarf Ranking.
- **Reconsolidation.** Bedeutet, dass Erinnerung sich beim Abruf verändert —
  das exakte Gegenteil des Kernprinzips.
- **Zustandsmaschine `NEW → CAPTURED → INTERPRETED → …`.** Das ist ein
  Boolean („strukturiert ja/nein"), verkleidet als Neurobiologie.

## Schichten

```
capture.recorded         Audio/Text — Original, unantastbar
      ↓
transcript.derived       ASR-Ausgabe — erste Interpretation, mit Modell
transcript.corrected     Mensch überschreibt, Original bleibt daneben
      ↓
entity.* / relation.*    LLM-Ausgabe — zweite Interpretation, mit Modell
      ↓
Projektionen             entities, observations, relations, capture_search
```

Jede Schicht kennt ihre Quelle und das Modell, das sie erzeugt hat. Jede
Schicht außer der ersten ist aus der jeweils darüberliegenden vollständig
neu ableitbar. Das ist der einzige Grund, warum dieses System Event Sourcing
benutzt — ohne einen tatsächlich implementierten Re-Derivation-Pfad wäre die
`events`-Tabelle bloß ein Audit-Log (siehe ADR 0003 und Issue „Re-Derivation").

## Event-Typen

### Erfassung
- `capture.recorded` — `{ origin: "audio"|"text", content_ref, device, duration_ms }`.
  Enthält **keinen** Text mehr (Änderung gegenüber heute, siehe ADR 0004).
- `transcript.derived` — `{ capture_event_id, model, language }`
- `transcript.corrected` — `{ capture_event_id, supersedes, corrected_by: "user" }`
- `capture.redacted` — `{ capture_event_id, scope: "content"|"all", reason? }`

### Ableitung
- `entity.created` — `{ entity_type, name }` *(existiert)*
- `entity.observed` — `{ text, source_event_id }` *(existiert)*
- `relation.proposed` — `{ from_entity_id, to_entity_id, relation_type }` *(existiert)*
- `relation.confirmed` / `relation.rejected` — `{ decided_by: "user" }`

### Identität
- `entity.merge_proposed` — `{ from_entity_id, into_entity_id, score, reason }`
- `entity.merged` — `{ from_entity_id, into_entity_id, decided_by }`
- `entity.unmerged` — `{ merge_event_id }` — jede Verschmelzung ist umkehrbar

## Wo Inhalte liegen — und warum nicht im Event Log

`events` ist per DB-Regel gegen UPDATE und DELETE gesperrt. Redaktion
(bewusstes Entfernen von Inhalten) wäre damit unmöglich — außer man weicht
die Regel auf und verliert jede Aussage über Unantastbarkeit.

**Auflösung:** Das Event Log enthält nur *Struktur und Verweise*, niemals
Inhalte. Inhalte liegen in eigenen Tabellen, die auf die Event-ID verweisen:

```
events                 (unantastbar: was passierte, wann, durch wen/welches Modell)
capture_content        (event_id → audio_path | text)        ← redigierbar
transcript_content     (event_id → text, model, superseded_by) ← redigierbar
```

Redaktion nullt den Inhalt in `capture_content`/`transcript_content`, löscht
abgeleitete Observations und hängt ein `capture.redacted`-Event an. Die Kette
der Ereignisse bleibt lückenlos und prüfbar; der Inhalt ist weg. Siehe
ADR 0005.

## Projektionen

Bestehend: `entities`, `observations`, `relations`, `capture_search`,
`entity_type_registry`.

Zu ändern:
- `capture_search.transcript` muss künftig die **aktuelle** Transkriptfassung
  spiegeln und bei jeder Korrektur neu eingebettet werden.
- `entities.current_summary` wird heute von der jeweils letzten Observation
  überschrieben (`structuring.rs:151`). Das ist Last-write-wins und
  widerspricht der Idee einer konsolidierten aktuellen Sicht. Entweder das
  Feld entfällt und die Sicht wird zur Lesezeit aus den Observations
  gebildet, oder es braucht einen echten Konsolidierungsschritt. Bis dahin:
  Feld nicht als „aktuelle Überzeugung" interpretieren.

Neu:
- `entity_merge_queue` — offene Verschmelzungsvorschläge für die Review-Queue.

## Entitäts-Identität

Heute: `where entity_type = $1 and lower(name) = lower($2)` — exakter
String-Vergleich (`structuring.rs:99`). Bei 5.000 Captures im Jahr zerfallen
Namen dadurch in unverbundene Fragmente („Lena", „Lena M.", ASR-Müll),
und der Graph bleibt nicht falsch, sondern leer.

Zielverfahren:
1. Exakter Treffer → automatisch zuordnen.
2. Sonst Kandidaten über Namensähnlichkeit (`pg_trgm`) und Embedding-Nähe.
3. Über Schwellwert → `entity.merge_proposed`, landet in der Review-Queue.
4. Mensch entscheidet, `entity.merged` oder verworfen. Immer umkehrbar.

Dieselbe Oberfläche bedient Transkript-Korrekturen. Ein Mechanismus, zwei
Probleme.
