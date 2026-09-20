# Was geprüft ist — und was nicht

Automatisierte Tests decken das meiste ab, aber nicht alles. Drei Dinge
lassen sich von einem Agenten grundsätzlich nicht verifizieren: ein
echtes Mikrofon, eine Berechtigung, die jemand anklickt, und ein Token,
das Cloudflare erst ausstellt, wenn der Tunnel existiert. Diese Liste
hält fest, was davon noch offen ist, damit es nicht in PR-Beschreibungen
versickert.

## Offen — braucht einen Menschen

### Die Sprachaufnahme am echten Mikrofon
Aufnehmen, sprechen, „Done" drücken. Erwartet: Transkript erscheint,
darunter das Echo. Ungeprüft ist genau das Stück zwischen Mikrofon und
Samples — alles davor und danach hat Tests, und der ASR-Pfad selbst ist
vorab isoliert gemessen worden (0,7 s Modellladen, 0,25 s für 5,7 s
deutsche Sprache).

Beim ersten Mal fragt macOS nach der Mikrofon-Berechtigung.

### Der aufgezeichnete Shortcut
In den Einstellungen `[ Change ]` drücken, eine Kombination tippen,
danach aus einer anderen App heraus auslösen. Erwartet: greift sofort,
ohne Neustart, und überlebt einen Neustart.

### Ein echtes Cloudflare-Access-Token
Die Signaturprüfung ist gegen selbst erzeugte Schlüsselpaare getestet
(gültig, fremde `aud`, fremdes Team, abgelaufen, gefälscht, `alg: none`).
Ungeprüft ist, ob ein von Cloudflare tatsächlich ausgestelltes Token
durchgeht — das geht erst, wenn der Tunnel steht.

## Geprüft — und wie

| Was | Wie |
| --- | --- |
| Echo-Schwelle | An echten Captures gemessen: Rauschgrenze 0,877, echte Treffer ab 0,905. Schwelle 0,89. |
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
  lassen sich also derzeit nicht entfernen.
- Dependabot meldet `glib` 0.18.5 (unsound `Iterator`-Implementierung).
  Kommt über GTK aus Tauris Linux-Webview-Stack, wird auf macOS nicht
  kompiliert, und Tauri 2.11 lässt sich nicht auf `glib` 0.20 heben.
  Nichts, was sich hier beheben ließe.
