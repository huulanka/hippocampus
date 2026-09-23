//! Which of the things you know about a meeting is about.
//!
//! A calendar event arrives as a title and a list of attendee names, and
//! the question is which entities those mention. Deliberately **not** a
//! similarity score and not a model call: the answer drives a signal in
//! the menu bar, and a signal has to be explainable in one line — "because
//! *Paul* is in the title" — or it cannot be trusted and will be ignored.
//! So a match is a name, or a known alias, appearing whole in the words of
//! the title or of an attendee. Nothing fuzzier than folding case and
//! accents.
//!
//! The price is a missed match now and then ("Paul N." will not find
//! "Paul Hartmann" through the title — though the attendee list usually
//! does). The alternative, a fuzzy one, costs a false sparkle, and a false
//! sparkle is what teaches someone to stop looking.

use uuid::Uuid;

/// Something the graph holds, with every name it is known by.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: Uuid,
    pub entity_type: String,
    /// The canonical name first, then aliases.
    pub names: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchedOn {
    Title,
    Attendee,
}

impl MatchedOn {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Attendee => "attendee",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub id: Uuid,
    pub on: MatchedOn,
    /// The words as they stand in the calendar.
    pub text: String,
    /// How many characters of name matched. Longer is surer: "Paul
    /// Hartmann" beats "Paul", and both beat a four-letter project code.
    strength: usize,
}

/// Words that say what kind of meeting it is rather than what it is
/// about. An entity called "Review" or "Team" exists in real graphs, and
/// without this it would be on the page of every meeting that has one.
#[rustfmt::skip]
const FORM_WORDS: &[&str] = &[
    "meeting", "call", "termin", "jour", "fixe", "sync", "weekly", "daily", "standup", "stand",
    "up", "review", "retro", "besprechung", "telefonat", "abstimmung", "update", "team",
    "workshop", "session", "kickoff", "kick", "off", "planning", "check", "in", "mit", "und",
    "with", "and", "the", "der", "die", "das", "zu", "fur", "for", "von", "am", "im", "an",
    "online", "teams", "zoom", "intern", "extern", "privat", "1", "2", "3", "q1", "q2", "q3", "q4",
];

/// Shortest single word worth matching on its own. Two-letter names are
/// initials and abbreviations, and match everywhere.
const MIN_WORD: usize = 3;

/// One word, folded, with where it came from in the original string.
#[derive(Debug)]
struct Word {
    folded: String,
    start: usize,
    end: usize,
}

/// Lowercase, accents off, "ß" to "ss". The same folding on both sides,
/// so "Günter" in the graph finds "Gunter" in a calendar synced through
/// something that strips diacritics — and the other way round.
fn fold(word: &str) -> String {
    let mut out = String::with_capacity(word.len());
    for c in word.chars().flat_map(char::to_lowercase) {
        match c {
            'ä' | 'á' | 'à' | 'â' | 'å' | 'ã' => out.push('a'),
            'ö' | 'ó' | 'ò' | 'ô' | 'õ' | 'ø' => out.push('o'),
            'ü' | 'ú' | 'ù' | 'û' => out.push('u'),
            'é' | 'è' | 'ê' | 'ë' => out.push('e'),
            'í' | 'ì' | 'î' | 'ï' => out.push('i'),
            'ç' => out.push('c'),
            'ñ' => out.push('n'),
            'ß' => out.push_str("ss"),
            other => out.push(other),
        }
    }
    out
}

/// Splits on anything that is not a letter or a digit — so
/// "Hafenportal-Deadline" is two words, and "paul.hartmann@firma.example" is
/// four.
fn words(text: &str) -> Vec<Word> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (offset, c) in text.char_indices() {
        if c.is_alphanumeric() {
            start.get_or_insert(offset);
        } else if let Some(from) = start.take() {
            out.push(Word {
                folded: fold(&text[from..offset]),
                start: from,
                end: offset,
            });
        }
    }
    if let Some(from) = start {
        out.push(Word {
            folded: fold(&text[from..]),
            start: from,
            end: text.len(),
        });
    }
    out
}

/// The folded words of a name, or `None` if the name is too generic to
/// match anything by itself.
fn name_words(name: &str) -> Option<Vec<String>> {
    let folded: Vec<String> = words(name).into_iter().map(|w| w.folded).collect();
    let meaningful = folded
        .iter()
        .filter(|w| !FORM_WORDS.contains(&w.as_str()))
        .count();
    if meaningful == 0 {
        return None;
    }
    if folded.len() == 1 && folded[0].chars().count() < MIN_WORD {
        return None;
    }
    Some(folded)
}

/// Where `needle` appears as a run of whole words in `hay`, as a byte
/// range of the original string.
fn find_run(hay: &[Word], needle: &[String]) -> Option<(usize, usize)> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    hay.windows(needle.len())
        .find(|window| window.iter().zip(needle).all(|(w, n)| &w.folded == n))
        .map(|window| (window[0].start, window[window.len() - 1].end))
}

/// Every candidate the meeting mentions, strongest first, each once.
pub fn find(candidates: &[Candidate], title: &str, people: &[String]) -> Vec<Match> {
    let title_words = words(title);
    let people_words: Vec<(&String, Vec<Word>)> = people
        .iter()
        .map(|person| (person, words(person)))
        .collect();

    let mut found: Vec<Match> = Vec::new();
    for candidate in candidates {
        let mut best: Option<Match> = None;
        for name in &candidate.names {
            let Some(needle) = name_words(name) else {
                continue;
            };
            let strength = needle.iter().map(|w| w.chars().count()).sum::<usize>();

            let mut consider = |on: MatchedOn, text: String| {
                let better = best.as_ref().is_none_or(|b| {
                    strength > b.strength
                        // The same name found both ways: the attendee
                        // list is the surer source — a name in a title
                        // can be the subject, an attendee is the person.
                        || (strength == b.strength && on == MatchedOn::Attendee && b.on == MatchedOn::Title)
                });
                if better {
                    best = Some(Match {
                        id: candidate.id,
                        on,
                        text,
                        strength,
                    });
                }
            };

            if let Some((from, to)) = find_run(&title_words, &needle) {
                consider(MatchedOn::Title, title[from..to].to_string());
            }
            for (person, person_words) in &people_words {
                if find_run(person_words, &needle).is_some() {
                    consider(MatchedOn::Attendee, person.trim().to_string());
                }
            }
        }
        if let Some(best) = best {
            found.push(best);
        }
    }

    found.sort_by_key(|m| std::cmp::Reverse(m.strength));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(n: u128, names: &[&str]) -> Candidate {
        Candidate {
            id: Uuid::from_u128(n),
            entity_type: "Person".into(),
            names: names.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn ids(matches: &[Match]) -> Vec<u128> {
        matches.iter().map(|m| m.id.as_u128()).collect()
    }

    #[test]
    fn a_first_name_in_the_title_is_found() {
        let found = find(&[candidate(1, &["Paul"])], "Jour fixe Paul", &[]);
        assert_eq!(ids(&found), vec![1]);
        assert_eq!(found[0].on, MatchedOn::Title);
        assert_eq!(found[0].text, "Paul");
    }

    #[test]
    fn an_attendee_is_found_by_first_name_or_by_email() {
        let paul = [candidate(1, &["Paul"])];
        let by_name = find(&paul, "Weekly", &["Paul Hartmann".into()]);
        assert_eq!(ids(&by_name), vec![1]);
        assert_eq!(by_name[0].on, MatchedOn::Attendee);
        assert_eq!(by_name[0].text, "Paul Hartmann");

        let by_mail = find(
            &paul,
            "Weekly",
            &["paul.hartmann@northwind.example".into()],
        );
        assert_eq!(ids(&by_mail), vec![1]);
    }

    #[test]
    fn a_hyphenated_title_still_names_the_project() {
        let found = find(
            &[candidate(2, &["Hafenportal"])],
            "Hafenportal-Deadline besprechen",
            &[],
        );
        assert_eq!(ids(&found), vec![2]);
    }

    #[test]
    fn part_of_a_word_is_not_the_word() {
        // "Paul" inside "Paulaner" is a different thing entirely.
        assert!(find(&[candidate(1, &["Paul"])], "Hafen Paulaner", &[]).is_empty());
        // And a multi-word name has to be there whole and in order.
        assert!(
            find(
                &[candidate(3, &["Northwind Abrechnungs Projekt"])],
                "Northwind Jour fixe",
                &[]
            )
            .is_empty()
        );
    }

    #[test]
    fn accents_and_eszett_fold_on_both_sides() {
        let found = find(&[candidate(4, &["Günter Strauß"])], "1:1 Gunter Strauss", &[]);
        assert_eq!(ids(&found), vec![4]);
    }

    #[test]
    fn an_alias_finds_the_entity_it_belongs_to() {
        let found = find(
            &[candidate(5, &["Hippocampus Projekt", "Hippocampus"])],
            "Hippocampus Planung",
            &[],
        );
        assert_eq!(ids(&found), vec![5]);
        assert_eq!(found[0].text, "Hippocampus");
    }

    #[test]
    fn a_name_that_only_says_what_kind_of_meeting_it_is_matches_nothing() {
        // Real entities with these names exist; every meeting would light up.
        let generic = [
            candidate(6, &["Review"]),
            candidate(7, &["Team Meeting"]),
            candidate(8, &["QS"]),
        ];
        assert!(find(&generic, "Sprint Review Team Meeting QS", &[]).is_empty());
    }

    #[test]
    fn the_longer_name_ranks_first_and_each_entity_appears_once() {
        let found = find(
            &[
                candidate(1, &["Paul"]),
                candidate(9, &["Hafenportal Relaunch"]),
            ],
            "Hafenportal Relaunch mit Paul",
            &["Paul Hartmann".into()],
        );
        assert_eq!(ids(&found), vec![9, 1]);
        // Found in the title and among the attendees: reported once, and
        // as the attendee, which is the surer of the two.
        assert_eq!(found[1].on, MatchedOn::Attendee);
    }
}
