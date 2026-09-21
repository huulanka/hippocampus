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
**Teilweise erledigt** in #11 (Shortcut holt das Fenster) und #14 (frei
konfigurierbar, persistiert). Offen bleiben Tray-Icon und Autostart.
Tauri-Plugin für globale Shortcuts, Tray-Icon, Autostart. Vom Tastendruck
bis „nimmt auf" unter 2 Sekunden. Sicht- oder hörbares Feedback, ohne dass
ein Fenster in den Vordergrund springt.

### [P1] Lokale Outbox mit Hintergrund-Sync
Capture wird sofort lokal persistiert (SQLite oder Dateien), Upload mit
Retry und Backoff. Sichtbarer Zustand „n Captures warten auf Sync". Die
Erfassung darf niemals wegen Netzwerk fehlschlagen.

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
`pg_trgm` aktivieren, Kandidaten über Namensähnlichkeit und Embedding-Nähe
suchen, über Schwellwert `entity.merge_proposed` erzeugen. Ersetzt den
exakten Vergleich in `structuring.rs:99`.

### [P3] Review-Queue
Eine Oberfläche für offene Merge-Vorschläge und unsichere Transkripte.
Zustimmen/Ablehnen als Event, jede Verschmelzung per `entity.unmerged`
umkehrbar.

### [P3] Relationen über Capture-Grenzen hinweg
`structuring.rs:65-73` verwirft Relationen, deren Endpunkte nicht in
derselben Extraktion stehen. Gegen bestehende Entitäten auflösen, statt zu
verwerfen — sonst bleibt der Graph eine Menge unverbundener Sterne.

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
An den Review-Termin gekoppelt: was diese Woche erfasst wurde, welche
Themen wiederkehren. Kein täglicher Brief.

---

### [P3] Ergebnis-Cutoff der Suche neu bewerten
Bei kleinem Korpus liefert der semantische Retriever jeden Capture zurück,
also erscheint hinter den echten Treffern ein Rauschschwanz („Rasenmäher"
bei Suche nach „Kardamom"). Die Rangfolge stimmt, nur die Länge nicht.
Bewusst nicht jetzt gelöst: Ein Schwellwert, der auf 13 Captures kalibriert
wird, ist Overfitting — bei 500 Captures filtert `CANDIDATE_DEPTH` von
selbst. Nach Phase 1 an echten Daten neu messen.
