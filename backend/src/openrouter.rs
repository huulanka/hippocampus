//! Structure extraction via OpenRouter: turns a raw transcript into a set
//! of candidate entities/relations. This is the only place original text
//! leaves the machine — as text, never audio — and only to a provider
//! routed with Zero Data Retention when `zdr` is enabled.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const CHAT_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

/// One chat completion, with the telemetry every caller wants.
///
/// A failed call keeps OpenRouter's answer. `error_for_status` used to
/// throw it away, and the answer is the diagnosis: a 429 says whether the
/// account hit a limit or one upstream provider is throttling everybody
/// ("temporarily rate-limited upstream"), and those two call for opposite
/// fixes. With only the status code, the echo judge failed several
/// hundred times before anyone could tell which one it was.
pub(crate) async fn chat(
    http: &reqwest::Client,
    api_key: &str,
    purpose: &str,
    model: &str,
    body: &Value,
) -> anyhow::Result<Value> {
    let started = std::time::Instant::now();
    let response = match http
        .post(CHAT_URL)
        .bearer_auth(api_key)
        .json(body)
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => {
            crate::telemetry::model_call_failed(purpose, model, started.elapsed(), &err);
            return Err(err.into());
        }
    };

    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        // Bounded: an error page is not supposed to be long, but the log
        // line is read by a person and should not become one.
        let detail: String = text.chars().take(500).collect();
        let err = anyhow::anyhow!("OpenRouter answered {status}: {detail}");
        crate::telemetry::model_call_failed(purpose, model, started.elapsed(), &err);
        return Err(err);
    }

    let payload = match response.json::<Value>().await {
        Ok(payload) => payload,
        Err(err) => {
            crate::telemetry::model_call_failed(purpose, model, started.elapsed(), &err);
            return Err(err.into());
        }
    };
    crate::telemetry::model_call(purpose, model, &payload, started.elapsed());
    Ok(payload)
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ExtractedEntity {
    pub name: String,
    pub entity_type: String,
    pub observation: String,
    /// When the observation is *about*, resolved against the moment the
    /// note was spoken. ISO 8601: a date, or a date and time when the
    /// speaker named one. `None` when the observation is not about a
    /// point in time at all, which is the common case.
    #[serde(default)]
    pub when: Option<String>,
    /// How precise `when` really is. Stored because precision is part of
    /// the fact: "next summer" is not a timestamp, and rounding it to one
    /// would invent an accuracy the speaker never had.
    #[serde(default)]
    pub when_precision: Option<String>,
}

/// The moment a note was spoken, in the speaker's own timezone.
///
/// Without this the extraction has no way to turn "morgen" into a date —
/// and a note that says "Dienstag" is unreadable three weeks later, which
/// is exactly when a memory system gets read.
#[derive(Debug, Clone, Copy)]
pub struct SpokenAt {
    pub utc: DateTime<Utc>,
    pub timezone: Tz,
}

impl SpokenAt {
    /// The line handed to the model. Spelled out rather than an ISO
    /// stamp: a weekday name is what makes "nächsten Dienstag" resolvable
    /// at all.
    fn describe(&self) -> String {
        let local = self.utc.with_timezone(&self.timezone);
        format!(
            "This note was spoken on {} ({}), local time, in timezone {}. Today is therefore {}.",
            local.format("%A, %-d %B %Y at %H:%M"),
            local.format("%Y-%m-%dT%H:%M%:z"),
            self.timezone.name(),
            local.format("%Y-%m-%d"),
        )
    }
}

/// One thing the graph already holds, offered to the extraction so it can
/// recognise it rather than invent a variant of it.
#[derive(Debug, Clone)]
pub struct KnownEntity {
    pub name: String,
    pub entity_type: String,
    pub summary: Option<String>,
}

/// What the memory already contains, as far as it could bear on this
/// transcript.
///
/// The cheapest half of entity resolution: a name the model reuses is a
/// duplicate that never has to be found and merged afterwards. Measured
/// on the real graph, four of the six duplicate pairs existed *only*
/// because the type vocabulary was invented afresh per note — `Sauna`
/// as both `Ort` and `Aktivität`, `Northwind` as both `Organisation`
/// and `Kunde`. Those pairs simply do not arise once the model is told
/// which types are already in use.
#[derive(Debug, Clone, Default)]
pub struct KnownGraph {
    /// Entity types already in use, most-used first.
    pub types: Vec<String>,
    /// Entities that might be what this transcript is about.
    pub entities: Vec<KnownEntity>,
}

impl KnownGraph {
    /// The block handed to the model.
    ///
    /// The closing sentence is not decoration. Without it a model shown
    /// forty known entities helpfully returns all forty, each with a
    /// plausible observation it invented — which in a memory system is
    /// the one unforgivable output, because a fabricated memory is
    /// indistinguishable from a real one.
    fn describe(&self) -> String {
        let mut out = String::new();

        if !self.types.is_empty() {
            out.push_str(
                "Entity types already in use in this person's memory. Reuse one of these                  whenever it fits; invent a new type only when none of them does:\n",
            );
            out.push_str(&self.types.join(", "));
            out.push_str("\n\n");
        }

        if !self.entities.is_empty() {
            out.push_str(
                "Things already in this person's memory that this transcript may be                  referring to. If it is talking about one of them, use that exact `name`                  and `entity_type` rather than a variant or a synonym of it:\n",
            );
            for entity in &self.entities {
                out.push_str(&format!("- {} ({})", entity.name, entity.entity_type));
                if let Some(summary) = entity.summary.as_deref().filter(|s| !s.trim().is_empty()) {
                    // Truncated: this is here to disambiguate which
                    // "Paul" is meant, not to retell what is known about
                    // him.
                    let short: String = summary.chars().take(160).collect();
                    out.push_str(&format!(" — {short}"));
                }
                out.push('\n');
            }
            out.push_str(
                "\nThis list is for recognition only. Include an entity in your answer ONLY                  if THIS transcript actually says something about it. Never invent an                  observation for an entity the transcript does not mention.\n\n",
            );
        }

        out
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ExtractedRelation {
    /// Also accepted as `name`: the model now and then writes a relation
    /// in the shape of the entity it just wrote, and every such relation
    /// used to be dropped whole for one key.
    #[serde(alias = "name")]
    pub from: String,
    pub to: String,
    pub relation_type: String,
}

/// Something the speaker means to do, say or ask later.
///
/// Heard, not commanded: nobody says "remind me" into a note about their
/// day, they say "muss ich Paul noch fragen". That sentence has no date,
/// so it never reached `upcoming`; it hangs on Paul instead, and this is
/// what lets it come back when Paul does (docs/prospective-memory.md).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ExtractedIntention {
    /// Short, imperative, in the speaker's language: "Paul nach der
    /// Hafenportal-Deadline fragen".
    pub text: String,
    /// The speaker's own words it was heard in. Checked against the
    /// transcript before it is kept — see `structuring::verbatim`.
    #[serde(default)]
    pub quote: Option<String>,
    /// Names from this extraction's own `entities` list. Anything else is
    /// dropped on the way in, the same rule relations follow.
    #[serde(default)]
    pub about: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize, Serialize)]
pub struct ExtractionResult {
    #[serde(default)]
    pub entities: Vec<ExtractedEntity>,
    #[serde(default)]
    pub relations: Vec<ExtractedRelation>,
    #[serde(default)]
    pub intentions: Vec<ExtractedIntention>,
}

const SYSTEM_PROMPT: &str = r#"You extract structured knowledge from a short personal voice-note transcript. The transcript may be in German or English.

Identify every distinct thing worth remembering: people, projects, topics, ideas, decisions, tasks, or literally anything else the speaker mentions (health, recipes, travel, hobbies — anything).

Naming matters more than coverage here, because these names are how the speaker will find this again. Two rules:
- If the user message lists entity types already in use, prefer one of those. Invent a new type only when none of them honestly fits. A type that already exists under a different word ("Kunde" when "Organisation" is in the list) is a wrong answer.
- If the user message lists things already in memory and this transcript is about one of them, use that exact name. "Espresso mit Kardamom" when "Kardamom-Espresso" is already known is a wrong answer; so is "Paul" when "Paul Hartmann" is known to be the person being talked about.

Naming two genuinely different things the same is worse than naming one thing twice, so when you are unsure whether the transcript means the known entity or a new one, treat it as new.

For each entity, write a short first-person observation capturing what was just learned about it from this transcript alone (not general knowledge).

Also list relations between entities mentioned in the same transcript. Each relation has EXACTLY these three keys: "from", "to", "relation_type" (never "entity_type" — that key belongs only to entities). Use the exact same `name` values as in your entities list for "from"/"to".

Also list the speaker's intentions: things they say they still mean to do, say, ask, bring up, check or send later, and have not done yet ("muss ich Paul noch fragen", "beim nächsten Termin mit Northwind ansprechen", "sollte ich mal ausprobieren", "remind me to…"). Each intention has "text" (a short imperative phrase in the transcript's language that names who or what it is about, e.g. "Paul nach der Hafenportal-Deadline fragen"), "quote" (the exact words from the transcript it comes from, copied character for character, never paraphrased), and "about" (the `name` values from your entities list it concerns — the people, projects or things it would come up with). Only intentions the speaker actually expresses: a plan already carried out, a wish with no intent to act, or something someone else will do is not one. Most transcripts contain none; then return an empty list.

Every entity may additionally carry "when" and "when_precision" when the observation is about a point in time. Resolve relative expressions ("morgen", "nächsten Dienstag", "letzte Woche", "im Sommer") against the moment given in the user message, and write the result as ISO 8601: "2026-09-22" for a day, "2026-09-22T19:00" only when a time of day was actually named. "when_precision" is one of "time", "day", "week", "month", "year" — use the coarsest one that is still honest. Omit both keys entirely when the observation is not about a point in time; most are not.

Respond with ONLY a JSON object of this exact shape, no prose, no markdown fences:
{"entities": [{"name": "...", "entity_type": "...", "observation": "...", "when": "...", "when_precision": "..."}], "relations": [{"from": "...", "to": "...", "relation_type": "..."}], "intentions": [{"text": "...", "quote": "...", "about": ["..."]}]}

If nothing is worth extracting, respond with {"entities": [], "relations": [], "intentions": []}."#;

pub struct OpenRouterClient {
    http: reqwest::Client,
    api_key: String,
    model: String,
    zdr: bool,
}

impl OpenRouterClient {
    pub fn new(api_key: String, model: String, zdr: bool) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key,
            model,
            zdr,
        }
    }

    /// The OpenRouter model id used for extraction, recorded as provenance
    /// on every entity/observation/relation it produces.
    pub fn model_name(&self) -> &str {
        &self.model
    }

    /// One JSON-answering call with a system prompt of the caller's own.
    ///
    /// For the readings that live next to the code they serve (the context
    /// judge, the gist, the answer to a question) rather than in here. Same
    /// model, same zero-retention routing, same telemetry as extraction.
    pub(crate) async fn complete_json<T: serde::de::DeserializeOwned>(
        &self,
        purpose: &str,
        system: &str,
        user: &str,
    ) -> anyhow::Result<T> {
        let body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "response_format": {"type": "json_object"},
            "provider": {"zdr": self.zdr},
        });

        let response = chat(&self.http, &self.api_key, purpose, &self.model, &body).await?;
        let content = response["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("{purpose} response had no content"))?;
        Ok(serde_json::from_str(strip_fences(content))?)
    }

    pub async fn extract(
        &self,
        transcript: &str,
        spoken_at: SpokenAt,
        known: &KnownGraph,
    ) -> anyhow::Result<ExtractionResult> {
        // Known graph first, then the moment, then the words. The order
        // is the order the model needs them in: what things are called,
        // when this was said, what was said.
        let user_message = format!(
            "{}{}\n\nTranscript:\n{}",
            known.describe(),
            spoken_at.describe(),
            transcript,
        );

        let body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": user_message},
            ],
            "response_format": {"type": "json_object"},
            "provider": {"zdr": self.zdr},
        });

        let response = chat(&self.http, &self.api_key, "structuring", &self.model, &body).await?;

        let content = response["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("OpenRouter response had no message content"))?;

        parse_extraction(content)
    }
}

#[derive(Deserialize)]
struct RawExtraction {
    #[serde(default)]
    entities: Vec<Value>,
    #[serde(default)]
    relations: Vec<Value>,
    #[serde(default)]
    intentions: Vec<Value>,
}

/// A model asked for bare JSON still wraps it in a Markdown fence now
/// and then.
pub(crate) fn strip_fences(raw: &str) -> &str {
    raw.trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
}

/// Parses the model's JSON response. Split out from `extract` so it can be
/// unit-tested without a network call.
///
/// Parses entities/relations one at a time rather than the whole array at
/// once: a model occasionally emits one malformed item (wrong field name,
/// missing key), and that item alone should be skipped rather than
/// discarding every other entity/relation the same response found.
fn parse_extraction(raw: &str) -> anyhow::Result<ExtractionResult> {
    let cleaned = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let raw: RawExtraction = serde_json::from_str(cleaned)?;

    let entities = raw
        .entities
        .into_iter()
        .filter_map(|v| match serde_json::from_value::<ExtractedEntity>(v.clone()) {
            Ok(entity) => Some(entity),
            Err(err) => {
                tracing::warn!(?err, value = %v, "skipping malformed entity in extraction response");
                None
            }
        })
        .collect();

    let relations = raw
        .relations
        .into_iter()
        .filter_map(
            |v| match serde_json::from_value::<ExtractedRelation>(v.clone()) {
                Ok(relation) => Some(relation),
                Err(err) => {
                    tracing::warn!(?err, value = %v, "skipping malformed relation in extraction response");
                    None
                }
            },
        )
        .collect();

    let intentions = raw
        .intentions
        .into_iter()
        .filter_map(
            |v| match serde_json::from_value::<ExtractedIntention>(v.clone()) {
                // An intention with no words is not one, whatever else it
                // carries.
                Ok(intention) if !intention.text.trim().is_empty() => Some(intention),
                Ok(_) => None,
                Err(err) => {
                    tracing::warn!(?err, value = %v, "skipping malformed intention in extraction response");
                    None
                }
            },
        )
        .collect();

    Ok(ExtractionResult {
        entities,
        relations,
        intentions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_well_formed_response() {
        let raw = r#"{"entities":[{"name":"Lena","entity_type":"Person","observation":"sprach über Tourenplanung"},{"name":"HPortal","entity_type":"Projekt","observation":"braucht evtl. einen Workflow für Tourenplanung"}],"relations":[{"from":"HPortal","to":"Lena","relation_type":"besprochen_mit"}]}"#;
        let result = parse_extraction(raw).unwrap();
        assert_eq!(result.entities.len(), 2);
        assert_eq!(result.relations.len(), 1);
        assert_eq!(result.entities[0].name, "Lena");
    }

    #[test]
    fn skips_one_malformed_relation_without_losing_valid_entities() {
        // Regression test: a real OpenRouter response had one relation
        // missing `relation_type`, which previously discarded the whole
        // response — including two perfectly valid entities.
        let raw = r#"{"entities":[{"name":"Lena","entity_type":"Person","observation":"a"},{"name":"Contoso","entity_type":"Kunde","observation":"b"}],"relations":[{"from":"Lena","to":"Contoso"}]}"#;
        let result = parse_extraction(raw).unwrap();
        assert_eq!(result.entities.len(), 2);
        assert_eq!(result.relations.len(), 0);
    }

    #[test]
    fn a_relation_written_like_an_entity_is_kept() {
        // The model now and then writes `name` where `from` belongs,
        // carrying over the shape of the entity list it just wrote.
        let raw = r#"{"entities":[],"relations":[{"name":"Lena","to":"Contoso","relation_type":"arbeitet_bei"}]}"#;
        let result = parse_extraction(raw).unwrap();
        assert_eq!(result.relations.len(), 1);
        assert_eq!(result.relations[0].from, "Lena");
    }

    #[test]
    fn parses_an_intention_alongside_its_entities() {
        let raw = r#"{"entities":[{"name":"Paul","entity_type":"Person","observation":"soll nach der Deadline gefragt werden"}],"relations":[],"intentions":[{"text":"Paul nach der Hafenportal-Deadline fragen","quote":"muss ich Paul noch nach der Deadline fragen","about":["Paul"]}]}"#;
        let result = parse_extraction(raw).unwrap();
        assert_eq!(result.intentions.len(), 1);
        assert_eq!(result.intentions[0].about, vec!["Paul".to_string()]);
        assert_eq!(
            result.intentions[0].quote.as_deref(),
            Some("muss ich Paul noch nach der Deadline fragen")
        );
    }

    #[test]
    fn a_response_from_before_intentions_still_parses() {
        // The key is new. A model that leaves it out has found none, not
        // produced a malformed answer.
        let raw = r#"{"entities":[],"relations":[]}"#;
        assert!(parse_extraction(raw).unwrap().intentions.is_empty());
    }

    #[test]
    fn an_intention_without_words_is_dropped() {
        let raw = r#"{"entities":[],"relations":[],"intentions":[{"text":"  ","about":["Paul"]},{"about":["Paul"]},{"text":"Zahnarzt anrufen"}]}"#;
        let result = parse_extraction(raw).unwrap();
        assert_eq!(result.intentions.len(), 1);
        assert_eq!(result.intentions[0].text, "Zahnarzt anrufen");
        assert!(result.intentions[0].about.is_empty());
    }

    #[test]
    fn strips_markdown_code_fences() {
        let raw = "```json\n{\"entities\": [], \"relations\": []}\n```";
        let result = parse_extraction(raw).unwrap();
        assert_eq!(result, ExtractionResult::default());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_extraction("not json at all").is_err());
    }
}

/// One entity, as the consolidation run shows it to the model.
#[derive(Debug, Clone, Serialize)]
pub struct EntityDossier {
    /// Short handle used in the answer instead of a UUID. Models are
    /// unreliable at copying 36 hex characters and very reliable at
    /// copying `e12`.
    pub tag: String,
    pub name: String,
    pub entity_type: String,
    /// Other names this entity has answered to.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// What the person actually said about it, verbatim. This is the
    /// evidence — everything the model decides has to be defensible from
    /// these sentences and nothing else.
    pub observations: Vec<String>,
}

/// What one consolidation pass decided.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ConsolidationPlan {
    #[serde(default)]
    pub merges: Vec<PlannedMerge>,
    #[serde(default)]
    pub retypes: Vec<PlannedRetype>,
    #[serde(default)]
    pub relations: Vec<PlannedRelation>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlannedMerge {
    /// Tag of the entity that survives.
    pub keep: String,
    /// Tags of the entities folded into it.
    pub absorb: Vec<String>,
    /// What the surviving entity should be called afterwards. Usually the
    /// fuller form; every other name is kept as an alias regardless.
    #[serde(default)]
    pub name: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlannedRetype {
    pub tag: String,
    pub entity_type: String,
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlannedRelation {
    pub from: String,
    pub to: String,
    pub relation_type: String,
    pub reason: String,
}

/// The rules the consolidation run works under.
///
/// Written at length on purpose. This is the one prompt in the system
/// that is allowed to *change* the shape of what the person knows rather
/// than add to it, and the thing it can destroy — a distinction the
/// person actually draws — cannot be recovered by re-reading the notes,
/// because the notes will still be there and the graph will merely look
/// tidy and wrong.
///
/// The central rule, settled with Sauna and Finnischer Aufguss before any
/// of this was built: things that
/// belong together are not necessarily the same thing. Merging is for one
/// thing under two names. Everything else that belongs together is an
/// edge.
const CONSOLIDATION_PROMPT: &str = r#"You are tidying one person's private knowledge graph. It was built by extracting entities from their spoken notes, one note at a time, with no memory between notes — so the same thing often ended up recorded several times under different names or different type words.

The person's notes themselves are immutable and you never see or change them. You are only changing how what they said is organised.

You get a set of entities. Each has a tag, a name, a type, any other names it has answered to, and the observations recorded about it — the person's own words. Those observations are your only evidence. Never use outside knowledge about a real place, company or public figure to justify a decision.

Decide three kinds of thing.

1. MERGE — two or more entries are ONE thing recorded more than once.
   Merge when the difference is spelling, punctuation, word order, an abbreviation, or a fuller form of the same name: "Kardamom-Espresso" and "Espresso mit Kardamom"; "Hippocampus" and "Hippocampus Projekt"; "Paul" and "Paul Hartmann" when the observations show it is the same person.
   Do NOT merge a specific thing with the general thing it belongs to, and do NOT merge a thing with something it merely belongs to or is named after. Espresso and Kardamom-Espresso are not the same. Coffee and espresso are not the same. An ingredient and the dish it goes into are not the same. A project and the customer it is for are not the same — "Northwind Abrechnungsprojekt" is a project AT the company "Northwind", so those are two entries and one edge, never one entry. Sharing a word in the name is not evidence of being the same thing; it is usually evidence of an edge. If your own reason for merging contains "gehört zu", "beim Kunden", "ist Teil von", "findet statt in", "eine Art von" or anything like them, you have described a relation and must put it under relations instead.
   Merging destroys a distinction the person draws and will need again.
   Do NOT merge two people, projects or places whose names merely resemble each other. "Hafenportal" and "HPortal" are two projects. If the observations do not positively show these are the same thing, leave them apart.
   Choose the entry to keep, and give the name it should carry afterwards — normally the fullest, most specific form. Every other name is kept as a searchable alias automatically, so nothing is lost by choosing.

2. RETYPE — the type word is wrong, or is a synonym of one already used in this set.
   The type vocabulary was invented afresh per note, so synonyms accumulate. Unify them onto whichever word best fits the things that carry it. Prefer a type that already appears in this set. You MAY introduce a new type when nothing existing honestly fits — this vocabulary is meant to keep growing with the person's life, not to be frozen.
   Do not retype something merely to reduce the number of types. A type with one member that genuinely describes it is fine.

3. RELATE — two entries belong together but are not the same thing.
   This is where the pairs you refused to merge go: the specific and the general, the project and its customer, the dish and its ingredient, the person and their employer. Use a short lowercase German or English relation_type that reads as a sentence from `from` to `to`, and reuse a relation type you have already used in this answer rather than coining a second word for the same link.
   Only relate things the observations actually connect. Two notes that happen to mention food are not thereby related.

Across all three: when you are unsure, do nothing. Two entries left apart cost the person a little tidiness. Two different things fused into one cost them a distinction, silently, and it will look correct afterwards. Those costs are not comparable. An empty answer is a good answer.

Every decision needs a `reason`: one short sentence, in the language of the observations, that a person reading the changelog can check against their own notes.

Respond with ONLY this JSON object, no prose, no markdown fences:
{"merges": [{"keep": "e1", "absorb": ["e7"], "name": "...", "reason": "..."}], "retypes": [{"tag": "e3", "entity_type": "...", "reason": "..."}], "relations": [{"from": "e1", "to": "e4", "relation_type": "...", "reason": "..."}]}

Use {"merges": [], "retypes": [], "relations": []} when nothing should change."#;

impl OpenRouterClient {
    /// Asks the model what should be folded together, retyped or linked.
    pub async fn consolidate(
        &self,
        dossiers: &[EntityDossier],
    ) -> anyhow::Result<ConsolidationPlan> {
        let listing = serde_json::to_string_pretty(dossiers)?;

        let body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": CONSOLIDATION_PROMPT},
                {"role": "user", "content": format!("Entities:\n{listing}")},
            ],
            "response_format": {"type": "json_object"},
            "provider": {"zdr": self.zdr},
        });

        let payload = chat(
            &self.http,
            &self.api_key,
            "consolidation",
            &self.model,
            &body,
        )
        .await?;

        let content = payload["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("consolidation response had no content"))?;

        let cleaned = content
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        Ok(serde_json::from_str(cleaned)?)
    }
}

/// One note of the week, as the model sees it.
#[derive(Debug, Serialize)]
pub struct StoryNote {
    pub tag: String,
    /// Weekday, date and time, spelled out — "Tuesday" is what lets the
    /// model say "early in the week" without doing calendar arithmetic.
    pub said: String,
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct WrittenSentence {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub sources: Vec<String>,
}

#[derive(Deserialize)]
struct WrittenWeek {
    #[serde(default)]
    sentences: Vec<WrittenSentence>,
}

const WEEK_PROMPT: &str = r#"You write a short look back on one person's week, from the notes they captured during it. They will read it once, at the end of the week, to see what the week was about.

Write 3 to 4 sentences. Say what the week was about: the subjects that took up most of it, anything that changed or began, anything announced for later. Be concrete — name the things, people and places from the notes. Do not praise, advise, diagnose or speculate about feelings the notes do not state. Do not invent anything: every sentence must rest on specific notes, and you must cite them by tag.

Write in the language the notes are written in. Address the person as "du" in German, "you" in English.

Respond with ONLY this JSON object, no prose, no markdown fences:
{"sentences": [{"text": "...", "sources": ["n3", "n7"]}]}"#;

impl OpenRouterClient {
    /// Writes the week up from its notes. Every sentence cites the tags it
    /// rests on; the caller discards what it cannot trace back.
    pub async fn write_week(&self, notes: &[StoryNote]) -> anyhow::Result<Vec<WrittenSentence>> {
        let listing = serde_json::to_string_pretty(notes)?;

        let body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": WEEK_PROMPT},
                {"role": "user", "content": format!("Notes:\n{listing}")},
            ],
            "response_format": {"type": "json_object"},
            "provider": {"zdr": self.zdr},
        });

        let response = chat(&self.http, &self.api_key, "review", &self.model, &body).await?;

        let content = response["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("review response had no content"))?;
        let cleaned = content
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        Ok(serde_json::from_str::<WrittenWeek>(cleaned)?.sentences)
    }
}
