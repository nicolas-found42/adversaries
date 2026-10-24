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

fn collect_claims_from_turn(turn: &TurnState, claims: &mut Vec<Claim>) {
    for line in turn.text.lines() {
        if let Some((id, text)) = parse_claim_line(line) {
            if !claims.iter().any(|c| c.id == id) {
                claims.push(Claim {
                    id,
                    author_model: turn.model.clone(),
                    turn_index: turn.index,
                    text,
                    attackers: Vec::new(),
                    verdict: ClaimVerdict::Open,
                });
            }
        }
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
        .chars()
        .take(120)
        .collect::<String>()
        .trim_end_matches(['*', '_', '`'])
        .trim_end()
        .to_string()
}

fn link_attacks_from_turn(turn: &TurnState, claims: &mut [Claim]) {
    if !turn.role.to_lowercase().contains("red") && !turn.role.to_lowercase().contains("attack") {
        return;
    }
    for claim in claims.iter_mut() {
        if claim.turn_index != turn.index && line_references_claim(&turn.text, &claim.id) {
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
    for claim in claims.iter_mut() {
        let id_upper = claim.id.to_uppercase();
        if let Some(verdict) = detect_verdict_for_claim(&upper, &id_upper) {
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
