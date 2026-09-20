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
Tauri-Webview.

## Geprüft — und wie

| Was | Wie |
| --- | --- |
| Die Sprachaufnahme am echten Mikrofon | Vom Nutzer am 21.09.2026 bestätigt: Dialog kam, Sprache wurde transkribiert. |
| Echo-Schwelle (Kosinus) | An echten Captures gemessen: Rauschgrenze 0,877, echte Treffer ab 0,905. Schwelle 0,89 — gilt nur noch ohne Reranker. |
| Echo-Schwelle (Cross-Encoder) | Über alle 38 Captures kalibriert. *Sauna ↔ Sauna* +0,02, *Sauna ↔ Aufguss* −2,03, *cardamom buns ↔ Sauna* −2,15. Schwelle −2,0 dazwischen; der Abstand ist mit 0,12 schmal. |
| Reranker-Latenz | Zehn Kandidaten: Jina 178 ms, BGE 605 ms (`examples/rerank_latency.rs`). |
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
- Dependabot meldet `glib` 0.18.5 (unsound `Iterator`-Implementierung).
  Kommt über GTK aus Tauris Linux-Webview-Stack, wird auf macOS nicht
  kompiliert, und Tauri 2.11 lässt sich nicht auf `glib` 0.20 heben.
  Nichts, was sich hier beheben ließe.
