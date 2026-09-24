# ADR 0014: Structuring is a job any device may claim

## Status
Not built (2026-09-24). Spike d) in `docs/iphone.md` ran the structuring
prompt over real notes on the devices that can run a model, and neither
model was good enough to take the job from the backend. The design below
stands if a later model changes that; see *Measured* at the end.

## Context
Structuring runs on the backend and goes through OpenRouter (P10),
because the NAS is a Celeron without a GPU and cannot run a language
model. The note text leaves your own hardware for that, as text, to a
zero-retention provider. Structuring and the echo judge (ADR 0010) are the
only places where it does.

Apple's Foundation Models framework now runs a language model on the
device, with structured output and tool calling. In iOS 27 and macOS 27
it sits behind a `LanguageModel` protocol that serves the on-device model,
a model bundled through Core AI or MLX, and external providers from the
same call site. The model comes in two sizes, and not every device can run
both: the smaller one runs from an iPhone 15 Pro or an M1 up, the larger
one needs an iPhone 17 Pro or an M4. A realistic setup has a phone and
two Macs with different chips, so some devices can run the larger model
and some only the smaller.

The devices that can run a model are the clients. The backend, which
knows what needs structuring, cannot reach them: without push (free
signing has none, see `docs/iphone.md`) and with the Macs behind NAT,
there is no way for it to call a device and ask. And if every client
simply structured its own captures, a capture typed on a Mac that can
only run the small model would get the small model, while a phone with
the large one sat idle, and two clients that both saw an unstructured
capture would both structure it.

## Decision
**The backend keeps the queue; devices pull jobs from it under a lease.**

- `capture_pipeline` stays the one list of what needs structuring. Nothing
  changes about how a capture gets into it.
- A device with a model asks when it is awake and online:
  `POST /structuring/claim` with the model it can run. The backend picks
  the oldest unstructured capture that device may take, marks it leased
  to that device until a deadline (`FOR UPDATE SKIP LOCKED`, so two
  devices asking at once never get the same capture), and returns the
  transcript with the context the prompt needs.
- The device structures, and posts the result to
  `POST /structuring/{capture_id}` with the lease. The backend checks the
  lease, writes the same events it writes today, with the device's model
  in `model` and in `capture_pipeline.structured_by`, and clears the
  lease.
- A lease that runs out puts the capture back in the queue. A device that
  was put to sleep halfway loses nothing.
- **Stronger models go first.** The backend knows which models are good
  enough from spike d). A device with a model that is not good enough is
  never given a job. A device with a weaker but acceptable model is given
  a job only after the capture has waited for a stronger one for a while.
- **OpenRouter stays as the fallback.** If nobody has claimed a capture
  within a set time, the backend structures it itself, as it does today.
  With no device ever asking, the system behaves exactly as it does now.
- **Every device may take part**, including a work Mac. What passes
  through it is the text of one capture and the context for it, held in
  memory for the length of one model call. The archive stays on the NAS.

## Consequences
- The note text leaves your own hardware only when no device has picked
  it up in time. How often that happens depends on how often a device
  with a good enough model is awake and online.
- Structuring can arrive later than today. A capture spoken on the way
  home may wait until the phone is charging or a Mac wakes up. The
  timeline and search do not wait: a capture is findable before it is
  structured, as it is now.
- Different models will structure differently. The graph already copes
  with that (consolidation, P9, P12), and because every result names its
  model, a capture can be structured again later with a better one.
- Re-structuring the whole archive becomes a job for the devices too, run
  at night on whichever device is plugged in.
- Two endpoints, a lease column and a deadline on `capture_pipeline`, and
  a structuring path in the client that today exists only in the backend.
  The prompt must exist once, not in two copies that drift apart.
- The echo judge (ADR 0010) is a separate question. It can move the same
  way later, but this decision does not cover it.

## Measured
The unchanged production prompt, run over real notes with the same
context the backend would give, compared with what the current OpenRouter
model had made of the same notes.

- **The smaller model** (3B, the one an M1 runs) found most of the same
  entities under the same names and types, but reported an intention in
  almost every note, although the prompt says most notes have none, and
  most of those quotes were not the speaker's words. It invented more
  relations than there were, a good share of them pointing at entities it
  had not named. Its 4096-token context did not hold the longest note.
- **The larger model** (20B, on an iPhone 17 Pro) was more sensible about
  intentions and dates, but found fewer of the entities, still produced
  relations pointing nowhere, and returned JSON with a list left open in a
  sizeable share of answers. Guided generation would prevent the last
  one; it does not change the rest.
- **Both** were stopped by Apple's safety filter on a private note now and
  then, which the backend's model never is.
- **Private Cloud Compute** answered neither a command-line tool nor an
  app signed by a free Personal Team: the request never returned. It
  would only ever have been reachable from a device anyway, since the
  backend runs on Linux.

What would reopen this: a model that at least matches the backend's on
entities and intentions, under the free signing the app lives with.

