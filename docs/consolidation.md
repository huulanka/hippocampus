# Konsolidierung und Zeitbewusstsein

Scope-Dokument, noch nichts davon gebaut. Entstanden aus einer Beobachtung
des Nutzers nach den ersten echten Sprachnotizen:

> „Viele Sachen stehen dann doch irgendwo miteinander in einer Beziehung,
> die aber nicht ersichtlich ist, wenn ich nur Message für Message an das
> KI-Modell gebe zum Beziehungsauflösen. Wenn ich jetzt aber mehrere gleich
> mitgebe, dann wird das auch klar."

Und, früher, zum selben Thema:

> „Wenn einmal finnischer Aufguss als Erlebnis gekennzeichnet ist und einmal
> Sauna als Aktivität … beides an sich ist erst mal nicht falsch, aber wenn
> man sich das gesamte Wissensnetzwerk anguckt, ist das eben unscharf."

---

## Der Befund, gemessen

Stand 21. September 2026, 38 Captures in der Entwicklungsdatenbank:

| | |
| --- | --- |
| Entitäten | 54 |
| Entitätstypen | **25** |
| Typen mit genau einem Mitglied | 15 |
| Relationen | 27 |
| Captures, die überhaupt eine Relation erzeugt haben | 13 von 38 |
| Entitäten ohne jede Kante | **16** |

Das Typ-Vokabular wird pro Capture frisch erfunden, und das sieht man:
`Kunde`, `Unternehmen` und `Organisation` sind ein Typ mit drei Namen.
`Test` und `Thema`, `Getränk` und `Idee` stehen jeweils an derselben
Entität — „Kardamom-Espresso" existiert zweimal, einmal als Getränk und
einmal als Idee.

Dieselbe Sache, zwei Einträge, keine Kante dazwischen. Genau das, was der
Nutzer beschrieben hat.

Der Graph ist heute das, was ADR 0006 vorhergesagt hat: **eine Menge
unverbundener Sterne.** Relationen entstehen nur innerhalb einer einzelnen
Extraktion (`structuring.rs`), also nur zwischen Dingen, die in *einem* Satz
vorkamen.

---

## Der falsche und der richtige Zuschnitt

Der naheliegende Entwurf ist ein nächtlicher Lauf über die letzten 24
Stunden, dann wöchentlich über sieben Tage, dann monatlich über dreißig —
eine Staffelung, damit „das große Ganze nicht aus dem Sinn" gerät, ohne
jedes Mal riesige Prompts zu schicken.

Das löst das Problem nicht, es verschiebt es nur: jede Stufe liest dieselben
Captures noch einmal, und der Prompt wächst mit der Historie.

**Der richtige Zuschnitt ist kein Zeitfenster, sondern der betroffene
Teilgraph.** Wenn die Notizen einer Nacht „Sauna" berühren, lädt der Lauf
alles zu Sauna — egal ob von letzter Woche oder letztem Jahr — und
konsolidiert diese Nachbarschaft. Die Kosten hängen dann an der Zahl der
*berührten Entitäten*, nicht an der Länge der Historie. Ein Zeitfenster ist
dafür nur ein schlechter Stellvertreter.

### Zwei Auslöser, beide beschränkt

1. **Schmutzmenge.** Alle Entitäten, die seit dem letzten Lauf berührt
   wurden. Deckt ab, was gerade passiert ist.
2. **Abgestandenheit.** Zusätzlich die *N* am längsten nicht konsolidierten
   Entitäten, reihum über `last_consolidated_at`. Deckt ab, was sonst für
   immer liegen bliebe, und ersetzt die Wochen- und Monatsstufe vollständig.

Beide haben ein festes Budget pro Nacht. Der Lauf wird dadurch nie teurer,
egal wie groß der Bestand wird.

### Inkrementalität kommt aus der Zusammenfassung

Damit auch eine Entität mit 200 Captures in einen Prompt passt, braucht
jede Entität eine **rollierende Zusammenfassung**. Höhere Läufe lesen die
Zusammenfassungen der Nachbarn, nicht deren Rohnotizen.

Das Feld dafür existiert bereits: `entities.current_summary`. Heute wird es
mit der jeweils letzten Beobachtung überschrieben (Last-write-wins), ist
also keine Zusammenfassung, sondern die neueste Notiz — deshalb ist es in
der Oberfläche seit #21 als „most recently observed" beschriftet. Der
Konsolidierungslauf ist genau das, was daraus eine echte Zusammenfassung
macht, und dann ändert sich auch die Beschriftung.

**Das ist die Reihenfolge:** erst die rollierende Zusammenfassung, dann der
Nachbarschaftslauf. Ohne sie skaliert der Lauf nicht.

---

## Was der Lauf verändern darf

Entscheidung des Nutzers, wörtlich:

> „Ich bin hier für Option 2, dass alles automatisch geht, der Agent
> verändert ja dadurch nicht meine Notizen, er verändert ja nur den Blick
> darauf beziehungsweise den Kontext, den er herstellt. Es sollte aber immer
> der Zustand davor wiederherstellbar sein … man soll schon auch irgendwie
> erkennen können, wie sich mein Graph im Laufe der Zeit entwickelt hat,
> obwohl die Quellen vielleicht gleich geblieben sind."

Also: **automatisch, aber vollständig rückverfolgbar.** Der Lauf darf
Relationen anlegen, Entitäten verschmelzen und das Typ-Vokabular
vereinheitlichen, ohne zu fragen.

### Die Versionierung ist praktisch geschenkt

Der Lauf schreibt selbst Events, nichts anderes:

| Event | Bedeutung |
| --- | --- |
| `relation.proposed` | neue Kante (existiert schon) |
| `entity.merged` | zwei Entitäten sind dieselbe; Ziel und Quelle im Payload |
| `entity.unmerged` | eine Verschmelzung zurückgenommen |
| `entity.retyped` | Typ geändert, alter Typ im Payload |
| `entity.summarised` | neue rollierende Zusammenfassung |
| `consolidation.ran` | Lauf mit Umfang, Modell, Kosten |

Der Graph ist die Projektion dieser Events. „Graph zum Stand X" heißt dann
schlicht: bis Datum X abspielen. Quellen unverändert, Sicht entwickelt sich
— genau die geforderte Eigenschaft, ohne eigenen Versionierungsapparat.

Entitäten werden dabei nie gelöscht, sondern auf ein Ziel gezeigt
(`merged_into`). Eine Verschmelzung zurückzunehmen ist dann ein Event, kein
Wiederherstellungsvorgang.

---

## Drei Dinge, die der Entwurf noch nicht löst

1. **Beziehungen zwischen nie gemeinsam berührten Entitäten.** Die
   Schmutzmenge findet sie nie. Dafür braucht es einen Kandidatengenerator
   über die Ähnlichkeit der Entitäts-Zusammenfassungen — nicht nur über
   Ko-Vorkommen. Das ist derselbe Mechanismus wie die Entitäts-Kandidaten-
   suche in `docs/issues.md`.

2. **Oszillation.** Ein automatischer Lauf kann verschmelzen, der Nutzer
   nimmt zurück, der nächste Lauf verschmilzt wieder. Der Lauf muss
   `entity.unmerged` lesen und dasselbe Paar nicht erneut vorschlagen.

3. **Der erste Durchlauf über den Bestand** ist ein großer Lauf. Bei 57
   Entitäten ist das egal; bei 5.000 ist es eine eigene Aufgabe mit
   Fortsetzbarkeit — dieselbe wie die Re-Derivation in `docs/issues.md`.

---

## Zeitbewusstsein

Zweiter Befund des Nutzers, an einer echten Notiz:

> „Wenn ich beispielsweise jetzt sage, morgen habe ich einen Termin, da
> müsstet ihr eigentlich aus dem heutigen Tag erkennen, an welchem Datum der
> morgige Termin ist."

Nachweisbar in der Datenbank: die Notiz „Ich treffe Lea am Dienstag im
Central" erzeugt die Beobachtung „Ich treffe sie am Dienstag …" — welcher
Dienstag, steht nirgends. In drei Wochen ist das unlesbar, und genau dann
liest man es.

Der Prompt an OpenRouter enthält heute nur das Transkript. Kein Datum, keine
Uhrzeit, keinen Wochentag, keine Zeitzone.

### Entschieden

**Zeitkontext rein, aufgelöstes Datum raus.**

- In den Prompt: Aufnahmezeitpunkt, Wochentag und Zeitzone der Aufnahme.
- Aus dem Modell zurück: pro Aussage ein optionales, normalisiertes
  `when`-Feld (ISO-Datum, bei Bedarf mit Uhrzeit) plus die Genauigkeit
  (Tag / Woche / Monat). „Morgen" wird zu einem echten Datum, „nächsten
  Sommer" bleibt grob und sagt das auch.
- Gespeichert an der Beobachtung, angezeigt an der Entität und im
  Capture-Detail.

Ausdrücklich **nicht** Teil davon: Erinnerungen, Benachrichtigungen,
Kalenderabgleich. Das wäre ein Kalender, kein Gedächtnis, und ist ein
eigener Themenblock.

### Nebenwirkung, die den Rest verbessert

Zeitkontext im Prompt hilft nicht nur der Datumsauflösung. „Heute Abend war
ich in der Sauna" und „Also Sauna heut war schon geil" sind erkennbar
derselbe Abend, sobald das Modell weiß, wann beide gesprochen wurden — und
das ist genau die Verknüpfung, die im ersten Test gefehlt hat.

Deshalb gehört der Zeitkontext in den Prompt des Konsolidierungslaufs
ebenso wie in den der Einzelextraktion.

---

## Reihenfolge

1. **Zeitkontext in den Extraktions-Prompt** samt `when`-Feld. Klein,
   unabhängig, verbessert sofort jede neue Notiz.
2. **Rollierende Zusammenfassung pro Entität** (`entity.summarised`). Ohne
   sie skaliert der Lauf nicht.
3. **Typ-Vokabular vereinheitlichen** (`entity.retyped`). Der billigste
   sichtbare Gewinn: 25 Typen auf vielleicht 8.
4. **Nachbarschaftslauf** mit Schmutzmenge und Abgestandenheit, der
   Relationen über Capture-Grenzen hinweg vorschlägt.
5. **Verschmelzung** (`entity.merged` / `entity.unmerged`) mit
   Kandidatengenerator.

Schritt 1 **ist gebaut** (#24) und lohnte sich sofort. Die Schritte 2 bis 5
lohnen sich erst mit mehr Bestand — bei 54 Entitäten verschmilzt ein Lauf
vier Dubletten, was auch von Hand ginge. Die Mechanik bei 500 Entitäten zu
bauen ist das Ziel; sie bei 54 zu testen ist der Weg dorthin.
