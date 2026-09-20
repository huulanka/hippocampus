-- ADR 0004: audio is the original, the transcript is interpretation.
-- ADR 0005: content lives outside the event log, so redaction is possible
-- without weakening the UPDATE/DELETE rule on `events`.
--
-- `events` keeps only structure and references from here on. Capture and
-- transcript text/audio move into their own tables, which are ordinary
-- writable tables — that is the whole point: the log of what happened
-- stays inviolable while the content it points at can be removed.

create table capture_content (
    event_id     uuid primary key references events (id),
    -- 'audio': the recording is the original and `text` stays null.
    -- 'text': typed or dictated straight into the app; the text itself is
    -- the original and is never derived from anything.
    origin       text not null check (origin in ('audio', 'text')),
    text         text,
    audio_path   text,
    audio_mime   text,
    duration_ms  integer,
    -- Set when the content was deliberately removed. The row stays as a
    -- tombstone so the capture's existence and time remain visible.
    redacted_at  timestamptz,
    check (
        (origin = 'text' and (text is not null or redacted_at is not null))
        or (origin = 'audio' and (audio_path is not null or redacted_at is not null))
    )
);

-- Every transcript ever produced for a capture, newest not implied by any
-- ordering — `supersedes` carries that. Corrections (ADR 0005) append a
-- row here rather than changing one.
create table transcript_content (
    event_id          uuid primary key references events (id),
    capture_event_id  uuid not null references events (id),
    text              text not null,
    -- ASR model id, or 'user' for a human correction.
    model             text not null,
    language          text,
    supersedes        uuid references events (id),
    created_at        timestamptz not null default now(),
    redacted_at       timestamptz
);

create index transcript_content_capture_idx
    on transcript_content (capture_event_id, created_at desc);

-- Backfill: existing captures carried their text inside the event payload,
-- which is exactly what ADR 0004 moves away from. The payloads stay as they
-- are (the log is append-only), but the content is now also readable from
-- the place the application looks from here on.
insert into capture_content (event_id, origin, text)
select id, 'text', payload ->> 'transcript_text'
from events
where event_type = 'capture.recorded'
  and payload ->> 'transcript_text' is not null;
