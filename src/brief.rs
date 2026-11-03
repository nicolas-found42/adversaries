//! Read Markdown structure without inventing decision content or confidence.
use pulldown_cmark::{Event, Parser, Tag, TagEnd};

#[derive(Debug, Clone)]
pub struct BriefSection {
    pub title: String,
    pub body: String,
}

pub fn sections(text: &str) -> Vec<BriefSection> {
    let mut headings = Vec::new();
    let mut active = None;
    for (event, range) in Parser::new(text).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { .. }) => active = Some((range.start, String::new())),
            Event::Text(label) | Event::Code(label) => {
                if let Some((_, title)) = &mut active {
                    title.push_str(&label);
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((start, title)) = active.take() {
                    headings.push((start, range.end, title));
                }
            }
            _ => {}
        }
    }
    headings
        .iter()
        .enumerate()
        .map(|(i, (_, end, title))| {
            let next = headings.get(i + 1).map_or(text.len(), |h| h.0);
            BriefSection {
                title: title.clone(),
                body: text[*end..next].trim().to_owned(),
            }
        })
        .collect()
}

/// Availability requires body text, not just whitespace, a heading or a rule.
/// This is a presentation check, never a factual-quality or agreement score.
pub fn is_available(text: &str) -> bool {
    let mut heading = false;
    Parser::new(text).any(|event| match event {
        Event::Start(Tag::Heading { .. }) => {
            heading = true;
            false
        }
        Event::End(TagEnd::Heading(_)) => {
            heading = false;
            false
        }
        Event::Text(text) | Event::Code(text) => !heading && !text.trim().is_empty(),
        _ => false,
    })
}

/// Derived presentation cache; checks the text itself so in-place edits with
/// unchanged length cannot leave the reader with a stale readiness result.
#[derive(Default)]
pub struct AvailabilityCache {
    text: String,
    available: bool,
}

impl AvailabilityCache {
    pub fn check(&mut self, text: &str) -> bool {
        if self.text != text {
            self.available = is_available(text);
            self.text.clear();
            self.text.push_str(text);
        }
        self.available
    }
}

pub fn qualified_export(text: &str, practice: bool) -> String {
    let mode = if practice {
        "Practice example: simulated content."
    } else {
        "Model-generated content."
    };
    format!(
        "{mode}\nNot independently verified. Any reported confidence is model-reported, not a calibrated probability. Debate outcomes do not establish factual truth.\n\n{text}"
    )
}

/// A recovery export preserves complete arguments without presenting a partial
/// debate as a brief or claiming that the visible work was saved durably.
pub fn completed_arguments_export(
    question: &str,
    turns: &[crate::model::TurnState],
    practice: bool,
) -> String {
    use std::fmt::Write;
    let mut text = format!(
        "Incomplete debate — no decision brief\nQuestion: {question}\nOnly completed arguments are included. This copy does not confirm saved storage.\n"
    );
    for turn in turns
        .iter()
        .filter(|turn| turn.status == crate::model::TurnStatus::Complete)
    {
        let _ = writeln!(
            text,
            "\n## Stage {} · {} · {}\n\n{}",
            turn.index, turn.role, turn.model, turn.text
        );
    }
    qualified_export(&text, practice)
}

/// A single-stage copy carries its completion status outside the interface.
pub fn argument_export(turn: &crate::model::TurnState, practice: bool) -> String {
    let status = match turn.status {
        crate::model::TurnStatus::Complete => {
            "Completed stage output. Completion does not verify its claims."
        }
        _ => "Incomplete stage output. Do not treat this text as a completed argument.",
    };
    qualified_export(
        &format!(
            "Stage {} · {} · {}\n{status}\n\n{}",
            turn.index, turn.role, turn.model, turn.text
        ),
        practice,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sections_preserve_actual_markdown_without_fabrication() {
        let doc = "# Brief\n\n## Recommendation\n\n**Keep A.**\n\n## Confidence\n\n72%\n";
        let parsed = sections(doc);
        assert_eq!(parsed[1].title, "Recommendation");
        assert_eq!(parsed[1].body, "**Keep A.**");
        assert_eq!(parsed[2].body, "72%");
        assert!(sections("Unstructured output with no headings").is_empty());
    }
}

/// Accept the engine's explicit bold-label format as well as heading sections.
pub fn recommendation(text: &str) -> Option<String> {
    sections(text)
        .into_iter()
        .find(|s| s.title.eq_ignore_ascii_case("recommendation") && !s.body.is_empty())
        .map(|s| s.body)
        .or_else(|| inline_value(text, "Recommendation:"))
}

pub fn inline_value(text: &str, wanted: &str) -> Option<String> {
    let (markers, ends) = inline_markers(text);
    let (_, start, _) = markers
        .iter()
        .find(|(_, _, label)| label.eq_ignore_ascii_case(wanted))?;
    let next = markers
        .iter()
        .filter(|(pos, _, _)| pos > start)
        .map(|(pos, _, _)| *pos)
        .min()
        .unwrap_or(text.len());
    let paragraph = ends
        .into_iter()
        .find(|end| end > start)
        .unwrap_or(text.len());
    let value = text[*start..next.min(paragraph)].trim();
    (!value.is_empty()).then(|| value.to_owned())
}

type InlineMarkers = (Vec<(usize, usize, String)>, Vec<usize>);

fn inline_markers(text: &str) -> InlineMarkers {
    let mut active = None;
    let mut markers = Vec::new();
    let mut ends = Vec::new();
    for (event, range) in Parser::new(text).into_offset_iter() {
        match event {
            Event::Start(Tag::Strong) => active = Some((range.start, String::new())),
            Event::Text(label) => {
                if let Some((_, value)) = &mut active {
                    value.push_str(&label);
                }
            }
            Event::End(TagEnd::Strong) => {
                if let Some((start, label)) = active.take()
                    && ["Recommendation:", "Confidence:"]
                        .iter()
                        .any(|s| label.eq_ignore_ascii_case(s))
                {
                    markers.push((start, range.end, label));
                }
            }
            Event::End(TagEnd::Paragraph) => ends.push(range.end),
            _ => {}
        }
    }
    (markers, ends)
}

#[cfg(test)]
mod inline_tests {
    use super::*;
    #[test]
    fn engine_lead_keeps_markdown_and_separates_confidence() {
        let text = "# Decision Brief: Test\n\n**Recommendation:** Choose **option A** by Q2. **Confidence:** Medium, because evidence is incomplete.\n\n## Risks\n\nDelay.";
        assert_eq!(
            recommendation(text).as_deref(),
            Some("Choose **option A** by Q2.")
        );
        assert_eq!(
            inline_value(text, "Confidence:").as_deref(),
            Some("Medium, because evidence is incomplete.")
        );
        assert!(recommendation("```\n**Recommendation:** Fake field in code\n```").is_none());
    }
}
