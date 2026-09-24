//! Which hosted model should judge echoes.
//!
//! Every hosted judge gets the same invented German notes, with the same
//! prompt and request shape as `src/judge.rs`, a few times over, because
//! these models do not answer the same way twice. For each model this
//! reports:
//!
//! - how often a request failed (a 429 counts; the retry is counted too),
//! - latency of the calls that succeeded,
//! - per case, the lowest score of a true echo against the highest score
//!   of a non-echo — a positive margin means the order is right,
//! - hits and misses at the display threshold.
//!
//! The cases are built around the ways echo has gone wrong before:
//! notes that share surface words or a casual register but not a subject,
//! notes that share a subject only through world knowledge, and one case
//! where nothing should match at all. Pairs labelled `Maybe` are printed
//! but not scored — a reasonable person could go either way.
//!
//! Run from `backend/` with `OPENROUTER_API_KEY` set:
//! `cargo run --release --example judge_compare -- [model ...]`

use std::time::{Duration, Instant};

use serde_json::{Value, json};

const SYSTEM_PROMPT: &str = include_str!("../src/judge_prompt.txt");

const DEFAULT_MODELS: &[&str] = &[
    "google/gemini-3.5-flash-lite",
    "mistralai/mistral-small-2603",
];

/// Same as `DEFAULT_REMOTE_MIN_SCORE`.
const THRESHOLD: f32 = 0.5;
const RUNS: usize = 3;
const MAX_ATTEMPTS: usize = 6;

#[derive(Clone, Copy, PartialEq)]
enum Label {
    Echo,
    Not,
    Maybe,
}
use Label::*;

struct Case {
    new: &'static str,
    earlier: &'static [(&'static str, Label)],
}

const CASES: &[Case] = &[
    Case {
        new: "Also Sauna heut war schon geil",
        earlier: &[
            ("heute Zimtschnecken gebacken", Not),
            ("Heut Abend war ich in der Sauna", Echo),
            ("Heute war schon ein geiler Tag im Büro", Not),
            (
                "Der finnische Aufguss war heute brutal heiß, alle sind rausgeflüchtet",
                Echo,
            ),
            ("Muss noch den Rasenmäher zum Service bringen", Not),
        ],
    },
    Case {
        new: "Mit Jonas die Radtour an den Bodensee für Juni festgemacht",
        earlier: &[
            (
                "Juni wird im Büro stressig wegen dem Quartalsabschluss",
                Not,
            ),
            (
                "Jonas meinte, wir sollten dieses Jahr mal länger mit dem Rad unterwegs sein",
                Echo,
            ),
            (
                "Mit Tante Erika telefoniert, sie will im Sommer zu Besuch kommen",
                Not,
            ),
            ("Jonas hat am dritten März Geburtstag", Maybe),
            (
                "Fähre von Konstanz nach Meersburg fährt alle 15 Minuten, gut für die Tour",
                Echo,
            ),
        ],
    },
    Case {
        new: "Die Heizung im Keller tropft schon wieder",
        earlier: &[
            (
                "Es tropft mir schon wieder aus der Nase, Erkältung im Anmarsch",
                Not,
            ),
            (
                "Installateur angerufen wegen dem Leck an der Heizung, kommt Donnerstag",
                Echo,
            ),
            ("Keller aufräumen, alte Kisten zum Wertstoffhof", Maybe),
            ("Unter dem Heizkessel steht eine Pfütze", Echo),
            ("Wieder den Bus verpasst", Not),
        ],
    },
    Case {
        new: "Idee für die App: Notizen nach Orten gruppieren",
        earlier: &[
            ("Idee fürs Abendessen: Ofengemüse mit Feta", Not),
            (
                "Feature-Wunsch: eine Karte, auf der man sieht, wo man was gedacht hat",
                Echo,
            ),
            ("Orte in Portugal für den Urlaub rausgesucht", Not),
            ("Neue Bohrmaschine bestellt", Not),
            (
                "Für die App wäre ein Rückblick am Ende der Woche schön",
                Echo,
            ),
        ],
    },
    Case {
        new: "Kardamom-Espresso probiert, schmeckt überraschend gut",
        earlier: &[
            (
                "Heute an der App gearbeitet und ein neues Feature ausprobiert",
                Not,
            ),
            ("Rezept-Idee: Kaffeebohnen mit Kardamom rösten", Echo),
            ("Zahnarzttermin auf nächste Woche verschoben", Not),
            ("Zimtschnecken mit Kardamom gebacken", Maybe),
            (
                "Neue Espressomühle eingestellt, jetzt passt der Mahlgrad",
                Echo,
            ),
        ],
    },
    Case {
        new: "Wieder schlecht geschlafen, bin um vier aufgewacht",
        earlier: &[
            ("Um vier Uhr Termin mit dem Vermieter", Not),
            (
                "Seit Tagen wach ich nachts auf und kann nicht mehr einschlafen",
                Echo,
            ),
            ("Die Katze schläft den ganzen Tag auf der Heizung", Not),
            (
                "Vielleicht weniger Kaffee am Nachmittag, damit ich besser schlafe",
                Echo,
            ),
            ("Wieder den Schlüssel im Büro liegen lassen", Not),
        ],
    },
    Case {
        new: "Getränkemarkt nicht vergessen, Wasser ist alle",
        earlier: &[
            (
                "Wasserrohrbruch beim Nachbarn, der ganze Flur ist nass",
                Not,
            ),
            ("Nicht vergessen: Paket zur Post bringen", Not),
            ("Morgen dringend Sprudel und Apfelschorle holen", Echo),
            ("Heute zwei Liter getrunken, fühlt sich gut an", Not),
        ],
    },
    Case {
        new: "Präsentation für Freitag ist fertig, nur die Folien zum Budget fehlen noch",
        earlier: &[
            ("Freitag Pizzaabend bei Mia", Not),
            (
                "Budgetzahlen fürs dritte Quartal bei der Buchhaltung angefragt",
                Echo,
            ),
            ("Fertig mit dem Buch, das Ende war enttäuschend", Not),
            (
                "Am Freitag stell ich dem Team das Konzept vor, bin etwas nervös",
                Echo,
            ),
        ],
    },
    Case {
        new: "Heute zum ersten Mal Bouldern gewesen, Unterarme sind komplett platt",
        earlier: &[
            ("Heute zum ersten Mal Sushi selbst gerollt", Not),
            ("Kletterpflanze am Balkon umgetopft", Not),
            ("Heute war ein guter Tag", Not),
            ("Zum ersten Mal mit dem neuen Nachtzug gefahren", Not),
        ],
    },
];

struct Outcome {
    scores: Vec<f32>,
    missing: usize,
    latency: Duration,
}

struct Stats {
    calls_ok: usize,
    calls_failed: usize,
    gave_up: usize,
    latencies: Vec<Duration>,
    unparsable: usize,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let api_key = std::env::var("OPENROUTER_API_KEY")?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let models: Vec<String> = if args.is_empty() {
        DEFAULT_MODELS.iter().map(|m| m.to_string()).collect()
    } else {
        args
    };
    let http = reqwest::Client::new();

    for model in &models {
        println!("\n=== {model}");
        let mut stats = Stats {
            calls_ok: 0,
            calls_failed: 0,
            gave_up: 0,
            latencies: Vec::new(),
            unparsable: 0,
        };
        // [case][run] -> scores
        let mut all: Vec<Vec<Vec<f32>>> = vec![Vec::new(); CASES.len()];

        for run in 0..RUNS {
            for (c, case) in CASES.iter().enumerate() {
                match judge(&http, &api_key, model, case, &mut stats).await {
                    Some(outcome) => {
                        if outcome.missing > 0 {
                            println!(
                                "  run {run} case {c}: {} candidates missing from the answer",
                                outcome.missing
                            );
                        }
                        stats.latencies.push(outcome.latency);
                        all[c].push(outcome.scores);
                    }
                    None => stats.gave_up += 1,
                }
            }
        }

        report(&all, &stats);
    }
    Ok(())
}

async fn judge(
    http: &reqwest::Client,
    api_key: &str,
    model: &str,
    case: &Case,
    stats: &mut Stats,
) -> Option<Outcome> {
    let listing = case
        .earlier
        .iter()
        .enumerate()
        .map(|(i, (doc, _))| format!("{i}. {doc}"))
        .collect::<Vec<_>>()
        .join("\n");
    let body = json!({
        "model": model,
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": format!("NEW note:\n{}\n\nEARLIER notes:\n{listing}", case.new)},
        ],
        "response_format": {"type": "json_object"},
        "provider": {"zdr": true},
    });

    for attempt in 0..MAX_ATTEMPTS {
        let started = Instant::now();
        let response = http
            .post("https://openrouter.ai/api/v1/chat/completions")
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await;
        let latency = started.elapsed();
        let response = match response {
            Ok(r) => r,
            Err(err) => {
                println!("  transport error: {err}");
                stats.calls_failed += 1;
                continue;
            }
        };
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            stats.calls_failed += 1;
            println!(
                "  attempt {attempt}: {status} {}",
                text.chars().take(300).collect::<String>()
            );
            tokio::time::sleep(Duration::from_secs(2 << attempt)).await;
            continue;
        }
        stats.calls_ok += 1;
        let value: Value = serde_json::from_str(&text).ok()?;
        let content = value["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("");
        let Some((scores, missing)) = parse(content, case.earlier.len()) else {
            stats.unparsable += 1;
            println!(
                "  unparsable answer: {}",
                content.chars().take(200).collect::<String>()
            );
            return None;
        };
        return Some(Outcome {
            scores,
            missing,
            latency,
        });
    }
    None
}

/// Mirrors `judge::parse_scores`, and also counts what the model left out.
fn parse(content: &str, expected: usize) -> Option<(Vec<f32>, usize)> {
    let value: Value = serde_json::from_str(content.trim()).ok()?;
    let mut scores = vec![None; expected];
    for verdict in value["scores"].as_array()? {
        let (Some(id), Some(score)) = (verdict["id"].as_u64(), verdict["score"].as_f64()) else {
            continue;
        };
        if let Some(slot) = scores.get_mut(id as usize) {
            *slot = Some((score as f32).clamp(0.0, 1.0));
        }
    }
    let missing = scores.iter().filter(|s| s.is_none()).count();
    Some((
        scores.into_iter().map(|s| s.unwrap_or(0.0)).collect(),
        missing,
    ))
}

fn report(all: &[Vec<Vec<f32>>], stats: &Stats) {
    let (mut tp, mut fn_, mut fp, mut tn) = (0, 0, 0, 0);
    let mut wrong_order = 0;
    let mut max_spread = 0.0_f32;

    for (case, runs) in CASES.iter().zip(all) {
        println!("\n  NEW: {}", case.new);
        for (i, (doc, label)) in case.earlier.iter().enumerate() {
            let scores: Vec<f32> = runs.iter().map(|r| r[i]).collect();
            let spread = scores.iter().cloned().fold(f32::MIN, f32::max)
                - scores.iter().cloned().fold(f32::MAX, f32::min);
            if !scores.is_empty() {
                max_spread = max_spread.max(spread);
            }
            let tag = match label {
                Echo => "ECHO ",
                Not => "not  ",
                Maybe => "maybe",
            };
            let shown = scores
                .iter()
                .map(|s| format!("{s:.2}"))
                .collect::<Vec<_>>()
                .join(" ");
            println!("    {tag} [{shown}]  {doc}");
            for s in &scores {
                match (label, *s >= THRESHOLD) {
                    (Echo, true) => tp += 1,
                    (Echo, false) => fn_ += 1,
                    (Not, true) => fp += 1,
                    (Not, false) => tn += 1,
                    (Maybe, _) => {}
                }
            }
        }
        for run in runs {
            let worst_echo = case
                .earlier
                .iter()
                .zip(run)
                .filter(|((_, l), _)| *l == Echo)
                .map(|(_, s)| *s)
                .fold(f32::MAX, f32::min);
            let best_not = case
                .earlier
                .iter()
                .zip(run)
                .filter(|((_, l), _)| *l == Not)
                .map(|(_, s)| *s)
                .fold(f32::MIN, f32::max);
            if worst_echo != f32::MAX && worst_echo <= best_not {
                wrong_order += 1;
            }
        }
    }

    let mut ms: Vec<u128> = stats.latencies.iter().map(|d| d.as_millis()).collect();
    ms.sort();
    let pct = |p: f64| {
        ms.get(((ms.len() as f64 - 1.0) * p).round() as usize)
            .copied()
            .unwrap_or(0)
    };
    println!("\n  --- summary");
    println!(
        "  calls: {} ok, {} failed, {} judgements abandoned, {} unparsable",
        stats.calls_ok, stats.calls_failed, stats.gave_up, stats.unparsable
    );
    println!(
        "  latency ms: median {}, p90 {}, max {}",
        pct(0.5),
        pct(0.9),
        pct(1.0)
    );
    println!(
        "  at {THRESHOLD}: echoes shown {tp}/{}, non-echoes shown {fp}/{}",
        tp + fn_,
        fp + tn
    );
    println!("  runs where a non-echo scored >= a true echo: {wrong_order}");
    println!("  largest score spread for one pair across runs: {max_spread:.2}");
}
