# Was geprüft ist — und was nicht

Automatisierte Tests decken das meiste ab, aber nicht alles. Manches
lässt sich von einem Agenten grundsätzlich nicht verifizieren: ein Token,
das Cloudflare erst ausstellt, wenn der Tunnel existiert, ein Shortcut,
der aus einer fremden App heraus greifen muss, und alles, was sich erst
im fertigen Fenster zeigt. Diese Liste hält fest, was davon offen ist,
damit es nicht in PR-Beschreibungen versickert.

Das Mikrofon stand lange hier und steht jetzt in der Tabelle darunter —
der Nutzer hat es am 21.09.2026 selbst geprüft.

## Offen — braucht einen Menschen

### Der aufgezeichnete Shortcut
In den Einstellungen `[ Change ]` drücken, eine Kombination tippen,
danach aus einer anderen App heraus auslösen. Erwartet: greift sofort,
ohne Neustart, und überlebt einen Neustart.

### Die Verbindung zur NAS aus der fertigen App
Backend-URL und Service Token in den Einstellungen eintragen, speichern,
danach erfassen, suchen und eine Aufnahme abspielen. Erwartet: „Saved —
checked reachable just now.", und alle Screens sprechen mit der NAS statt
mit localhost. Gegen echte Sockets getestet ist, wie der Client auf 200,
302 und 403 reagiert und dass die Header tatsächlich auf der Leitung
liegen (`backend.rs`); ungeprüft ist der Weg durch den echten Tunnel.

### Der entfernte Echo-Judge gegen die echte API
Der Aufruf an OpenRouter ist in der Form identisch zu dem, den die
Strukturierung seit Monaten macht, und das Auswerten der Antwort ist mit
sechs Fällen unit-getestet (fehlendes Urteil, erfundene ID, Werte außerhalb
0-1, Prosa statt JSON, leere Liste, Reihenfolge). **Ungeprüft ist ein
echter Aufruf**: lokal liegt kein OpenRouter-Schlüssel, der Schlüssel lebt
auf der NAS. Das entscheidet sich beim nächsten Redeploy — und ist von
außen messbar, weil `GET /captures/{id}/echo` dann `judged_by` mit dem
Modellnamen füllt statt mit `similarity`.

### Die Schwelle des entfernten Judges
0,5 ist gesetzt, weil das Modell ausdrücklich nach einer 0-1-Relevanz
gefragt wird und die Hälfte der sinnvolle Mittelpunkt ist — **nicht, weil
sie gemessen wurde**. Sie braucht dieselbe Kalibrierung, die die
Cross-Encoder-Schwellen bekommen haben, sobald genug echte Captures da
sind. `?min_rerank=-99` gibt die Werte dafür aus und kostet nichts mehr,
weil jeder Kandidat mit seiner Bewertung gespeichert ist.

### Ein echtes Cloudflare-Access-Token
Die Signaturprüfung ist gegen selbst erzeugte Schlüsselpaare getestet
(gültig, fremde `aud`, fremdes Team, abgelaufen, gefälscht, `alg: none`).
Ungeprüft ist, ob ein von Cloudflare tatsächlich ausgestelltes Token
durchgeht — das geht erst, wenn der Tunnel steht.

### Die Korrektur am eigenen Text
`[ fix a word ]` in der Detailansicht, Text ändern, speichern. Erwartet:
die neue Fassung steht überall (Timeline, Suche, Entitäten), die alte
darunter unter `[ HOW THE WORDS CHANGED ]`. Gegen die Entwicklungs-
datenbank durchgespielt, aber nicht von einem Menschen in der App.

### Die Audiowiedergabe in der echten App
Der Player ist im Browser gegen den laufenden Dienst geprüft, nicht im
Tauri-Webview — und er holt die Aufnahme seit ADR 0009 nicht mehr über
`<audio src>`, sondern als Blob über Rust, weil ein `src` die
Access-Header nicht tragen kann. Beides gehört in der fertigen App
einmal angehört.

### Der Wechsel auf candle, tatsächlich auf der Zielhardware
`ort`s vorgebaute ONNX-Runtime-Binary hat auf der Synology DS220+ (Celeron
J4025, kein AVX2) mit SIGILL abgestürzt, noch bevor die erste Log-Zeile
geschrieben wurde. Der Umstieg auf `candle` (ADR 0008) ist lokal gebaut,
getestet und gegen echte Captures verifiziert — auch per
`docker build --platform linux/amd64`, das den Container-Build bestätigt.
Was das lokale Docker-Build auf einem Apple-Silicon-Mac **nicht** prüfen
kann: ob SIGILL auf der echten NAS-Hardware tatsächlich weg ist, weil die
QEMU-Emulation dort nicht dieselben (fehlenden) CPU-Features nachbildet.
Das entscheidet sich erst beim echten Redeploy auf der NAS.

## Geprüft — und wie

| Was | Wie |
| --- | --- |
| Die Sprachaufnahme am echten Mikrofon | Vom Nutzer am 21.09.2026 bestätigt: Dialog kam, Sprache wurde transkribiert. |
| Echo-Schwelle (Kosinus) | An echten Captures gemessen: Rauschgrenze 0,877, echte Treffer ab 0,905. Schwelle 0,89 — gilt nur noch ohne Reranker. |
| Echo-Schwelle (Cross-Encoder, Jina — abgelöst 21.09.2026) | Über alle 38 Captures kalibriert. *Sauna ↔ Sauna* +0,02, *Sauna ↔ Aufguss* −2,03, *cardamom buns ↔ Sauna* −2,15. Schwelle −2,0 dazwischen; der Abstand ist mit 0,12 schmal. Gilt für ein Modell, das nicht mehr läuft (siehe ADR 0008). |
| Echo-Schwelle (Cross-Encoder, BGE über candle) | Gegen zwei echte Captures über den laufenden Dienst gemessen: ein echter Treffer +0,193, fünf unpassende Kandidaten −8,37 bis −10,33. Schwelle −4,0 mittig in der Lücke — ein echter Treffer bisher, keine Korpus-Kalibrierung wie bei Jina. |
| Reranker-Latenz (Jina — abgelöst 21.09.2026) | Zehn Kandidaten: Jina 178 ms, BGE (via `ort`) 605 ms (`examples/rerank_latency.rs`). |
| Reranker-Latenz (BGE über candle) | Zehn Kandidaten auf diesem Mac: kalt 1,6s, warm 1,3s (`examples/rerank_latency.rs`, nach dem Umstieg auf candle — siehe ADR 0008). Langsamer als die `ort`-Version auf derselben Maschine; noch nicht auf der NAS gemessen. |
| Zeitauflösung | Notiz um 00:24 Berlin: „morgen Abend um halb acht" → 2026‑09‑22 17:30 UTC, „nächsten Dienstag" → 2026‑09‑29. Beide richtig, inklusive der Feinheit, dass morgen schon Dienstag ist. |
| Korrektur-Kette | Typ-Capture zweimal korrigiert: Versionen `you, typed` → `you, corrected` → `you, corrected`, Entitäten neu abgeleitet, `structuring.invalidated` geschrieben. |
| Entitätsseiten | „Lena" führt drei Captures von verschiedenen Tagen und fünf Kanten auf einer Seite zusammen. |
| Hybride Suche (RRF) | „HPortal" steht bei zwei Retrievern vorn (0,0328) vor Einzeltreffern (0,0161). |
| Audio-Upload und -Ablage | Inhaltsadressiert, Dedup, Größenlimit, Formatprüfung — Unit-Tests plus curl gegen den laufenden Dienst. |
| Fehlerantworten | 400 bei leerem Capture, falschem MIME-Typ, fehlendem Transkript; 404 bei unbekannter Capture. |
| Zugriffsschutz | 401 ohne und mit Müll-Token, `/health` offen, CORS nur für die Client-Origins, Start bricht bei halber Konfiguration ab. |
| Backup und Restore | Einmal vollständig zurückgespielt, siehe `operations.md`. |
| Resampling, Mono-Mischung, WAV | Unit-Tests im Client. |
| Einstellungen | Default parst, Accelerator round-trippt, Unsinn wird abgelehnt, kaputte Datei fällt auf den Default zurück. |
| Secret in der Keychain | Schreiben, Lesen, Löschen und nochmals Löschen gegen die echte macOS-Keychain durchgespielt (`cargo test --lib -- --ignored`, 21.09.2026). Ein Unit-Test hält zusätzlich fest, dass `persist` das Secret nie in `settings.json` schreibt. |
| Access-Fehler sind unterscheidbar | Gegen echte Sockets: 200 wird akzeptiert, ein 302 auf die Cloudflare-Login-Seite wird als abgelehnter Service Token gemeldet statt als gesunder Backend, ein 403 nennt die Access-Policy, eine URL ohne Schema sagt das. |
| Echo-Persistenz, Ende zu Ende | Gegen die lokale Datenbank durchgespielt (21.09.2026): Capture speichern 81 ms mit `echo_pending: true`, Detailansicht 22 ms, Echo erneut lesen 2,5 ms, `?min_rerank=-99` liefert alle gespeicherten Kandidaten ohne Neuberechnung. Datenbank enthält Marker mit Provenienz plus die gerankten Zeilen. |
| Nachziehen und Neubewerten | Ein Capture ohne gespeichertes Urteil antwortet beim ersten Lesen mit `pending: true` und beim zweiten mit dem Ergebnis. Eine Korrektur löscht das Urteil und löst ein neues aus — nachgeprüft über `judged_at`. |
| Reranker-Latenz auf der NAS (BGE über candle) | Gegen das laufende Backend gemessen, 21.09.2026: 17,7 s bei zwei Kandidaten, 27,9 s bei drei, 36,6 s bei vier — linear, **9,4 s pro Kandidat**. Zum Vergleich auf demselben Weg: `/search` 247 ms, alle reinen Lese-Endpunkte 120-135 ms, `/health` 160 ms. Der Engpass ist ausschließlich der Cross-Encoder, und weil die Zeit linear mit der Kandidatenzahl wächst (Gewichte werden pro Batch einmal gelesen), ist es Rechenleistung und nicht Speicher. |
| Redaktion, Ende zu Ende | Gegen den laufenden Dienst durchgespielt (21.09.2026): `DELETE` meldet, was entfernt wurde; das Capture ist danach aus Timeline und Suche verschwunden; über seine ID geöffnet zeigt es `redacted: true`, keinen Text und keine Transkripte; ein zweites `DELETE` antwortet mit 200 statt mit einem Fehler. |
| Echo-Judge, echter Aufruf | Gegen OpenRouter gemessen (21.09.2026, lokal): `mistralai/mistral-small-2603` über den Anbieter Mistral, **zehn Kandidaten in 1,0 s**, 547 Prompt- und 147 Completion-Token. Auf der NAS dauerte derselbe Aufruf bei *fünf* Kandidaten 12,3 s — dasselbe Modell, derselbe Anbieter, also liegt der Unterschied am Weg der NAS nach draußen und nicht am Modell. Ab jetzt steht das in den Logs. |
| Was das Backend über sich selbst sagt | Eine Zeile je Anfrage mit Dauer, eine je Modellaufruf mit Anbieter, Dauer und Token, eine je Echo-Urteil mit Dauer, bestem und zweitbestem Wert. Gegen den laufenden Dienst geprüft, Beispiele in `operations.md`. |
| Service-Token-Header auf der Leitung | Gegen einen echten Socket geprüft: mit Credentials stehen `cf-access-client-id` und `cf-access-client-secret` im Request, ohne Credentials steht keiner der beiden drin (statt leerer Header). |

## Bekannter Zustand

- In der Entwicklungsdatenbank liegen ein paar Test-Captures von mir
  („Hotkey-Test…", „Rasenmäher…"). Redaction ist noch nicht gebaut, sie
  lassen sich also derzeit nicht entfernen. Spätere Test-Captures sind
  aus den Projektionen entfernt (Timeline, Suche, Entitäten sehen sie
  nicht mehr); ihre Events stehen aus demselben Grund weiterhin im Log.
- Die Beobachtungen aus der Zeit vor #24 haben kein aufgelöstes Datum.
  Sie bekommen eins erst, wenn die Re-Derivation gebaut ist oder das
  jeweilige Capture korrigiert wird — der Abschnitt „you said this was
  coming" füllt sich also erst mit neuen Notizen.
- Der Bi-Encoder sortiert weiterhin nachweislich falsch, und das ist jetzt
  dokumentiert statt erinnert: eine Notiz „Ich war heute Abend in der
  Sauna, der finnische Aufguss war richtig gut" bekam am 21.09.2026 gegen
  den lokalen Bestand **Kardamom-Espresso mit 0,9192 vor dem finnischen
  Aufguss mit 0,9064** und der Sauna-Notiz mit 0,9053. Genau dafür gibt es
  die zweite Stufe.
- Der Client spricht seit ADR 0009 nicht mehr aus dem Webview mit dem
  Backend, sondern aus Rust. Die CORS-Konfiguration des Backends betrifft
  damit nur noch den Browser-Entwicklungsbuild.
- Dependabot meldet `glib` 0.18.5 (unsound `Iterator`-Implementierung).
  Kommt über GTK aus Tauris Linux-Webview-Stack, wird auf macOS nicht
  kompiliert, und Tauri 2.11 lässt sich nicht auf `glib` 0.20 heben.
  Nichts, was sich hier beheben ließe.
