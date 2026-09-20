//! Structure extraction via OpenRouter: turns a raw transcript into a set
//! of candidate entities/relations. This is the only place original text
//! leaves the machine — as text, never audio — and only to a provider
//! routed with Zero Data Retention when `zdr` is enabled.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

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

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ExtractedRelation {
    pub from: String,
    pub to: String,
    pub relation_type: String,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize, Serialize)]
pub struct ExtractionResult {
    #[serde(default)]
    pub entities: Vec<ExtractedEntity>,
    #[serde(default)]
    pub relations: Vec<ExtractedRelation>,
}

const SYSTEM_PROMPT: &str = r#"You extract structured knowledge from a short personal voice-note transcript. The transcript may be in German or English.

Identify every distinct thing worth remembering: people, projects, topics, ideas, decisions, tasks, or literally anything else the speaker mentions (health, recipes, travel, hobbies — anything). Do not force items into a fixed category list; use whatever entity_type fits best (reuse common ones like "Person", "Projekt", "Thema", "Idee", "Entscheidung", "Aufgabe" when they genuinely fit, invent others freely otherwise).

For each entity, write a short first-person observation capturing what was just learned about it from this transcript alone (not general knowledge).

Also list relations between entities mentioned in the same transcript. Each relation has EXACTLY these three keys: "from", "to", "relation_type" (never "entity_type" — that key belongs only to entities). Use the exact same `name` values as in your entities list for "from"/"to".

Every entity may additionally carry "when" and "when_precision" when the observation is about a point in time. Resolve relative expressions ("morgen", "nächsten Dienstag", "letzte Woche", "im Sommer") against the moment given in the user message, and write the result as ISO 8601: "2026-09-22" for a day, "2026-09-22T19:00" only when a time of day was actually named. "when_precision" is one of "time", "day", "week", "month", "year" — use the coarsest one that is still honest. Omit both keys entirely when the observation is not about a point in time; most are not.

Respond with ONLY a JSON object of this exact shape, no prose, no markdown fences:
{"entities": [{"name": "...", "entity_type": "...", "observation": "...", "when": "...", "when_precision": "..."}], "relations": [{"from": "...", "to": "...", "relation_type": "..."}]}

If nothing is worth extracting, respond with {"entities": [], "relations": []}."#;

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

    pub async fn extract(
        &self,
        transcript: &str,
        spoken_at: SpokenAt,
    ) -> anyhow::Result<ExtractionResult> {
        let body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": format!("{}\n\nTranscript:\n{}", spoken_at.describe(), transcript)},
            ],
            "response_format": {"type": "json_object"},
            "provider": {"zdr": self.zdr},
        });

        let response = self
            .http
            .post("https://openrouter.ai/api/v1/chat/completions")
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;

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

    Ok(ExtractionResult {
        entities,
        relations,
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
