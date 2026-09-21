# Betrieb

## Backup und Restore

Ohne bewiesenen Restore ist „permanentes Gedächtnis" eine leere Zusage.
Das Verfahren unten ist einmal vollständig durchgespielt worden; was
dabei herauskam, steht weiter unten.

Zu sichern sind **zwei** Dinge, und sie gehören zusammen:

| Was | Wo | Warum |
| --- | --- | --- |
| Postgres | `data/postgres` (Volume) | Events, Projektionen, Embeddings |
| Audio | `data/audio` | Das Original (ADR 0004). Steht nicht in der Datenbank. |

Das Embedding-Modell unter `data/models` ist **kein** Backup wert — es
ist ein Download, kein Zustand.

### Reihenfolge

**Erst Audio kopieren, dann die Datenbank sichern.** Beides ist
append-only, aber die Datenbank verweist auf Audiodateien, nicht
umgekehrt. In dieser Reihenfolge kann höchstens eine Audiodatei ohne
zugehörige Zeile im Archiv landen, und die ist harmlos. Andersherum
entstünde eine Capture, deren Original fehlt — genau der Verlust, den
dieses System ausschließen soll.

### Sichern

```sh
# 1. Audio zuerst. Inhaltsadressiert, also ist ein erneutes Kopieren
#    derselben Datei immer identisch.
tar -cf audio-$(date +%F).tar -C data audio

# 2. Danach die Datenbank, im custom format (komprimiert, selektiv
#    wiederherstellbar).
docker compose exec -T postgres \
  pg_dump -U hippocampus -Fc -d hippocampus > db-$(date +%F).dump
```

Beide Dateien gehören auf Speicher, der die Maschine überlebt.

### Zurückspielen

```sh
# In eine leere Instanz:
docker compose up -d postgres
docker compose exec -T postgres \
  psql -U hippocampus -d postgres -c "create database hippocampus"
docker compose exec -T postgres \
  pg_restore -U hippocampus -d hippocampus --no-owner < db-2026-09-20.dump

tar -xf audio-2026-09-20.tar -C data
```

`--no-owner` ist nötig, wenn die Zielinstanz andere Rollen hat als die
Quelle. Migrationen müssen danach **nicht** laufen: der Dump enthält das
Schema und `sqlx migrate` erkennt den Stand an `_sqlx_migrations`.

### Was der Durchlauf gezeigt hat

Gegen einen echten Bestand (61 Events, 19 Captures, 1 Audiodatei) in
eine frische Datenbank zurückgespielt:

- Zeilenzahlen aller Tabellen identisch — `events`, `capture_content`,
  `transcript_content`, `capture_search`, `entities`, `observations`,
  `relations`
- `vector`-Extension (0.8.6) kommt mit, Embeddings behalten ihre 384
  Dimensionen, eine Ähnlichkeitsabfrage liefert dieselben Werte
- HNSW- und GIN-Indizes werden neu aufgebaut
  (`capture_search_embedding_idx`, `capture_search_tsv_idx`,
  `entities_embedding_idx`)
- die generierte `tsv`-Spalte behält ihre deutsche Konfiguration;
  `plainto_tsquery('german', …)` findet dieselben Treffer
- **die Append-only-Regeln überleben**: `update events …` meldet
  `UPDATE 0`, `delete from events` meldet `DELETE 0`, alle 61 Zeilen
  stehen danach noch da
- Audio per `tar` hin und zurück: Prüfsumme über alle Dateien identisch
- jede `capture_content.audio_path` zeigt nach dem Restore auf eine
  Datei, die es gibt

Der einzige Unterschied zur Quelle: eine `capture_search`-Zeile hat kein
Embedding — vor dem Backup schon so, nicht durch den Restore entstanden.

### Wann das wieder zu prüfen ist

Nach jeder Migration, die eine Extension, eine generierte Spalte oder
eine Regel anfasst. Genau diese drei Dinge überstehen einen `pg_dump`
nicht automatisch nur deshalb, weil Zeilen es tun.

## Konfiguration im Container

`docker-compose.yml` reicht die Umgebungsvariablen durch, die das
Backend liest. Zwei davon sind nicht optional, sobald der Dienst
erreichbar ist:

- `CF_ACCESS_AUD` **und** `CF_ACCESS_TEAM_DOMAIN` — ohne beide werden
  alle Anfragen vertraut. Das Backend sagt das beim Start laut, und es
  startet gar nicht erst, wenn nur eine von beiden gesetzt ist.
- `CORS_ALLOWED_ORIGINS` — Standard sind die Origins des Desktop-Clients.
  Ein Wildcard wird abgelehnt.

`AUDIO_DIR` zeigt im Container auf ein gemountetes Volume. Ohne dieses
Volume landete das Archiv der Originale im Container-Dateisystem und
wäre beim nächsten `docker compose up --force-recreate` weg.

## Deployment auf der NAS

`backend/Dockerfile` baut das Image; `docker-compose.yml`s Build-Context
ist bewusst das Repository-Root und nicht `backend/`, weil `backend` ein
Workspace-Mitglied ist und die Root-`Cargo.toml`/`Cargo.lock` sowie die
`contracts`-Crate daneben zum Bauen braucht (siehe
[ADR 0007](adr/0007-separate-client-workspace.md) für die verwandte
Begründung, warum der Client umgekehrt ein *eigener* Workspace ist).

Getestet: `docker build --platform linux/amd64 -f backend/Dockerfile .`
vom Repository-Root aus, per QEMU-Emulation auf einem Apple-Silicon-Mac,
gegen eine laufende Postgres-Instanz — Migrationen, Embedding-Modell,
Reranker und `/health`/`/version` funktionieren. `fastembed` ist auf
`ort-download-binaries-rustls-tls` + `hf-hub-rustls-tls` statt der
native-tls-Standardfeatures gepinnt (sonst fehlt im schlanken Debian-Image
OpenSSL komplett), und die Runtime-Stage braucht Debian **Trixie**, nicht
Bookworm — die von `ort` heruntergeladene ONNX-Runtime-Binary verlangt
glibc/libstdc++-Symbole, die Bookworms glibc 2.36 noch nicht hat.

Für die Synology DS220+ (x86_64) reicht das Image unverändert; ein
`--platform`-Override ist nur für den lokalen Test auf Apple Silicon
nötig, auf der NAS selbst baut Docker nativ für die richtige Architektur.

**Noch offen, weil nicht von hier aus planbar:** die Anbindung an den auf
der NAS bereits laufenden Cloudflare Tunnel (welches Docker-Netzwerk der
Tunnel-Container nutzt, der Public-Hostname-Eintrag, die Access-Application
und ein Service Token für die beiden Tauri-Clients) — das passiert live,
zusammen, beim eigentlichen Deploy.
