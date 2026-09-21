-- Echo becomes something the system stores, not something it recomputes.
--
-- An echo looks only at captures that are strictly *earlier* than the one
-- being echoed, and those never change. So the answer is fixed the moment
-- a capture is recorded — yet it was being recomputed on every capture
-- detail view, cross-encoder and all. Measured against the NAS: 27
-- seconds to open a page whose answer had already been computed once.
--
-- Two tables rather than one, because "judged, and nothing echoed" has to
-- be distinguishable from "not judged yet". Without that marker every
-- capture with no echoes would be re-judged forever.

create table capture_echo (
    capture_event_id uuid not null references events (id) on delete cascade,
    echo_event_id    uuid not null references events (id) on delete cascade,
    -- Position as the judge ordered it, 0 first. Kept rather than derived
    -- from the score so the stored order survives a threshold change.
    rank             int  not null,
    -- The judge's verdict, on whatever scale that judge uses. Every
    -- candidate is stored, including ones below the display threshold:
    -- that way the threshold can be retuned, and calibration runs like
    -- `?min_rerank=-99` answered, without paying for the judgement again.
    score            real not null,
    -- What the bi-encoder thought before the judge saw it. Kept because
    -- the gap between the two is the evidence for whether the judge is
    -- earning its place.
    similarity       real not null,
    primary key (capture_event_id, echo_event_id)
);

create index capture_echo_rank_idx on capture_echo (capture_event_id, rank);

-- One row per capture whose echo has been decided, naming what decided
-- it. The model is provenance, exactly as `observations.model` is: when
-- the judge changes, this says which echoes predate the change and can be
-- re-judged deliberately rather than all at once.
create table capture_echo_judged (
    capture_event_id uuid primary key references events (id) on delete cascade,
    judged_by        text not null,
    judged_at        timestamptz not null default now()
);
