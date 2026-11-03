use crate::model::{Claim, ClaimVerdict, TurnState, TurnStatus};

pub fn extract_claims(turns: &[TurnState]) -> Vec<Claim> {
    let mut claims = Vec::new();
    let complete: Vec<_> = turns
        .iter()
        .filter(|t| t.status == TurnStatus::Complete)
        .collect();
    for turn in &complete {
        collect_claims_from_turn(turn, &mut claims);
    }
    for turn in &complete {
        link_attacks_from_turn(turn, &mut claims);
    }
    for turn in &complete {
        apply_rebuttal_verdicts(turn, &mut claims);
    }
    finalize_claim_verdicts(&mut claims);
    claims
}

/// A verbatim reference, without asserting that a shared label means the same claim.
pub struct ClaimReference<'a> {
    pub turn: &'a TurnState,
    pub text: &'a str,
}

pub fn references<'a>(turns: &'a [TurnState], claim: &Claim) -> Vec<ClaimReference<'a>> {
    let mut result = Vec::new();
    for turn in turns
        .iter()
        .filter(|turn| turn.status == TurnStatus::Complete)
    {
        let mut offset = 0;
        for chunk in turn.text.split_inclusive('\n') {
            let text = chunk.trim_end_matches(['\n', '\r']);
            let selected_line = turn.index == claim.turn_index && offset == claim.source_start;
            if !selected_line && line_references_claim(text, &claim.id) {
                result.push(ClaimReference { turn, text });
            }
            offset += chunk.len();
        }
    }
    result
}

fn collect_claims_from_turn(turn: &TurnState, claims: &mut Vec<Claim>) {
    let mut offset = 0;
    for chunk in turn.text.split_inclusive('\n') {
        let line = chunk.trim_end_matches(['\n', '\r']);
        if let Some((id, text)) = parse_claim_line(line) {
            claims.push(Claim {
                id,
                author_model: turn.model.clone(),
                turn_index: turn.index,
                text,
                source_start: offset,
                source_end: offset + line.len(),
                attackers: Vec::new(),
                ambiguous_references: Vec::new(),
                verdict: ClaimVerdict::Open,
            });
        }
        offset += chunk.len();
    }
}

fn parse_claim_line(line: &str) -> Option<(String, String)> {
    let stripped = line.trim().trim_start_matches(|c: char| {
        matches!(
            c,
            '|' | '>' | '#' | '*' | '_' | '`' | '-' | '+' | ' ' | '\t'
        )
    });
    for prefix in ["C", "R"] {
        if let Some(rest) = stripped.strip_prefix(prefix) {
            if let Some((num_str, body)) = split_number_and_body(rest) {
                let id = format!("{prefix}{num_str}");
                let text = clean_claim_body(body);
                if !text.is_empty() {
                    return Some((id, text));
                }
            }
        }
    }
    None
}

fn split_number_and_body(rest: &str) -> Option<(&str, &str)> {
    let digits_len = rest.chars().take_while(char::is_ascii_digit).count();
    if digits_len == 0 {
        return None;
    }
    let (num, remainder) = rest.split_at(digits_len);
    let body = remainder.trim_start();
    let body = body.trim_start_matches(['*', '_', '`']);
    let body = body.trim_start();
    let body = body
        .strip_prefix(':')
        .or_else(|| body.strip_prefix('.'))
        .or_else(|| body.strip_prefix('-'))
        .or_else(|| body.strip_prefix('—'))
        .or_else(|| body.strip_prefix('–'))
        .unwrap_or(body);
    let body = body.trim_start_matches(['*', '_', '`']);
    let body = body.trim();
    Some((num, body))
}

fn clean_claim_body(body: &str) -> String {
    body.trim()
        .trim_end_matches(['*', '_', '`'])
        .trim_end()
        .to_owned()
}

fn link_attacks_from_turn(turn: &TurnState, claims: &mut [Claim]) {
    if !turn.role.to_lowercase().contains("red") && !turn.role.to_lowercase().contains("attack") {
        return;
    }
    let counts = origin_counts(claims, turn.index);
    for claim in claims.iter_mut() {
        if claim.turn_index < turn.index && line_references_claim(&turn.text, &claim.id) {
            if counts.get(&claim.id).copied().unwrap_or(0) > 1 {
                claim.ambiguous_references.push(turn.index);
                continue;
            }
            if !claim.attackers.contains(&turn.model) {
                claim.attackers.push(turn.model.clone());
            }
        }
    }
}

fn line_references_claim(text: &str, claim_id: &str) -> bool {
    text.split(|c: char| !c.is_alphanumeric())
        .any(|token| token.eq_ignore_ascii_case(claim_id))
}

/// Applies the Lead Analyst's verdicts to claims cited in a Rebuttal turn.
///
/// Recognizes the GLOSSARY vocabulary plus its natural inflections:
/// CONCEDE/CONCEDING/CONCEDED and CONCESSION concede; REJECT, REBUT and
/// DEFEND (any inflection) reject — "rejected" means the attack was
/// rejected, so the claim stands. "partially" or "in part" escalates to
/// PartiallyConceded. The claim id may sit on either side of the verdict
/// keyword; matching is line-granular, so a line naming two claims under
/// one keyword assigns both that keyword's verdict.
fn apply_rebuttal_verdicts(turn: &TurnState, claims: &mut [Claim]) {
    if !turn.role.to_lowercase().contains("rebuttal") {
        return;
    }
    let upper = turn.text.to_uppercase();
    let counts = origin_counts(claims, turn.index);
    for claim in claims.iter_mut() {
        let id_upper = claim.id.to_uppercase();
        if claim.turn_index < turn.index
            && let Some(verdict) = detect_verdict_for_claim(&upper, &id_upper)
        {
            if counts.get(&claim.id).copied().unwrap_or(0) > 1 {
                claim.ambiguous_references.push(turn.index);
                continue;
            }
            claim.verdict = verdict;
        }
    }
}

fn detect_verdict_for_claim(text: &str, claim_id: &str) -> Option<ClaimVerdict> {
    for line in text.lines() {
        if let Some(offset) = verdict_keyword_offset(line)
            && line_references_claim(line, claim_id)
        {
            return Some(verdict_from_keyword_tail(&line[offset..]));
        }
    }
    None
}

/// Classifies a verdict from the earliest keyword's tail. Partial markers
/// outrank plain concession; whatever remains rejects the attack.
fn verdict_from_keyword_tail(tail: &str) -> ClaimVerdict {
    const PARTIAL: [&str; 4] = [
        "PARTIALLY CONCEDING",
        "PARTIALLY CONCEDE",
        "PARTIAL CONCESSION",
        "IN PART",
    ];
    const CONCEDE: [&str; 3] = ["CONCEDING", "CONCEDE", "CONCESSION"];
    if PARTIAL.iter().any(|marker| tail.contains(marker)) {
        return ClaimVerdict::PartiallyConceded;
    }
    if CONCEDE.iter().any(|marker| tail.contains(marker)) {
        return ClaimVerdict::Conceded;
    }
    ClaimVerdict::Rejected
}

fn verdict_keyword_offset(line: &str) -> Option<usize> {
    [
        "PARTIALLY CONCEDING",
        "PARTIALLY CONCEDE",
        "PARTIAL CONCESSION",
        "CONCEDING",
        "CONCEDE",
        "CONCESSION",
        "REBUT",
        "REJECT",
        "DEFEND",
        "IN PART",
    ]
    .iter()
    .filter_map(|k| line.find(k))
    .min()
}

fn finalize_claim_verdicts(claims: &mut [Claim]) {
    for claim in claims.iter_mut() {
        if claim.verdict == ClaimVerdict::Open {
            if claim.attackers.is_empty() {
                claim.verdict = ClaimVerdict::Open;
            } else {
                claim.verdict = ClaimVerdict::Disputed;
            }
        }
    }
}

pub fn matches_query(claim: &Claim, query: &str) -> bool {
    format!(
        "{} {} {} {}",
        claim.id,
        claim.text,
        claim.author_model,
        claim.verdict.as_str()
    )
    .to_lowercase()
    .contains(&query.to_lowercase())
}

fn origin_counts(claims: &[Claim], before: usize) -> std::collections::BTreeMap<String, usize> {
    let mut counts = std::collections::BTreeMap::new();
    for claim in claims.iter().filter(|c| c.turn_index < before) {
        *counts.entry(claim.id.clone()).or_insert(0) += 1;
    }
    counts
}
