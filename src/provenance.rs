//! Deterministic occurrence matching against the material actually reviewed.
//! Matching an excerpt never establishes the truth of a claim or a source.
use crate::model::Claim;
use crate::transport::RunConfig;

#[derive(Debug, Clone)]
pub struct MatchedExcerpt {
    pub name: String,
    pub document_number: Option<usize>,
    pub excerpt: String,
    pub source: String,
}

pub fn supplied_excerpts(
    claim: &Claim,
    question: &str,
    config: Option<&RunConfig>,
) -> Vec<MatchedExcerpt> {
    if !claim.text.contains("[SOURCE EXCERPT MATCHED]") {
        return Vec::new();
    }
    let Some(quote) = quoted_source(&claim.text) else {
        return Vec::new();
    };
    let mut sources = vec![("Decision question".to_owned(), question.to_owned(), None)];
    if let Some(documents) = config.and_then(|c| c.context_snapshot.as_ref()) {
        sources.extend(
            documents
                .iter()
                .enumerate()
                .map(|(index, d)| (d.name.clone(), d.text.clone(), Some(index + 1))),
        );
    }
    sources
        .into_iter()
        .filter_map(|(name, source, document_number)| {
            let excerpt = exact_occurrence(&source, quote)?.to_owned();
            Some(MatchedExcerpt {
                name,
                document_number,
                excerpt,
                source,
            })
        })
        .collect()
}

fn quoted_source(text: &str) -> Option<&str> {
    let start = text
        .as_bytes()
        .windows(7)
        .position(|w| w.eq_ignore_ascii_case(b"source:"))?;
    text[start + 7..]
        .trim_start()
        .strip_prefix('"')?
        .split_once('"')
        .map(|(quote, _)| quote)
}

fn normalized(text: &str) -> (String, Vec<(usize, usize)>) {
    let mut out = String::new();
    let mut spans = Vec::new();
    let mut space = None;
    for (start, ch) in text.char_indices() {
        let end = start + ch.len_utf8();
        if ch.is_whitespace() {
            space = Some((space.map_or(start, |(s, _)| s), end));
            continue;
        }
        if let Some(span) = space.take()
            && !out.is_empty()
        {
            out.push(' ');
            spans.push(span);
        }
        for lower in ch.to_lowercase() {
            out.push(lower);
            spans.extend(std::iter::repeat_n((start, end), lower.len_utf8()));
        }
    }
    (out, spans)
}

pub fn exact_occurrence<'a>(source: &'a str, quote: &str) -> Option<&'a str> {
    let (source_normalized, spans) = normalized(source);
    let needle = normalized(quote).0;
    if needle.is_empty() {
        return None;
    }
    let start = source_normalized.find(&needle)?;
    source.get(spans[start].0..spans[start + needle.len() - 1].1)
}

#[cfg(test)]
mod tests {
    use super::exact_occurrence;
    #[test]
    fn normalized_match_retains_exact_unicode_and_whitespace_source() {
        assert_eq!(
            exact_occurrence("Préface: NAIROBI\n  has €50 available.", "nairobi has €50"),
            Some("NAIROBI\n  has €50")
        );
        assert_eq!(exact_occurrence("Unrelated", "Different"), None);
        assert_eq!(exact_occurrence("Unrelated", " "), None);
    }

    #[test]
    fn matching_documents_with_the_same_name_keep_separate_source_positions() {
        use crate::{
            claims::extract_claims,
            model::{TurnState, TurnStatus},
            preflight::DocumentSnapshot,
            transport::RunConfig,
        };
        let mut turn = TurnState::new(1, "Synthetic model".into(), "Opening".into());
        turn.text =
            "C1: [SOURCE EXCERPT MATCHED] Supplied statement | source: \"shared evidence\"".into();
        turn.status = TurnStatus::Complete;
        let claim = extract_claims(&[turn]).remove(0);
        let name = "Planning café & costs #1 + 100%.txt";
        let config = RunConfig {
            context_snapshot: Some(
                ["First: shared evidence.", "Second: shared evidence."]
                    .into_iter()
                    .map(|text| DocumentSnapshot {
                        name: name.into(),
                        text: text.into(),
                        ..Default::default()
                    })
                    .collect(),
            ),
            ..Default::default()
        };
        let found = super::supplied_excerpts(&claim, "Unrelated decision question", Some(&config));
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].name, name);
        assert_eq!(found[1].name, name);
        assert_eq!(found[0].document_number, Some(1));
        assert_eq!(found[1].document_number, Some(2));
        assert_eq!(found[0].source, "First: shared evidence.");
        assert_eq!(found[1].source, "Second: shared evidence.");
        let mut only_second = config.clone();
        only_second.context_snapshot.as_mut().unwrap()[0].text =
            "No matching quote in this source.".into();
        let found =
            super::supplied_excerpts(&claim, "Unrelated decision question", Some(&only_second));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].document_number, Some(2));
    }
}
