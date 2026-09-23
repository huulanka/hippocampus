# Zukunft erinnern — prospektives Gedächtnis

Stand: 2026-09-23. Ergebnis der Product-Scoping-Session vom selben Tag.
Wie `docs/product.md` sagt dieses Dokument, *warum* und *was*; das *wie*
für den Kalender steht in `docs/adr/0013-the-calendar-stays-on-the-mac.md`.

## Das Problem, das hier gelöst wird

`docs/product.md` verspricht, Gedanken „in dem Moment wieder
hervorzuholen, in dem sie relevant sind — ohne dass man sich erinnern
muss, danach zu fragen". Bis hierher gab es genau drei solche Momente,
und alle drei gehören der App:

1. Du sprichst → Echo.
2. Du öffnest das Fenster → Today.
3. Freitag, 16:00 → Wochenrückschau.

Der Moment, in dem Wissen tatsächlich zählt, gehört aber der Welt: In
zehn Minuten ist der Jour fixe mit Paul, und vor drei Wochen hast du
gesagt, dass du ihn nach der Hafenportal-Deadline fragen willst. Das wusste
bisher niemand.

Dazu kommt eine zweite Lücke: Hippocampus kannte keine **Absichten**. Die
README versprach „entities, relations, and tasks", extrahiert wurden
aber nur Entitäten und Relationen. Sätze wie *„wenn ich Paul das nächste
Mal spreche, frag ich ihn nach X"* haben kein Datum, also landeten sie
auch nicht in `upcoming`. Sie hängen an einem **Menschen oder Thema**,
nicht an einer Uhrzeit. Genau dafür gibt es kein Werkzeug: Erinnerungs-
Apps kennen Zeit und Ort, Notiz-Apps kennen gar nichts.

## Was gebaut wird

Eine Absicht ist etwas, das du **später tun, sagen oder fragen** willst
und das noch nicht erledigt ist. Sie wird beiläufig erkannt, ohne
Schlüsselwort, und hängt an den Entitäten, um die es geht.

Sie meldet sich in zwei Momenten:

| Auslöser | Wo | Wie |
|---|---|---|
| **Nächste Erwähnung** — du sprichst wieder über Paul | Capture-Sheet, direkt unter dem Echo | „Du wolltest noch …", mit *Erledigt* |
| **Kalendertermin** — ein Termin, dessen Titel oder Teilnehmer zu einer Entität passen, beginnt in *n* Minuten | Menüleiste (eingefärbt, funkelt), optional ein Banner, und die Kontext-Seite zum Termin | Offene Absichten zuerst, darunter deine letzten Sätze zu den Beteiligten |

Nach dem Termin fragt Hippocampus einmal: **„Hast du's angesprochen?"**
*Ja* schließt die Absicht (als neues Event, P7), *Noch nicht* lässt sie
offen und fragt für diesen Termin nicht noch einmal.

Direkt nach dem Sprechen zeigt das Capture-Sheet **„Absicht erkannt"**
mit einem [x] zum Verwerfen. Die Erkennung ist ein Modell-Urteil, sie
wird also gelegentlich danebenliegen; das [x] ist die Absicherung, und
es ist rücknehmbar.

## Entscheidungen

| # | Entscheidung | Begründung |
|---|---|---|
| F1 | **Beiläufige Erkennung**, kein Schlüsselwort | Der Nutzer spricht über „alles, was mir durch den Schädel geistert". Ein Schlüsselwort verlangt, dass man daran denkt — genau das soll das Produkt abnehmen. |
| F2 | **Sichtbar direkt nach dem Sprechen**, mit [x] | Ein Urteil, das man nicht sieht, kann man nicht korrigieren. Die Erfahrung aus der Konsolidierung (erst automatisch, dann zurückgenommen) gilt hier genauso. |
| F3 | **Auslöser in V1: Kalender und nächste Erwähnung.** *Nicht* das aktive Fenster | Das aktive Fenster braucht die Accessibility-Berechtigung, fühlt sich nach Überwachung an und liefert die meisten Fehlalarme. |
| F4 | **Kalender pro Mac angehakt**, nicht nach Konto geraten | Privater Mac (iCloud) und Arbeits-Mac (Exchange über Apple Kalender). Ein Arbeitstermin darf nie auf dem privaten Mac auftauchen. Ohne Häkchen wird kein Kalender gelesen. |
| F5 | **Der Kalender bleibt auf dem Mac.** Titel und Teilnehmernamen eines anstehenden Termins gehen einmal an das eigene Backend, werden dort abgeglichen und **nicht gespeichert** | Die Entitäten, Aliase und Merges leben im Backend; dort ist der Abgleich richtig und testbar. Der Termin selbst wird nie Teil des Gedächtnisses. Siehe ADR 0013. |
| F6 | **Das Icon ändert sich deutlich**, nicht bloß ein Punkt: eingefärbt, mit Funkeln | Das Icon ist fein gezeichnet, ein Punkt geht im Alltag unter. Aufnahme hat Vorrang vor dem Funkeln. |
| F7 | **Banner optional** (Standard an), respektiert Fokus/Nicht stören | Das ist die **zweite Ausnahme von P11**. Sie hängt, wie die erste, an einem Termin, den der Nutzer ohnehin hat. macOS unterdrückt Mitteilungen im Fokus-Modus selbst, solange sie als Systemmitteilung kommen. |
| F8 | **Das Banner verrät keinen Notizinhalt** — nur den Termintitel, der aus dem eigenen Kalender stammt | Lesen ist hinter Touch ID, ein Banner nicht. Die Menüleiste weiß, *dass* etwas ansteht, nicht *was*. |
| F9 | **Erledigt per Nachfrage nach dem Termin**, nicht per Modell | Ein Modell, das Absichten selbst schließt, schließt auch falsche — und das merkt man nicht. |
| F10 | **Ohne Absicht funkelt nichts** | Ein Termin mit bekannten Menschen, aber ohne offene Absicht, bekommt die Kontext-Seite (Today zeigt ihn an), aber kein Funkeln und kein Banner. Sonst funkelt es vor jedem Termin, und das Funkeln wird zur Tapete. |
| F11 | **Der Text der Absicht liegt im Inhalt, nicht im Event-Payload** | Wie der Sitzungstitel (`CaptureSession`): das Log wird nie geändert, also kann nur redigiert werden, was nicht darin steht. `intention.noted` trägt Ids, der Wortlaut liegt in `intentions`. |
| F12 | **Zu jeder Absicht das wörtliche Zitat**, wo es sich finden lässt | Die kurze Formulierung ist die des Modells; das Zitat ist deins. Es wird nur übernommen, wenn es wirklich im Transkript steht — ein erfundenes Zitat ist schlimmer als keins. |

## Datenmodell

```
events
  intention.noted      { intention_id, source_event_id, entity_ids }   (Modell)
  intention.dismissed  { intention_id }                                (Nutzer, [x])
  intention.fulfilled  { intention_id, via: "calendar" | "manual" }    (Nutzer)
  intention.reopened   { intention_id }                                (Nutzer, Rücknahme)

intentions              — Projektion + Inhalt
  id, source_event_id, text, quote, status (open|fulfilled|dismissed),
  model, created_at, resolved_at

intention_entities      — woran sie hängt
  intention_id, entity_id
```

Merges werden beim Lesen aufgelöst (`coalesce(merged_into, id)`), nicht
beim Merge umgeschrieben. Damit bleibt ein Unmerge von selbst korrekt.

Redigieren eines Captures löscht seine Absichten mit, wie seine
Beobachtungen und Relationen.

## Oberfläche

- **Capture-Sheet:** unter dem Echo „Absicht erkannt" (mit [x]) und
  „Du wolltest noch …" (mit *Erledigt*).
- **Today:** oben der nächste passende Termin („In 8 Minuten: Jour fixe
  Paul"), darunter **„Noch vorgenommen"**, alle offenen Absichten.
- **Kontext-Seite zum Termin:** der Termin, seine offenen Absichten groß,
  deine letzten Sätze zu jeder beteiligten Entität. Nach dem Termin:
  *Ja / Noch nicht*.
- **Entitätsseite:** die offenen Absichten, die an ihr hängen.
- **Settings → Foresight:** Kalenderzugriff, Kalender mit Konto und Farbe
  zum Anhaken, Vorlauf (5 / 10 / 15 / 30 Minuten), Banner an/aus.
- **Menüleiste:** Ruhe → Vorlage (template). Aufnahme → atmender Punkt.
  Etwas steht an → das Gehirn in Clay, Funkeln in Ember.
- **Das Menü unter dem Icon** (nach dem ersten echten Test gewünscht,
  23.09.2026): **beide Klicks öffnen das native Menü**, und das zeigt
  zuerst die Termine — der leuchtende oben, darunter gesperrt nur „Something
  to bring up", entsperrt das Zitat und wen es betrifft; ohne Termin „Still
  on your mind". Ein Termin öffnet seinen Brief, „Unlock to See…" fragt
  Touch ID und entsperrt auch das Fenster, **„Write a Note…"** öffnet das
  Capture-Sheet zum Tippen (ohne Mikrofon). Ein eigenes Panel-Fenster war
  gebaut und flog wieder raus: AppKit gibt den Linksklick an diesem
  Statusitem direkt ans Menü, er erreicht die App nie. Ein Eingabefeld
  *im* Menü bräuchte Zugriff aufs `NSMenu`, den Tauri nicht hergibt —
  vorgemerkt. Die Regel aus ADR 0012 gilt weiter: kein Klick startet je
  eine Aufnahme.
- **Nicht in die Terminbeschreibung schreiben** (vom Nutzer gefragt,
  entschieden: nein). Bei Exchange verschickt eine Änderung als
  Organisator ein Update an alle Teilnehmer, die Notizen lägen danach auf
  dem Firmenserver, und der Kalender bleibt nur gelesen (ADR 0013).

## Was bewusst nicht drin ist

- Termine anlegen oder ändern. Hippocampus liest den Kalender nur.
- Absichten per Sprache abhaken („hab ich erledigt"). Siehe F9.
- Ort, aktives Fenster, Mails.
- Wiederholungen („jedes Mal, wenn …"). Eine Absicht wird einmal erfüllt.

## Wie man es prüft

- Backend: Extraktion mit Absichten (Parser-Tests), Zitat-Prüfung,
  Abgleich Termin ↔ Entität (reine Funktion, Tests mit echten Namen aus
  der Dev-DB), Routen gegen Postgres (`db-tests`).
- Client: Phasen eines Termins (vorher / läuft / danach / abgelaufen) als
  reine Funktion, Icon-Frames als Tests wie beim Punkt.
- Echt: Kalender-Berechtigung und Banner nur im gebündelten Build —
  wie Mikrofon und Touch ID (`docs/verification.md`).
