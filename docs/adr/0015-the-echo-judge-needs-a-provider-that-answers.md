# ADR 0015: The echo judge needs a provider that answers

## Status
Accepted (2026-09-24). Replaces the model choice in ADR 0010; everything
else in 0010 stands.

## Context
ADR 0010 moved the echo judgement to a hosted model and chose
`mistralai/mistral-small-2603` for its German. In production the echo then
almost never appeared. The cause was not speed: a successful judgement
took about a second. Nearly every call came back `429 Too Many Requests`.

The logs only had the status code, because the response body was thrown
away. Once the body was logged, it said:

> mistralai/mistral-small-2603 is temporarily rate-limited upstream.
> Please retry shortly, or add your own key to accumulate your rate
> limits.

That model has exactly one provider on OpenRouter, Mistral itself, and
Mistral throttles the route OpenRouter shares between its users. Nothing
on the OpenRouter account side causes this or can fix it, and with one
provider there is nothing to fall back to. The structuring model, called
through the same account and the same zero-retention routing, never got a
single 429.

A second weakness made the first one worse. A failed judgement was only
retried when somebody read the capture, and the client stops asking after
45 seconds. A capture whose judgement failed while it was open never got
an echo at all.

## Decision

**The default judge is `google/gemini-3.5-flash-lite`,** the model that
already does structuring and is served by several zero-retention
endpoints.

**A fallback model sits behind it,** `mistralai/mistral-small-3.2-24b-instruct`,
which has three zero-retention providers. The request sends both in
OpenRouter's `models` list, and OpenRouter tries them in order. So a
provider outage costs one failed attempt, not the judgement.
`ECHO_JUDGE_FALLBACK_MODEL` changes the fallback, and `none` turns it off.

**`judged_by` records the model that answered,** read from the response,
not the model that was asked first. With a fallback these differ, and the
stored echo has to say which one judged it.

**Unjudged captures are judged in the background.** A loop looks every
minute for captures that have a searchable row and no judgement, newest
first, and judges them one after the other. Each capture keeps its own
backoff, starting at `ECHO_JUDGE_RETRY_BACKOFF_SECS` and doubling with
every failure up to an hour. A pass stops at the first failure, because
the next capture would go to the same provider that just said no. Reading
a capture still starts a judgement too, under the same backoff.

**A failed model call logs OpenRouter's answer,** for every caller, not
only the judge. The answer is what tells a rate limit on the account
apart from a rate limit upstream, and the two need opposite fixes.

## Measured
`backend/examples/judge_compare.rs` sends the production prompt with
invented German notes: nine new notes, four or five earlier ones each,
36 pairs labelled as echo, not an echo, or undecided. The cases target
the ways echo has gone wrong before. Some share words or a casual tone but
not a subject ("tropft schon wieder" about a cold and about the heating).
Some share a subject only through world knowledge (a sauna infusion and a
sauna visit). In one case nothing should match at all. Each model judged
every case three times.

| | gemini-3.5-flash-lite | mistral-small-3.2-24b-instruct | mistral-small-2603 |
| --- | --- | --- | --- |
| Calls answered | 27 of 27 | 27 of 27 | 0 of 29, all 429 |
| Latency, median / max | 555 ms / 1.3 s | 686 ms / 5.6 s | — |
| True echoes shown (≥ 0.5) | 44 of 45 | 43 of 45 | — |
| Non-echoes shown | 2 of 72 | 5 of 72 | — |
| Runs with a non-echo ranked above a true echo | 0 | 2 | — |

Gemini's two false hits both scored exactly 0.5 (a climbing plant for a
bouldering session). Several narrow true echoes also scored exactly 0.5.
The threshold sits on the model's own "related" step, so raising it would
cost real echoes to remove rare false ones. It stays at 0.5.

## Consequences
- The German argument from ADR 0010 is set aside. On the measured cases
  it did not decide anything: the model chosen for German answered almost
  no calls, and the model that does answer judged German better.
- The fallback judges somewhat worse than the default. It only judges
  when the default is unreachable, and a slightly worse echo is better
  than none.
- Echoes judged by the old model keep that judgement. They are finished
  and correct as far as they go; provenance says which model made them,
  should they ever need judging again.
- The backoff lives in memory, like the in-flight set from ADR 0010. A
  restart resets it, which costs at most one early retry per capture.
