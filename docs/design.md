# Das Interface

Stand: 2026-09-23. Dieses Dokument sagt, **wo jede Funktion geblieben
ist** und **warum es so aussieht, wie es aussieht**. Es geht der
Umsetzung nicht vor wie `docs/product.md` — es hält fest, was entschieden
wurde, damit man nachprüfen kann statt zu glauben.

## Warum überhaupt

Der alte Zuschnitt war ein Kostüm: eine Monospace-Schrift für alles,
Scanlines über jeder Fläche, Bedienelemente als ASCII gezeichnet
(`[ New Capture ]`, `[x] depth`). Drei Befunde haben den Umbau ausgelöst.

1. **Die Hierarchie stand auf dem Kopf.** Laut ADR 0004 ist das
   Gesprochene das Original. Es rendelte als 13px, gedämpft, monospace —
   unscheinbarer als die goldenen Etiketten drumherum.
2. **Nichts war mit der Tastatur erreichbar.** Jedes Bedienelement war
   ein `<span onClick>`. Tab sprang über alle hinweg.
3. **Der Graph war eine zweite App.** Eigener Hintergrund, eigene
   Scanlines — im CSS stand wörtlich, dass er sich einer eigenen Bildwelt
   verschreibt. Genau deshalb fügte er sich nicht ein.

## Die Orte

Aus acht Reitern wurden fünf, plus ein Knopf.

| Ort | Was er beantwortet |
|---|---|
| **Capture** (Knopf, kein Ort) | „Mir ist gerade etwas eingefallen." |
| **Today** | „Was ist gerade dran?" — das Einzige, was von selbst spricht |
| **Search** | „Wo ist das nochmal?" — leer zeigt es alles, nach Tagen |
| **Write** | „Ich sitze vier Stunden in einem Workshop." |
| **Graph** | „Womit hängt das zusammen?" |
| **Settings** | alles, was keine Notiz ist |

**Capture ist kein Ort.** Der Shortcut ist der Aufnahmeknopf (ADR 0012),
also öffnet Aufnehmen über dem, wo man gerade war, und schließt dorthin
zurück. Als Reiter war die Handlung, die man zwanzigmal am Tag macht,
zugleich die einzige, die wegwirft, wo man war. Der Knopf oben in der
Leiste ist der sichtbare Beweis, dass es den Shortcut gibt — und der Weg
hinein auf dem Telefon, im Browser-Build und wenn die Tastenkombination
belegt ist.

## Inventar: wo jede Funktion geblieben ist

Nichts darf ohne Eintrag verschwinden. Wenn hier etwas fehlt, ist es ein
Fehler, keine Entscheidung.

| Vorher | Jetzt | Anmerkung |
|---|---|---|
| Sidebar → `[ New Capture ]` | Capture-Knopf oben in der Leiste | |
| Sidebar → Laufende Arbeit | unverändert in der Leiste | |
| Sidebar → `[ lock now ]` | Leiste unten; auf dem Telefon in Settings | |
| Capture: aufnehmen, tippen, verwerfen, sichern | unverändert, als Overlay | |
| Capture: Echo direkt danach | unverändert | |
| Resurface: „kommt auf dich zu" | **Today**, oberster Abschnitt | |
| Resurface: „darauf kommst du zurück" | **Today**, zweiter Abschnitt | |
| Timeline: alles nach Tagen | **Search**, Leerzustand | war dieselbe Frage ein drittes Mal |
| Search: Anfrage, Typfilter, Treffer | **Search**, unverändert | |
| Relations: Graph, Suche, Filter, Merge | **Graph**: Map als Einstieg, Klick → Orbit | siehe „Der Graph“ |
| Tidying: Vorschau, Vorschläge, Anwenden | **Graph**, Schublade rechts | es geht um den Graphen |
| Tidying: Protokoll und Rückgängig | **Graph**, dieselbe Schublade | |
| Chat | **entfernt** | war „soon" in der Hauptnavigation |
| Capture-Detail (alles) | unverändert im Umfang, neu gesetzt | echte Knöpfe, Audio als Range-Input |
| Entity-Detail (alles) | unverändert im Umfang, neu gesetzt | Relationen zusammengefasst, Zeitleiste |
| Settings (alles) | unverändert | |
| — | **Write**, neu | siehe unten |

## Write: eine Sitzung

Der Fall, für den es vorher nichts gab: vier Stunden Workshop, laufend
mitschreiben, am Ende einmal abschicken. Zwei Entscheidungen tragen den
Bildschirm, und beide sind darauf zu sehen.

**Es bleiben mehrere Notizen, nicht eine.** Eine Leerzeile trennt Blöcke;
jeder Block wird ein eigenes Capture. Vier Stunden als eine Textwand
würden das Echo ertränken — es vergleicht ganze Captures — und der
Extraktion nichts geben, woran sie sich festhalten kann.

**Jede Notiz behält die Zeit, zu der sie geschrieben wurde.** Nicht die
Sendezeit. Die Uhrzeiten stehen im Bund, während man tippt, damit das
eine sichtbare Tatsache ist und keine Behauptung. Stünde überall 14:00,
würde die Timeline darüber lügen, wann man etwas gedacht hat — und die
Timeline ist das Meiste, wofür es dieses System gibt.

Der Entwurf liegt über `client/src-tauri/src/draft.rs` auf der Platte,
bewusst **nicht** in der Outbox: die hält fertige Captures, die nicht
mehr verändert werden können, hier liegt ein Dokument, das sich bei jedem
Tastendruck ändert. Geschrieben wird über eine Temp-Datei mit Rename, ein
abgebrochener Schreibvorgang lässt den vorherigen Entwurf unangetastet.

Eine Sitzung ist **keine Entität** und erscheint nicht im Graphen. Die
Extraktion findet Entitäten in dem, was *gesagt* wurde; eine Überschrift,
die in ein Textfeld getippt wurde, wurde nicht gesagt. Einen Knoten daraus
zu machen hieße, dass die Oberfläche eine Vermutung in den Graphen
schreibt — die Grenze, die P12 zieht. Der Titel liegt in
`capture_sessions`, wo er änderbar und löschbar ist; ins Event-Payload
geht nur die Id (ADR 0005: das Log wird nie verändert).

## Das Fundament

`client/src/styles/tokens.css` ist die einzige Datei, die eine Farbe,
eine Größe oder eine Dauer benennen darf. Vorher gab es zwölf
Schriftgrade zwischen 10 und 14px in derselben Ansicht — nicht weil das
jemand so wollte, sondern weil es keinen Ort gab, an dem der richtige
Wert stand.

Die Ebenen (`@layer tokens, base, components, screens`) sind das zweite
Stück davon: ein Screen schlägt immer eine Komponente, eine Komponente
immer die Basis, und keiner muss den anderen mit mehr Selektoren
überbieten. Das alte Blatt trug ein `:not(.graph-ring)` mit sich herum,
nur damit eine allgemeine Regel eine spezielle nicht übermalt.

### Drei Schriften, drei Aufgaben

| Schrift | Wofür |
|---|---|
| **Fraunces** | Deine Worte, und die Namen von Dingen |
| **Instrument Sans** | Alles, was die Oberfläche sagt |
| **JetBrains Mono** | Maschinenwahrheit: Zeitstempel, Zahlen, Ids |

Mono auf Fließtext war der Grund, warum jeder Bildschirm dieselbe flache
Textur hatte. Die Schriften liegen im Bundle (206 KB), nicht im CDN: eine
App, deren Audio den Mac nicht verlässt, darf beim Start keine Schrift
nachladen.

### Vier Akzente, je eine Aufgabe

| Farbe | Bedeutung |
|---|---|
| **Clay** | Handlungen, und was im Fokus steht |
| **Ember** | Zeit: wann gesagt, wann fällig |
| **Moss** | Von der Maschine abgeleitet — nie deine eigenen Worte |
| **Plum** | Personen, und der vierte Entitäts-Farbton |

Entitätsfarben kommen aus diesen vieren, über einen FNV-1a-Hash des
Typnamens. Vorher hing es an der Reihenfolge, in der die laufende Sitzung
den Typen zum ersten Mal begegnete — nach jedem Neustart eine andere
Farbe, während der Kommentar oben in `entityType.ts` das Gegenteil
behauptete. Farbe war damit das Einzige auf dem Schirm, das man nicht
lernen konnte.

### Bedienelemente

Echte `<button>`, `<input>`, `<label>`, `<input type="checkbox">`. Ein
2px-Ring in Ember auf `:focus-visible`, durch die Grundfarbe abgesetzt.

Ein Chip, der eine Liste *einengt*, ist nicht ausgewählt — nur leise. Ein
Chip, der einen Teil eines Bildes *versteckt* (die Typfilter im Graphen),
ist durchgestrichen: dort fehlt dem Bild vor dir etwas, und du musst
sehen können, was.

## Die Marke

Ein Gehirn im Seitenprofil, nach links. Beide Vorgänger wurden aus einer
gespiegelten Hälfte gebaut und liefen unten mittig spitz zu — das ist die
Konstruktion eines Herz-Glyphs, und beide lasen sich auch so. Ein Gehirn
erkennt man am Profil: asymmetrisch, zwölf Wölbungen entlang der Kontur
(die Gyri *sind* die Silhouette, deshalb überleben sie das
Herunterskalieren), ein langer Sulcus, ein Kleinhirn hinten und ein Stamm
außermittig.

Die Geometrie liegt in `client/src/brand/mark.json` und nirgends sonst.
`scripts/render-brand.py` rendert daraus App-Icon, `.icns` und die
Menüleisten-Vorlage — Fenster und Menüleiste können nicht mehr
auseinanderdriften.

Das Zeichen bewegt sich nie von selbst. Es blinkt und wippt nicht; es
sagt nur, wenn etwas wirklich passiert: `listening`, solange das Mikrofon
offen ist, `thinking`, solange das Backend an etwas arbeitet. In der
Menüleiste steht es vollkommen still und ein Punkt daneben atmet — die
Vorlage reserviert den Platz dafür, damit das Element nie die Breite
wechselt.

## Auf dem Telefon

Die 76px-Leiste wird eine Tab-Bar unten, der Capture-Knopf wandert in die
Mitte, wo ein Daumen ohnehin ist. Sonst ändert sich nichts: jeder Screen
ist schon eine Spalte mit Maß. Typo geht eine Stufe herunter, Abstände,
Radien, Farben und Bedienelemente bleiben, jedes Ziel ist mindestens
44px.

## Der Graph

**Er öffnet auf der Map, nicht auf einem Orbit.** Der erste Wurf öffnete
zentriert auf das zuletzt Besprochene. Oben stand dann nur „Lena“ —
und das las sich, als sei der Graph schon auf eine Person gefiltert,
bevor man irgendetwas angefasst hatte. Jetzt ist „Everything“ der erste
Schritt jedes Wegs und der Weg zurück; ein Klick auf einen Punkt oder
Namen der Map öffnet dessen Orbit.

**Map:** jede Art mit mindestens zwei Einträgen bekommt eine eigene
Scheibe, die Einzelstücke teilen sich „One-off kinds“. Die Scheiben
werden gegeneinander gepackt (größte in die Mitte, jede weitere an die
freie Stelle, die der Mitte am nächsten ist) und das Ganze auf das
Fenster skaliert. Namen stehen nur an dem, was mehr als einmal gesagt
wurde, und nur dort, wo sie nichts überdecken — eine echte
Kollisionsprüfung, keine Versätze.

**Orbit:** beide Ringe passen immer ins Bild (`ringRadii` in `orbit.ts`),
Namen zeigen radial nach außen, Relationen suchen sich entlang ihrer
Speiche einen freien Platz über oder unter ihr. Zwei Relationen zum
selben Nachbarn ergeben einen Knoten und eine Linie, nicht zwei.

## Was noch aussteht

- Das Telefon-Layout ist mitgedacht, aber nicht durchgesehen.
- Die Einzelstück-Arten („One-off kinds“) sind ein Symptom des
  Typ-Vokabulars aus der Extraktion; die Konsolidierung
  (`docs/consolidation.md`) ist der Ort, das zu beheben, nicht die
  Oberfläche.
