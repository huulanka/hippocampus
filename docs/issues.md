# Arbeitspakete

Vorgeschlagene GitHub Issues, in Reihenfolge. `[P0]`–`[P4]` entspricht den
Phasen in `docs/roadmap.md`. Jedes Paket ist so geschnitten, dass es einzeln
umsetzbar und überprüfbar ist.

---

### [P0] Cloudflare-Access-JWT im Backend verifizieren
**Erledigt** in #15. Aktiv, sobald `CF_ACCESS_AUD` gesetzt ist; ohne
`CF_ACCESS_TEAM_DOMAIN` startet das Backend gar nicht erst.
`CF_ACCESS_AUD` ist in `.env.example` dokumentiert, wird aber nirgends
geprüft. Solange das fehlt, wäre das Backend hinter dem Tunnel offen.
Middleware in `backend/src/`, die das `Cf-Access-Jwt-Assertion`-Header gegen
die Team-JWKS prüft, mit `aud`-Abgleich. Leeres `CF_ACCESS_AUD` = lokale
Entwicklung, Prüfung übersprungen (Verhalten loggen).

### [P0] CORS einschränken
**Erledigt** in #15, über `CORS_ALLOWED_ORIGINS`. Wildcard wird abgelehnt.
`CorsLayer::permissive()` in `main.rs:69` durch eine konkrete Origin-Liste
ersetzen.

### [P0] Backup- und Restore-Durchlauf dokumentieren
**Erledigt** in #16: `docs/operations.md`, einmal vollständig
durchgespielt. Dabei fiel auf, dass `docker-compose.yml` kein Volume für
`data/audio` hatte — behoben im selben PR.
`pg_dump` der Datenbank plus Audio-Verzeichnis, einmal vollständig in eine
leere Instanz zurückspielen, Schritte in `docs/operations.md` festhalten.
Ohne bewiesenen Restore ist „permanentes Gedächtnis" eine leere Zusage.

---

### [P1] Datenmodell: Audio als Original, Transkript als Ableitung
**Erledigt** in #12.
Setzt ADR 0004 und ADR 0005 um. Migration: `capture_content` und
`transcript_content` einführen, `capture.recorded` ohne Text,
`transcript.derived` ergänzen. `CreateCaptureRequest` in `contracts`
anpassen, der Kommentar über nie hochgeladenes Audio entfällt.

### [P1] Audio-Upload und -Ablage
**Erledigt** in #12, als `POST /captures/audio` (WAV, Opus, Ogg, m4a).
`POST /captures` nimmt Audio (Opus/Ogg) entgegen, legt es inhaltsadressiert
unter einem konfigurierbaren Pfad ab, Größenlimit, Formatprüfung.

### [P1] Lokale Transkription im Tauri-Client
**Erledigt** in #13: Mikrofon über `cpal`, parakeet-tdt-0.6b-v3 int8 über
`transcribe-rs`, mehrsprachig. Audio verlässt den Mac nicht.
`transcribe-rs` einbinden (Vorbild: Handy), Modellwahl konfigurierbar,
deutsch/englisch. `client/src-tauri/src/lib.rs` ist derzeit noch das
Template mit `greet()`.

### [P1] Globaler Hotkey und Push-to-talk
**Erledigt** in #11 (Shortcut holt das Fenster), #14 (frei konfigurierbar,
persistiert) und zuletzt Tray-Icon, Autostart und Single-Instance-Sperre
(ADR 0012). Vom Tastendruck bis „nimmt auf" unter 2 Sekunden. Sicht- oder
hörbares Feedback, ohne dass ein Fenster in den Vordergrund springt.

Dabei fiel ein echter Fehler auf: die Funktion, die das Fenster
hervorholt, hat auch dem Webview signalisiert, der Aufnahme-Shortcut sei
gedrückt worden — das ist beim globalen Shortcut richtig (Druck startet
die Aufnahme, zweiter Druck beendet sie), war aber auch für den Klick auf
das Tray-Icon, den Menüpunkt und den Single-Instance-Neustart verdrahtet.
Jedes „App wieder zeigen" startete damit unbemerkt eine Aufnahme. Jetzt
sind es zwei Funktionen: `reveal_window` (nur zeigen) und
`summon_capture` (zeigen und den Shortcut-Druck melden), Letztere nur für
den globalen Shortcut selbst. Details in ADR 0012.

### [P1] Lokale Outbox mit Hintergrund-Sync
**Erledigt.** Dateien statt SQLite: pro wartendem Capture eine JSON-Datei
und, bei einer Sprachnotiz, die WAV daneben — unter
`~/Library/Application Support/com.andreasbauer.hippocampus/outbox/`.
Geschrieben wird über `.tmp` plus `rename` mit `sync_all`, also sieht ein
Leser entweder die alte Fassung oder die ganze neue, nie eine halbe.

Der Grund für Dateien ist derselbe, den das Produkt für Audio auf der
Platte nennt: die Warteschlange ist per Definition der Teil des Systems,
der an genau einer Stelle existiert. Was dort liegt, muss mit dem Finder
lesbar und kopierbar sein.

Vorher lief `stop_recording` als aufnehmen → transkribieren → hochladen,
und gab bei einem fehlgeschlagenen Upload `Err` zurück — in diesem Moment
waren Audio *und* Transkript weg. Ein schlafendes NAS, ein abgebrochenes
WLAN oder ein abgelaufener Access-Token genügten, um einen bereits
ausgesprochenen Gedanken zu verlieren. Jetzt liegt der Capture auf der
Platte, bevor das Netzwerk überhaupt angefasst wird, und das Hochladen ist
etwas, das jetzt oder später gelingt. Getippte Captures gehen denselben
Weg (`capture_text`), weil sie vorher dasselbe Loch hatten.

Der Sync-Lauf wiederholt die ganze Warteschlange gestaffelt (10 s bis
10 min) und bricht beim ersten Fehlschlag ab: die Ursache ist fast immer
eine gemeinsame, und der Rest der Schleife wäre nur derselbe Fehler *n*
mal. Ein Mutex serialisiert alle Uploads, damit der Timer und der
„ich habe gerade zu Ende gesprochen"-Pfad nicht denselben Eintrag zweimal
senden — Duplikate wären schlimmer als das Problem, das die Outbox löst.

Belegt durch `sync::tests::a_queued_capture_goes_up_and_leaves_the_queue`
(mit laufendem Backend, `--ignored`): ein angenommener Capture verlässt
die Warteschlange, und ein Capture gegen ein totes Backend bleibt samt
Begründung liegen.

### [P1] Strukturierung und Indexierung dürfen nicht lautlos scheitern
**Erledigt** zusammen mit der Outbox, weil es dieselbe Fehlerklasse ist:
Arbeit, die hinter einem Capture lief, scheiterte, sagte es nur in einer
Logzeile und wurde nie wiederholt.

`structuring.rs` gab bei einem fehlgeschlagenen OpenRouter-Aufruf genau
eine Zeile aus. Der Capture blieb sicher — aber ohne Entitäten, ohne
Beobachtung, ohne aufgelöstes Datum, und damit für immer unsichtbar auf
Entitätsseiten und in Resurface. Nichts sah kaputt aus: die Notiz stand in
der Timeline und war über ihren Wortlaut auffindbar. **In der
Entwicklungsdatenbank waren so 11 von 45 Captures nie strukturiert
worden.** Alle 11 sind beim ersten Lauf nachgeholt worden, die Zahl der
Entitäten ging dabei von 54 auf 77.

`index_capture` wurde zudem mit `?` aufgerufen, also beantwortete ein
fehlgeschlagenes Embedding einen bereits dauerhaft gespeicherten Capture
mit einem 500er — der Aufrufer erfuhr, seine Notiz sei nicht gespeichert,
obwohl sie es war. Mit einer Outbox davor wäre daraus ein Duplikat
geworden.

Beides liegt jetzt auf `capture_pipeline` (Migration 0005), mit Versuchen,
letztem Fehler und einem Aufgeben nach *n* Versuchen. Eine Schleife im
Backend holt nach, gestaffelt und mit Budget. `GET /pipeline` zählt, was
wartet und was aufgegeben wurde; `POST /pipeline/retry` und
`POST /captures/{id}/retry` fragen erneut.

Die Regel, auf die sich der Rest des Codes ab hier verlassen kann: **ist
ein Capture gespeichert, ist das Speichern gelungen.** Alles danach ist
eine Verzögerung, kein Fehlschlag.

### [P1] Eine Zeile, die sagt, was nicht fertig ist
**Erledigt.** `PendingWork` in der Sidebar, still solange es nichts zu
sagen gibt — eine Statuszeile, die dauerhaft grün leuchtet, liest an dem
Tag niemand, an dem sie rot wird. Sie zeigt „*n* waiting to sync" (lokal)
und „*n* not processed" (Backend), nennt im Tooltip den tatsächlichen
Grund und hat genau eine Handlung: nochmal versuchen. Der Grund gehört
dazu, weil ein abgelaufener Service-Token und ein schlafendes NAS ohne ihn
identisch aussehen.

### [P1] Echo-Endpunkt
**Erledigt** in #9.
`GET /captures/{id}/echo` — semantisch nächste frühere Captures über
`capture_search`, eigener Capture ausgeschlossen, Mindestähnlichkeit als
Schwelle, Standard 3 Treffer. Kein LLM beteiligt.

### [P1] Echo in der Capture-Oberfläche
**Erledigt** in #10.
Nach dem Erfassen Transkript plus Echo-Treffer wörtlich mit Datum anzeigen,
klickbar zum vollständigen Capture. Kein generierter Text.
`CaptureScreen.tsx` ist derzeit ein Mock mit Timer.

### [P1] Suche: Reciprocal Rank Fusion statt Score-Addition
**Erledigt** in #9.
`search.rs:27` mischt `(1 - cosine) * 0.7` mit `ts_rank * 0.3`. `ts_rank`
liegt typisch bei 0,01–0,1, die Cosine-Werte von e5 bei 0,7–0,9 — der
Volltextanteil ist numerisch wirkungslos und es gibt keine brauchbare
Relevanzschwelle. Durch RRF über zwei getrennte Rangfolgen ersetzen.

### [P1] Eine Oberfläche, die von selbst etwas zeigt
**Erledigt** in #26. `GET /resurface` plus der Screen „Resurface":
was du angekündigt hast und noch bevorsteht (möglich erst seit der
Zeitauflösung), und die Themen, die mehr als ein Capture berührt hat.

Bewusst nur zwei Abschnitte. Eine Oberfläche, die alles zeigt, liest
niemand. Offen bleibt „heute vor einem Jahr" — sinnvoll erst, wenn der
Bestand alt genug dafür ist.

### [P1] Echo-Qualität: Cross-Encoder statt nur Embedding
**Erledigt** in #25. Der Bi-Encoder sucht weiter die Kandidaten (Recall,
Vektoren liegen schon in der Datenbank), ein Cross-Encoder sortiert die
Top 10 neu und die Echo-Schwelle hängt an dessen Wert. Keine Migration
nötig.

Belegt am Fall, den der Nutzer gemeldet hat: mit Kosinus stand
*cardamom buns ↔ Sauna* (0,871) über *finnischer Aufguss ↔ Sauna* (0,849).
Mit dem Reranker sind es −2,15 gegen −2,03 — richtig herum, Schwelle bei
−2,0 dazwischen.

Der Abstand ist mit 0,12 schmal, und einzelne unverwandte Paare liegen
weiterhin darüber. Ehrlich ist: die *Reihenfolge* stimmt jetzt, die
Trennung ist knapp. Neu kalibrieren, wenn der Bestand deutlich wächst —
`GET /captures/{id}/echo?min_rerank=-99` gibt die Werte dafür aus.

### [P1] Zeitbewusstsein in der Extraktion
**Erledigt** in #24. Der Extraktions-Prompt bekommt Aufnahmezeitpunkt,
Wochentag und Zeitzone; das Modell löst „morgen" und „nächsten Dienstag"
gegen diesen Moment auf und gibt `when` plus `when_precision` zurück.
Gespeichert in `observations.happened_on` / `happened_at` /
`happened_precision`, angezeigt als Badge an der Beobachtung.

Die Zeitzone kommt vom Gerät (`timezone` im Capture), damit die Antwort
auch nach einem Flug stimmt; `HIPPOCAMPUS_TIMEZONE` ist nur der Rückfall.
Offen bleibt, die Zeitangaben auch abfragbar zu machen („was steht diese
Woche an") — der Index dafür liegt schon.

### [P1] Entitäts-Lesezugriff und Entitätsseiten
**Erledigt** in #21. `GET /entities` (nach Häufigkeit sortiert, nach Typ und
Namen filterbar) und `GET /entities/{id}` mit allen Beobachtungen, den
Quell-Captures und den Kanten in beide Richtungen. Der Entitäten-Screen ist
damit kein Mock mehr, und Capture-Detail, Suche und Entitätsseiten sind
gegenseitig verlinkt: Wissen wird begehbar statt abfragbar.

### [P1] Suche: Zeitfilter und toter Parameter
**Erledigt** in #9.
`from`/`to` ergänzen — „wann" ist in einem Gedächtnissystem der wichtigste
Abrufschlüssel. `entity_type` in `SearchQuery` wird akzeptiert und ignoriert:
implementieren oder entfernen.

---

### [P2] Transkript-Korrektur
**Erledigt** in #20. `POST /captures/{id}/transcript` hängt
`transcript.corrected` an, die alte Fassung bleibt unter
`[ HOW THE WORDS CHANGED ]` sichtbar, und die Korrektur löst Neu-Einbettung
und Neu-Strukturierung aus.

Ein Detail, das die Re-Derivation unten betrifft: die Korrektur wirft die
abgeleiteten Projektionszeilen weg und schreibt dafür
`structuring.invalidated`. Die `entity.observed`-Events der alten Fassung
stehen weiterhin im Log — ein vollständiger Replay muss diesen Marker
beachten, sonst erweckt er die Lesart eines Satzes wieder, den es nicht
mehr gibt.

### [P2] Redaktion mit Tombstone
**Erledigt.** `DELETE /captures/{id}` leert `capture_content`, setzt
`redacted_at` auch auf den Transkripten, entfernt abgeleitete Observations
und Relations samt verwaister Entitäten, löscht die Zeile aus
`capture_search` und die gespeicherten Echos in beide Richtungen, entfernt
die Audiodatei von der Platte und hängt `capture.redacted` an.

Der Löschvorgang der `capture_search`-Zeile ist der Punkt, an dem es
tatsächlich Redaktion wird: bliebe sie stehen, wäre der Wortlaut weiter
über die Suche auffindbar. Die Folge ist, dass ein redigiertes Capture aus
Timeline, Suche, Entitätsseiten und fremden Echos verschwindet; über seine
ID geöffnet zeigt es weiterhin den Grabstein.

Die Oberfläche fragt zweistufig zurück (`[ take this back ]` →
`[ Yes, take it back ]`) und sagt vorher, was verschwindet und dass
ältere Backups den Wortlaut weiterhin enthalten. Bewusst kein
`confirm()`: ein modaler Dialog im Tauri-Webview blockiert alle
folgenden Events.

### [P2] Re-Derivation über den gesamten Bestand
CLI/Endpunkt, das alle Captures neu strukturiert — für Modellwechsel und
nach Korrekturen. Idempotent, fortsetzbar, mit Fortschritt. Ohne das ist
`events` nur ein Audit-Log. Muss `structuring.invalidated` beachten (siehe
Transkript-Korrektur).

### [P2] JSONL-Spiegel der Rohdaten
Captures und Transkripte zusätzlich als Zeilen-JSON auf Platte, damit die
Daten ohne dieses Programm lesbar bleiben.

---

### [P3] Entitäts-Kandidatensuche
**Erledigt** mit dem Konsolidierungslauf (#46): `pg_trgm` (Migration
0006), Kandidaten über Namensähnlichkeit und Embedding-Nähe, entschieden
von einem Modell mit vier Urteilen. Siehe `docs/entity-resolution.md`.
`pg_trgm` aktivieren, Kandidaten über Namensähnlichkeit und Embedding-Nähe
suchen, über Schwellwert `entity.merge_proposed` erzeugen. Ersetzt den
exakten Vergleich in `structuring.rs:99`.

### [P3] Review-Queue
**Erledigt** als Vorschau mit [x] pro Vorschlag (#49): Konsolidierung
läuft nur auf Nachfrage, jeder Vorschlag lässt sich vor dem Anwenden
streichen, Merges und Kanten sind im Tab „Changes" rücknehmbar.
Eine Oberfläche für offene Merge-Vorschläge und unsichere Transkripte.
Zustimmen/Ablehnen als Event, jede Verschmelzung per `entity.unmerged`
umkehrbar.

### [P3] Relationen über Capture-Grenzen hinweg
**Erledigt** mit dem Konsolidierungslauf (#46, Migration 0008:
`relations.source_event_id` nullable, in der UI `[across notes]`).
`structuring.rs:65-73` verwirft Relationen, deren Endpunkte nicht in
derselben Extraktion stehen. Gegen bestehende Entitäten auflösen, statt zu
verwerfen — sonst bleibt der Graph eine Menge unverbundener Sterne.

### [P3] Von Hand zusammenführen: das Ziel nicht im Graphen suchen
**Erledigt.** „Fold into…" verlangte, das Ziel im Graphen anzuklicken.
Das ging nur, wenn es im Orbit lag — und eine Dublette liegt fast nie
dort: zwei Namen für dasselbe wurden nie zusammen gesagt, sind also keine
Nachbarn. Hinzulaufen zentrierte den Graphen neu, und das Ausgangs-Ding
verschwand aus dem Bild.

Jetzt wird das Ziel im Panel gewählt: `GET /entities/{id}/fold-candidates`
schlägt ohne Eingabe die Doppelgänger vor, mit Eingabe sucht es nach Namen
und Aliasen. Vor dem Bestätigen stehen beide Seiten mit je zwei
Beobachtungen untereinander, und der Graph zeigt das Ziel gestrichelt an
— als Linie, wenn es im Orbit liegt, sonst als Geisterknoten in der
größten Lücke des inneren Rings.

Gemessen an den echten Entitäten: e5 setzt fast jedes Paar kurzer Namen
auf ~0,9 Kosinus („Aufguss" liegt so nah an „Rasenmäher" wie an
„Finnischer Aufguss"). Deshalb sortiert der Endpunkt nach Name, dann Typ,
und das Embedding bricht nur Gleichstände.

### [P3] `current_summary` klären
`structuring.rs:151` überschreibt das Feld mit der jeweils letzten
Observation (Last-write-wins). Entweder entfernen und zur Lesezeit bilden
oder echte Konsolidierung. Aktuell suggeriert es eine Verdichtung, die nicht
stattfindet.

Seit #21 steht es prominent auf der Entitätsseite — dort ist es deshalb als
„most recently observed" beschriftet und nicht als Zusammenfassung. Sobald
der Konsolidierungslauf echte Verdichtungen schreibt, wird daraus wieder
eine Zusammenfassung und die Beschriftung ändert sich mit.

---

### [P4] iOS-Erfassung
Kurzbefehl mit Apple-Diktat gegen `POST /captures`, später ggf. eigene App
mit lokalem Whisper. Setzt Phase 0 voraus.

### [P4] Wöchentliche Rückschau
**Erledigt.** Eine Kalenderwoche Mo–So in der Zeitzone des Geräts,
blätterbar, erreichbar über Today, das Tray-Menü und die eine Mitteilung
pro Woche (Tag und Uhrzeit in Settings, Standard Freitag 16:00,
abschaltbar — siehe P11). `GET /review`, `POST /review/story`.

Alles außer dem Absatz wird bei jedem Lesen gezählt: Bestand und Verlauf
(Anzahl, gesprochen/getippt, Tage, acht Wochen), wachsende Themen (mehr
als das Doppelte des üblichen Wochenanteils der acht Wochen davor), neue,
offene Enden (angekündigt, Tag vorbei, seitdem kein Wort mehr), was die
Woche danach bringt, und still gewordene Themen (≥ 3 Captures, seit zwei
Wochen nichts, aber jünger als zwölf Wochen). Gezählt in *Captures*, nicht
Beobachtungen, und datiert nach dem Sprechzeitpunkt, nicht nach der
Extraktion — sonst landet eine spät nachgeholte Strukturierung in der
falschen Woche.

Der Absatz ist das einzige Geschriebene: 3–4 Sätze, jeder mit den Notizen,
auf denen er beruht (`n1`, `n2` … im Prompt, danach zurück in Capture-IDs).
Ein Satz ohne gültige Quelle wird verworfen. Einmal geschrieben und als
`review.written` gespeichert; eine abgeschlossene Woche bekommt ihn beim
ersten Öffnen, eine laufende nur auf Nachfrage.

Offene Enden zählen nur **Ankündigungen** — die Notiz muss vor dem Tag
gesprochen sein, um den es geht. „Gestern war der Aufguss zu heiß" ist
auch auf gestern datiert, hat sich aber im Moment des Sagens erledigt.

Ungeprüft: der Absatz gegen ein echtes Modell — die lokale Umgebung hat
keinen OpenRouter-Schlüssel. Die Zählungen sind per DB-Test belegt,
weil die lokalen Daten keine einzige Ankündigung enthalten.

---

### [P3] Ergebnis-Cutoff der Suche neu bewerten
Bei kleinem Korpus liefert der semantische Retriever jeden Capture zurück,
also erscheint hinter den echten Treffern ein Rauschschwanz („Rasenmäher"
bei Suche nach „Kardamom"). Die Rangfolge stimmt, nur die Länge nicht.
Bewusst nicht jetzt gelöst: Ein Schwellwert, der auf 13 Captures kalibriert
wird, ist Overfitting — bei 500 Captures filtert `CANDIDATE_DEPTH` von
selbst. Nach Phase 1 an echten Daten neu messen.
