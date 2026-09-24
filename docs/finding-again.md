# Finding things again

Like `docs/product.md`, this document says *why* and *what*. The *how* for
meetings is in ADR 0016, for answering questions in ADR 0017.

## The problem it solves

Capturing works, and the echo brings back what is close in meaning to a
note just spoken. Everything else that comes back has to be asked for:
the search field, the graph, an entity page. Three things made that
harder than it needs to be, and they are the same three that make human
memory work where this one did not.

1. **A note knew nothing about its circumstances.** People remember
   through context: "I said that in the meeting with Paul", "that was the
   afternoon of the workshop". The notes carried a time and nothing else,
   although the Mac's calendar knows which meeting was running.
2. **Nearness in time connected nothing.** Ten notes from one workshop
   belong together even when they are about different things. Echo and
   search only ever compared meaning.
3. **An entity was a list, not knowledge.** After twenty mentions of
   something a person has a sense of what it is and where it stands. The
   entity page had twenty notes and, as its summary, whichever of them
   came last.

And the search field could find notes, but not answer the question you
actually had.

## What it does

| Piece | What you see | Where |
|---|---|---|
| **The meeting around a note** | "During a meeting with Paul and the harbour portal", "20 min before a meeting with Lena" | Under the time of a note |
| **Episodes** | "Said in the same stretch": the notes before and after that carry on from each other | On a note |
| **Gist** | Two to four sentences about an entity, each with the notes it rests on | At the top of an entity page |
| **Asking** | A question in the search field gets an answer above the matches, each sentence numbered with its notes | Search, on the Mac and the phone |

## Decisions

| # | Decision | Why |
|---|---|---|
| R1 | **Time nominates, a reader decides.** A meeting near a note and a note shortly before it are only candidates; a model reads the note and says whether it belongs | Around a meeting people say all sorts of unrelated things: a shopping list, a private errand, another project. The same split as entity resolution: similarity finds candidates, a reader judges them (`docs/entity-resolution.md`) |
| R2 | **Of a meeting, only who and what it was with is kept**, and where the note stood: before, during or after, and how many minutes. No title, no attendee list, no times | The calendar stays the calendar's (ADR 0013). What is kept is a pointer into the graph the memory already holds. See ADR 0016 |
| R3 | **The window is an hour either side**, half an hour selectable | Preparation happens over lunch and follow-up after the next coffee. A note between two meetings may belong to both: follow-up of one, preparation for the next |
| R4 | **Every Mac looks up every note once**, including notes from the phone | A note spoken on the phone during a meeting in the work calendar was spoken during that meeting. Each Mac answers only for the calendars ticked on it |
| R5 | **Old notes are looked up too**, once a calendar is ticked | The calendar remembers last month's meetings, so last month's notes can learn theirs |
| R6 | **An episode is a chain of "carries on from"**: each note is read against up to three notes from the half hour before it | Chains reach through the gaps of a long workshop without making "the same afternoon" an episode by default |
| R7 | **A gist exists from three observations on**, and is written again whenever the entity is observed again | The gist of two notes is the two notes (P12: the weight of evidence per entity) |
| R8 | **Things change, and the gist and the answer say so**: the later note is the current state, the earlier one is named with its date as what it was before | Two observations of equal standing, "works at A" and "is now at B", are the most dangerous kind of wrong: correctly quoted and out of date. No belief layer is modelled for it (`docs/memory-model.md`); the rule lives in the writing |
| R9 | **Asking lives in the search field.** A question gets an answer above the matches; "Ask" forces one for anything the field does not recognise as a question | One field for "where was that" and "what was that". No chat place of its own, and no conversation to keep |
| R10 | **An answer is written only from notes this code chose**, at most thirty, and never from the archive as a whole | The search hits, the notes about what the question names, one step out along the graph, the notes that belonged to a meeting with them, and the notes the best hits continue. See ADR 0017 |
| R11 | **Every sentence cites its notes and is read a second time against them**; what fails either is dropped, and the answer says how many were | The weekly story's contract, plus a check. A sentence that cannot be followed back to your words is a rumour |
| R12 | **"Your notes don't say" is an answer**, shown with the closest notes | Better than a near miss. A memory system that always answers is one that guesses |
| R13 | **"Quotes only"** leaves the model's sentences out and shows only the notes | For the moments when only your own words will do |
| R14 | **Nothing about a question is stored** | A question is not a note. It is asked, answered and gone |

## Why this does not break "no chat"

`docs/product.md` excluded chat and RAG over the captures, because a
memory system must not confabulate about your own past. That reason
still stands; what changed is the shape of the answer. Chat, as the
exclusion meant it, is a model that talks about your life with the
archive somewhere behind it. What is built is a page of your notes,
arranged: every sentence carries the numbers of the notes it rests on, a
second reading has checked it against exactly those notes, and the notes
are right beneath it. Where the notes are silent, it says so.

The route is the one structuring already takes: text only, to a provider
with zero data retention, behind Touch ID on the Mac. No other model or
agent gets access to the memory.

## What is not built

- **Meetings on the phone.** The phone has no calendar access. Its notes
  still learn their meetings, from the Macs (R4).
- **Asking by voice.** The shortcut stays the record button (ADR 0012).
- **A conversation.** Each question stands alone; there is no follow-up
  that remembers the previous answer.
- **Showing the path an answer took through the graph.** It would make
  visible which entities and neighbours were drawn on; noted for later.
