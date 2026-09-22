# Entitäts-Auflösung: Implementierungsplan

Stand 22. September 2026. Setzt `docs/consolidation.md` um — das sagt
*warum* und *was der Lauf verändern darf*; dieses Dokument sagt *wie*, in
welcher Reihenfolge, und woran man erkennt, dass eine Stufe fertig ist.

Ausgelöst durch den Nutzer:

> „Ich erfasse einmal zum Beispiel Kaffee und ich erfasse einmal Espresso,
> da sind die beiden Knoten momentan getrennt … Das habe ich beispielsweise
> auch bei Tippfehlern oder wenn Namen anders geschrieben werden, einmal
> nenne ich den Kollegen Paul, einmal nenne ich ihn Paul Hartmann."

---

## Der Befund, an den echten Daten

78 Entitäten, 32 Typen. `pg_trgm` über alle Paare, Ähnlichkeit > 0,3:

| Paar | Ähnlichkeit | Was es ist |
| --- | --- | --- |
| Sauna (Ort) ↔ Sauna (Aktivität) | 1,00 | Dublette |
| Northwind (Organisation) ↔ (Kunde) | 1,00 | Dublette |
| Kardamom-Espresso (Idee) ↔ (Getränk) | 1,00 | Dublette |
| Hotkey-Test (Thema) ↔ (Test) | 1,00 | Dublette |
| Espresso mit Kardamom ↔ Kardamom-Espresso | 0,82 | Dublette |
| Hippocampus ↔ Hippocampus Projekt | 0,60 | Dublette |
| **Kardamom-Espresso ↔ Espresso** | **0,50** | **Relation** (Gattung) |
| **Kardamom-Espresso ↔ Kardamom** | **0,50** | **Relation** (Zutat) |
| Aufguss ↔ Finnischer Aufguss | 0,42 | Dublette |
| **Northwind Abrechnungsprojekt ↔ Northwind** | **0,41** | **Relation** |
| **Hafenportal ↔ HPortal** | **0,38** | **nichts** |
| **Cardamom Buns ↔ Kardamom** | **0,35** | **Relation** (Zutat) |

**Daraus folgt die zentrale Entwurfsentscheidung.** Es gibt keine
Schwelle, die diese drei Gruppen trennt: `Kardamom-Espresso ↔ Espresso`
(0,50) darf *nicht* verschmelzen und liegt **über** `Aufguss ↔
Finnischer Aufguss` (0,42), das verschmelzen *muss*. Und `Hafenportal ↔
HPortal` (0,38) liegt knapp darunter und ist gar nichts.

Das ist formgleich mit dem Echo-Problem, das dieses Projekt schon einmal
hatte: *cardamom buns ↔ Sauna* stand bei Kosinus 0,871 über *finnischer
Aufguss ↔ Sauna* bei 0,849. Die Lösung war nicht eine bessere Schwelle,
sondern ein Modell, **das beide Texte liest** (#25). Hier gilt dasselbe:

> **Ähnlichkeit erzeugt Kandidaten. Entscheiden muss ein Leser.**

### Vier Urteile, nicht zwei

Entschieden am 22.09.2026. Das Modell bekommt beide Entitäten samt ihren
Beobachtungen und antwortet mit einem von vier Urteilen:

| Urteil | Folge | Beispiel aus den Daten |
| --- | --- | --- |
| `same` | Verschmelzung, Alias bleibt | Aufguss ↔ Finnischer Aufguss |
| `narrower` | `is_a`-Kante, beide bleiben | Kardamom-Espresso → Espresso |
| `related` | generische Kante, beide bleiben | Northwind Abrechnungsprojekt → Northwind |
| `different` | **festhalten**, nie wieder fragen | Hafenportal ↔ HPortal |

`different` wird gespeichert, nicht verworfen: sonst zahlt jeder Lauf
erneut dafür, dasselbe Nicht-Ergebnis herauszufinden.

### Warum Kaffee und Espresso *nicht* verschmelzen

Weil man danach nicht mehr wüsste, ob von einem Espresso die Rede war oder
von Filterkaffee. In einem Gedächtnissystem ist das ein schlimmerer
Verlust als die Fragmentierung, die es behebt — und praktisch unumkehrbar,
weil ein falsch verschmolzener Graph *stimmig aussieht*. `consolidation.md`
hat das über Sauna und Finnischen Aufguss bereits so entschieden; hier
steht es noch einmal, weil die Formulierung „zusammenfassen" beide Fälle
umfasst und sie auseinandergehalten werden müssen.

---

## Weitere Entscheidungen, 22.09.2026

**Der Lauf verschmilzt automatisch, ohne Rückfrage, und alles ist per
Event umkehrbar.** Bestätigung der Entscheidung aus `consolidation.md`.
Der Einwand dagegen wurde vorgebracht und verworfen: Umkehrbarkeit hilft
nur, wenn man den Fehler bemerkt.

Daraus folgt eine Pflicht, die damit einhergeht und in `consolidation.md`
schon als Anforderung steht — *„man soll schon auch irgendwie erkennen
können, wie sich mein Graph im Laufe der Zeit entwickelt hat"*: **was der
Lauf verändert hat, muss sichtbar und mit einem Griff rücknehmbar sein.**
Ohne das ist „automatisch und umkehrbar" nur die erste Hälfte. Das ist
Stufe 6 und gehört nicht ans Ende, sondern in denselben Zug wie Stufe 5.

**Benennung:** dasselbe Urteil, das verschmilzt, wählt auch den
überlebenden Namen — meist die vollständigere Form. **Jede frühere
Schreibweise bleibt als Alias durchsuchbar.** Ohne das tauscht man
Fragmentierung gegen Unauffindbarkeit: „Paul" fände den Kollegen nicht
mehr, sobald der Knoten „Paul Hartmann" heißt.

**Reihenfolge: erst vermeiden, dann reparieren.** Vier der Paare oben
existieren ausschließlich wegen des frei erfundenen Typ-Vokabulars, und
„Espresso mit Kardamom" entstand neben „Kardamom-Espresso", weil die
Extraktion nicht weiß, was es schon gibt. Was gar nicht erst zerfällt,
muss nicht zusammengeführt werden.

---

## Stufe 1 — Auflösen beim Schreiben

*Verhindert neue Fragmentierung. Wirkt ab der nächsten Notiz.*

Heute: `structuring.rs` sucht vor dem Anlegen nach `entity_type = $1 and
lower(name) = lower($2)` — exakter Name **und** exakter Typ. Der
Extraktions-Prompt enthält nichts über den bestehenden Graphen, also
erfindet das Modell Namen und Typen pro Notiz neu.

1. **Typ-Vokabular in den Prompt.** Die bestehenden Einträge aus
   `entity_type_registry` werden mitgeschickt, mit der Anweisung, einen
   davon zu verwenden, wenn einer passt. Skaliert: die Typenliste bleibt
   klein (heute 32, Ziel ~8–12), auch bei 5.000 Entitäten.
2. **Bekannte Nachbarn in den Prompt.** Kandidaten über
   `word_similarity(entity.name, transcript)` — genau der pg_trgm-Operator
   für „kommt dieser Name irgendwo in diesem Text vor, auch unsauber
   geschrieben" — plus die zuletzt berührten Entitäten, weil man in einer
   Sitzung über dieselben Dinge spricht. Gedeckelt (~40 Einträge: Name,
   Typ, kurze Zusammenfassung).
3. **Unscharfes Auflösen beim Anlegen.** Der exakte Vergleich wird
   erweitert: gleicher Name bei *abweichendem* Typ löst auf dieselbe
   Entität auf (behebt alle vier 1,00-Paare), und oberhalb einer hohen
   Trigramm-Schwelle ebenfalls. Unterhalb davon wird angelegt — und Stufe
   3 bis 5 kümmern sich darum.
4. **Entitäts-Embeddings endlich schreiben.** `entities.embedding` ist
   seit Migration 0001 deklariert und hat einen HNSW-Index, wird aber
   **nirgends gefüllt** — 0 von 78. Stufe 3 braucht die Spalte, und
   Kandidaten in Schritt 2 werden damit besser.

*Fertig, wenn:* zwei Notizen über denselben Gegenstand mit
unterschiedlicher Formulierung auf einer Entität landen, und eine neue
Notiz keinen neuen Typ erfindet, für den es schon einen gibt.

### Gebaut und gemessen, 22.09.2026

Drei Notizen gegen den bestehenden Graphen (78 Entitäten), jeweils in
einer Formulierung, die vorher garantiert einen neuen Knoten erzeugt
hätte, weil der alte Vergleich exakt auf Name *und* Typ ging:

| Gesagt | Vorher | Jetzt aufgelöst auf |
| --- | --- | --- |
| „Paul **Hartmann** hat mir geschrieben" | neuer Knoten | **Paul** (Person) |
| „ein **Kardamom Espresso**" (ohne Bindestrich) | neuer Knoten | **Kardamom-Espresso** (Getränk) |
| „am **Hippocampus-Projekt**" | neuer Knoten | **Hippocampus Projekt** (Projekt) |
| „das **Abrechnungsprojekt** bei Northwind" | neuer Knoten | **Abrechnungs Projekt** (Projekt) |
| „**Northwind**" | — | **Northwind** (Kunde) |

**Ergebnis: 78 Entitäten vorher, 78 danach. Kein einziger neuer Knoten.**

Bemerkenswert daran ist, *welcher* Mechanismus gegriffen hat: keiner der
unscharfen Fallbacks in `resolve_existing` hat geloggt. Das Modell selbst
hat die bestehenden Namen zurückgegeben, weil es sie im Prompt sah. Die
Auflösung passiert also an der Quelle, nicht in der Reparatur — was auch
der billigere Ort ist.

Der Fallback ist trotzdem richtig kalibriert, gemessen an denselben Daten:
reine Schreibweisen-Unterschiede erreichen Trigramm-Ähnlichkeit 1,00
(`Hippocampus-Projekt` ↔ `Hippocampus Projekt`, `Logging Probe` ↔
`Logging-Probe`) und greifen. Wortumstellungen wie `Espresso mit Kardamom`
↔ `Kardamom Espresso` bleiben bei 0,82 darunter und gehen damit ans Urteil
in Stufe 4 — richtig so, denn eine umgestellte Wortfolge ist eine
Entscheidung, keine Rechtschreibvariante.

**Was Stufe 1 ausdrücklich nicht tut:** den Bestand reparieren. Die vier
Paare mit Ähnlichkeit 1,00 (Sauna, Northwind, Kardamom-Espresso,
Hotkey-Test) stehen weiterhin doppelt in der Datenbank. Dafür sind die
Stufen 3 bis 5 da.

## Stufe 2–6 — gebaut am 22.09.2026

Der Nutzer hat den Zuschnitt korrigiert, und die Korrektur war besser als
der Plan. Wörtlich:

> „Ich möchte da keine fixen Paare haben, die da vorgegeben sind, sondern
> ich möchte einen Lauf, einen Job, der sich zyklisch die Themen einmal
> anguckt, die Knoten, und die eigenständig zusammenlegt — schön mit einem
> guten System-Prompt, damit das kein Chaos wird … Das System soll
> eigenständig leben und arbeiten … Ich habe unveränderliche
> Startprimitiven, Changelog auf die Repräsentation meines Wissens."

Die von mir entworfene 32→13-Typabbildung wurde damit verworfen. Zu
Recht: eine einmal festgelegte Liste beschreibt ein Leben ab dem Moment
nicht mehr, in dem sich dieses Leben ändert. Der Lauf leitet das
Vokabular stattdessen selbst her und darf neue Typen und neue
Beziehungswörter prägen.

### Was gebaut ist

| | |
| --- | --- |
| `entity_alias` | jeder Name, den eine Entität je trug, bleibt durchsuchbar |
| `entities.merged_into` | verschmelzen löscht nie, es zeigt |
| `entities.last_consolidated_at` | die Markierung, nach der gefragt wurde: `null` = nie angesehen |
| `entity_merge_block` | ein zurückgenommener Merge wird nie wieder vorgeschlagen |
| `consolidation.rs` | exakter Durchgang, dann Modell-Durchgang, dann Markierung |
| `GET /consolidation` | das Änderungsprotokoll |
| `GET /consolidation/preview` | derselbe Lauf, der nichts verändert |
| `POST /entities/{id}/merge` | von Hand, im Graphen |
| `POST /entities/{id}/unmerge` | zurücknehmen |

Die Arbeitsmenge ist kein Zeitfenster, sondern der betroffene Teilgraph,
wie `consolidation.md` es verlangt: die fälligen Entitäten
(`last_consolidated_at` zuerst `null`, dann am längsten her) plus deren
Nachbarschaft über Namensähnlichkeit *und* Embedding-Nähe. Beides, weil
sie unterschiedlich versagen — Trigramme übersehen „Sauna" neben
„Aufguss", Embeddings übersehen einen Tippfehler in einem Eigennamen.

### Gemessen am 22.09.2026

**Der erste Trockenlauf hat einen Fehler gefunden, und genau dafür war er
da.** Das Modell wollte `Northwind Abrechnungsprojekt` in `Northwind`
verschmelzen, mit der Begründung „beschreiben dasselbe Projekt beim
Kunden" — ein Argument für eine Kante und gegen eine Verschmelzung, im
selben Satz. Der Prompt verbietet das wörtlich und es passierte trotzdem.

Daraus folgt eine Regel, die in Code steht und nicht nur im Prompt
(`crosses_types`): **zwei verschieden benannte Dinge verschiedenen Typs
werden nicht verschmolzen.** Die beiden legitimen Formen überleben —
gleicher Name/anderer Typ (`Sauna` als `Ort` und `Aktivität`) und anderer
Name/gleicher Typ (`Hippocampus` und `Hippocampus Projekt`).

Danach, auf demselben Bestand:

| | |
| --- | --- |
| Durch exakte Namensgleichheit verschmolzen | 4 |
| Durch Urteil verschmolzen | 1 (`Hippocampus Projekt` → `Hippocampus`) |
| Falsche Verschmelzung abgelehnt | 1 |
| **Kanten über Capture-Grenzen hinweg** | **11** |

Die Kanten sind der eigentliche Gewinn, weil sie vorher *unmöglich* waren
— `structuring.rs` kann nur verbinden, was in einem Satz vorkam, weshalb
ADR 0006 den Graphen als „eine Menge unverbundener Sterne" vorhersagte:

- `Finnischer Aufguss —gehört zu→ Sauna` — das Paar, mit dem der Nutzer
  dieses Thema eröffnet hat. Verbunden, nicht verschmolzen.
- `Northwind Abrechnungsprojekt —gehört zu→ Northwind` — aus dem
  abgelehnten Merge wurde die richtige Antwort.
- `Test-Instanz —gehört zu→ Northwind Abrechnungsprojekt`
- `Konzept —wird gefordert von→ Ole`

Dafür musste `relations.source_event_id` nullable werden (Migration
0008). Eine Kante, die aus mehreren Notizen gelesen wurde, hat keine
einzelne Quelle, und eine zu erfinden wäre eine Lüge über Herkunft in
genau der Tabelle, in der Herkunft der Punkt ist. Die Oberfläche zeigt
dort `[across notes]` statt `[why]`.

### Von Hand, im Graphen

Auf Wunsch des Nutzers. Im Relations-Graphen: Knoten fokussieren,
`[ merge into… ]`, den Zielknoten anklicken, bestätigen. Zwei Klicks und
eine Rückfrage statt eines Ziehens — auf einer Fläche, auf der Ziehen
schon „verschieben" heißt, wäre ein Zieh-Merge einen Ausrutscher vom
falschen Paar entfernt.

Für den Menschen gelten die Sicherungen des automatischen Laufs
ausdrücklich **nicht**: `crosses_types` lehnt auch manche richtigen
Merges ab, und wer beide Entitäten vor sich hat, hat die bessere
Information. Ein bestehender Block wird dabei aufgehoben.

Belegt: `Abrechnungs Projekt` von Hand in `Northwind Abrechnungsprojekt`
gefaltet — Beobachtungen übernommen, der alte Name bleibt Alias, nichts
gelöscht.

### Offen

- **Die NAS ist nicht vermessen.** Alles oben ist gegen die lokale
  Entwicklungsdatenbank gelaufen (`localhost:5433`), die echte Notizen
  enthält, aber einen anderen Stand als das Zielsystem. Vor dem
  Scharfschalten dort: `GET /consolidation/preview`.
- **Stufe 7**, rollierende Zusammenfassung pro Entität. Erst nötig, wenn
  eine Entität nicht mehr samt ihren Beobachtungen in einen Prompt passt.
- **Re-Derivation** des Bestands mit dem neuen Extraktions-Prompt (Phase 2
  der Roadmap) — entschieden, aber noch nicht gebaut.

## Stufe 7 — rollierende Zusammenfassung und Nachbarschaftslauf
