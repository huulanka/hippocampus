-- Event-sourcing core: append-only log of everything that ever happened.
-- Nothing in this table is ever UPDATEd or DELETEd by application code.
create extension if not exists pgcrypto;
create extension if not exists vector;

create table events (
    id           uuid primary key default gen_random_uuid(),
    stream_id    uuid not null,
    version      bigint not null,
    event_type   text not null,
    payload      jsonb not null,
    source       text not null, -- device name, model id, or "user"
    occurred_at  timestamptz not null default now(),
    unique (stream_id, version)
);

create index events_stream_id_idx on events (stream_id, version);
create index events_event_type_idx on events (event_type);
create index events_occurred_at_idx on events (occurred_at);

-- Prevent UPDATE/DELETE at the database level, not just by convention.
create rule events_no_update as on update to events do instead nothing;
create rule events_no_delete as on delete to events do instead nothing;

-- Projections: derived read-models, rebuildable from `events`.

create table entity_type_registry (
    entity_type  text primary key,
    label        text,
    icon         text,
    color        text,
    created_at   timestamptz not null default now()
);

create table entities (
    id               uuid primary key default gen_random_uuid(),
    entity_type      text not null references entity_type_registry (entity_type),
    name             text not null,
    current_summary  text,
    embedding        vector(384), -- matches the default fastembed multilingual model
    created_at       timestamptz not null default now(),
    updated_at       timestamptz not null default now()
);

create index entities_type_idx on entities (entity_type);
create index entities_embedding_idx on entities using hnsw (embedding vector_cosine_ops);

create table observations (
    id               uuid primary key default gen_random_uuid(),
    entity_id        uuid not null references entities (id),
    source_event_id  uuid not null references events (id),
    text             text not null,
    model            text not null,
    confidence       real,
    created_at       timestamptz not null default now()
);

create index observations_entity_id_idx on observations (entity_id);
create index observations_source_event_id_idx on observations (source_event_id);

create table relations (
    id               uuid primary key default gen_random_uuid(),
    from_entity_id   uuid not null references entities (id),
    to_entity_id     uuid not null references entities (id),
    relation_type    text not null,
    source_event_id  uuid not null references events (id),
    model            text not null,
    created_at       timestamptz not null default now()
);

create index relations_from_idx on relations (from_entity_id);
create index relations_to_idx on relations (to_entity_id);

-- Full-text search over raw capture transcripts (payload->>'transcript_text'
-- for event_type = 'capture.recorded').
create table capture_search (
    event_id     uuid primary key references events (id),
    transcript   text not null,
    tsv          tsvector generated always as (to_tsvector('german', transcript)) stored,
    embedding    vector(384),
    occurred_at  timestamptz not null
);

create index capture_search_tsv_idx on capture_search using gin (tsv);
create index capture_search_embedding_idx on capture_search using hnsw (embedding vector_cosine_ops);
