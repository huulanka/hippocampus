# ADR 0006: Echo vor Graph

## Status
Accepted (2026-09-20)

## Context
Erklärtes Produktziel ist Verknüpfung, nicht bloßes Wiederfinden. Dafür gibt
es zwei sehr unterschiedlich teure Mechanismen.

**Echo** — nach dem Erfassen semantisch nahe frühere Captures zeigen. Beruht
allein auf Embedding-Ähnlichkeit. Der Kern davon läuft bereits
(`embedding.rs`, `pgvector`, `capture_search`). Funktioniert ab etwa 20
Captures und kann nichts erfinden, weil es ausschließlich eigene Worte zeigt.

**Graph** — typisierte Entitäten und Beziehungen, navigierbar. Hängt
vollständig an verlässlicher Entitäts-Zusammenführung. Heute passiert die
über exakten String-Vergleich (`structuring.rs:99`), und Relationen entstehen
nur innerhalb eines einzelnen Transkripts (`structuring.rs:65-73`) — der
Graph ist derzeit eine Menge unverbundener Sterne.

## Decision
Echo wird zuerst gebaut und muss allein tragen. Der Graph bleibt als Ziel
bestehen, wird aber erst nach Capture-Volumen und funktionierender
Review-Queue weiterentwickelt und nach etwa 1.000 Captures neu bewertet.

Konkret: kein weiterer Ausbau von Entitäts-/Relations-UI, bevor der
Capture-Pfad und Echo im täglichen Gebrauch stehen.

## Consequences
- Der schnellste Weg zu erlebbarem Produktwert; Echo ist in Tagen, nicht
  Monaten erreichbar.
- Die vorhandene Strukturierung läuft unverändert im Hintergrund weiter und
  sammelt Daten, an denen sich die Qualität der Extraktion später an echtem
  Material beurteilen lässt.
- Risiko: Echo könnte sich als ausreichend erweisen und der Graph nie gebaut
  werden. Das wäre kein Scheitern, sondern ein Ergebnis — festgestellt an
  echten Daten statt an einer Vorab-Annahme.
