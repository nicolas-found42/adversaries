use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::model::{SessionDetail, SessionSummary, TurnState, TurnStatus};

#[derive(Debug, Deserialize)]
struct SidecarJson {
    question: Option<String>,
    rounds: Option<u32>,
    created_at: Option<String>,
    brief: Option<String>,
    turns: Option<Vec<SidecarTurn>>,
    run_config: Option<crate::transport::RunConfig>,
    brief_turn: Option<SidecarTurn>,
    failed_turn: Option<SidecarTurn>,
    diagnostics: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct SidecarTurn {
    index: Option<usize>,
    model: Option<String>,
    role: Option<String>,
    text: Option<String>,
    seconds: Option<f64>,
    status: Option<TurnStatus>,
    diagnostics: Option<serde_json::Value>,
}

pub fn scan_sessions(reports_dir: &Path) -> Vec<SessionSummary> {
    let mut sessions = Vec::new();
    let Ok(entries) = fs::read_dir(reports_dir) else {
        return sessions;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("md") {
            if let Some(summary) = summarize_report(&path) {
                sessions.push(summary);
            }
        }
    }
    sessions.sort_by(|a, b| b.file_name.cmp(&a.file_name));
    sessions
}

fn summarize_report(md_path: &Path) -> Option<SessionSummary> {
    let stem = md_path.file_stem()?.to_string_lossy().to_string();
    let file_name = md_path.file_name()?.to_string_lossy().to_string();
    let json_path = md_path.with_extension("json");
    if json_path.is_file() {
        if let Some(s) = summarize_from_json(&json_path, &file_name) {
            return Some(s);
        }
    }
    summarize_from_md(md_path, &file_name, &stem)
}

fn summarize_from_json(json_path: &Path, file_name: &str) -> Option<SessionSummary> {
    let text = fs::read_to_string(json_path).ok()?;
    let parsed: SidecarJson = serde_json::from_str(&text).ok()?;
    let turns = parsed.turns.unwrap_or_default();
    Some(SessionSummary {
        file_name: file_name.to_string(),
        path_str: json_path.to_string_lossy().to_string(),
        created_at: parsed.created_at.unwrap_or_else(|| "Unknown".to_string()),
        question: parsed.question.unwrap_or_else(|| file_name.to_string()),
        rounds: parsed.rounds.unwrap_or(1),
        turn_count: turns.len() + usize::from(parsed.brief.as_ref().is_some_and(|b| !b.is_empty())),
        has_brief: parsed.brief.as_ref().is_some_and(|b| !b.is_empty()),
    })
}

fn summarize_from_md(md_path: &Path, file_name: &str, stem: &str) -> Option<SessionSummary> {
    let text = fs::read_to_string(md_path).ok()?;
    let question = extract_question_from_md(&text).unwrap_or_else(|| stem.to_string());
    let turn_count = text.matches("\n## Turn ").count();
    Some(SessionSummary {
        file_name: file_name.to_string(),
        path_str: md_path.to_string_lossy().to_string(),
        created_at: stem.get(..15).unwrap_or("Unknown").to_string(),
        question,
        rounds: 1,
        turn_count,
        has_brief: text.contains("# Decision Brief") || text.contains("Decision Brief:"),
    })
}

fn extract_question_from_md(text: &str) -> Option<String> {
    for line in text.lines() {
        if let Some(q) = line.strip_prefix("**Question:** ") {
            return Some(q.trim().to_string());
        }
    }
    None
}

pub fn load_session(path_str: &str) -> Option<SessionDetail> {
    let path = PathBuf::from(path_str);
    let json_path = if path.extension().and_then(|e| e.to_str()) == Some("json") {
        path.clone()
    } else {
        path.with_extension("json")
    };
    if json_path.is_file() {
        if let Some(res) = load_from_json(&json_path) {
            return Some(res);
        }
    }
    let md_path = path.with_extension("md");
    if md_path.is_file() {
        return load_from_md(&md_path);
    }
    None
}

fn load_from_json(path: &Path) -> Option<SessionDetail> {
    let content = fs::read_to_string(path).ok()?;
    let data: SidecarJson = serde_json::from_str(&content).ok()?;
    let question = data.question.unwrap_or_default();
    let brief = data.brief.unwrap_or_default();
    let mut turns = Vec::new();
    for (i, t) in data.turns.unwrap_or_default().into_iter().enumerate() {
        turns.push(TurnState {
            index: t.index.unwrap_or(i + 1),
            model: t.model.unwrap_or_else(|| "Unknown".to_string()),
            role: t.role.unwrap_or_else(|| "Debater".to_string()),
            text: t.text.unwrap_or_default(),
            seconds: t.seconds.unwrap_or(0.0),
            status: t.status.unwrap_or(TurnStatus::Complete),
            diagnostics: t.diagnostics.unwrap_or_default(),
        });
    }
    if !brief.is_empty() {
        turns.push(restore_brief(
            data.brief_turn,
            data.run_config.as_ref(),
            &brief,
            turns.len() + 1,
        ));
    }
    if let Some(failed) = data.failed_turn {
        turns.push(restore_failed(failed, turns.len() + 1));
    }
    Some(SessionDetail {
        question,
        brief,
        turns,
        diagnostics: refresh_diagnostics(path, data.diagnostics),
        run_config: data.run_config,
    })
}

fn restore_brief(
    saved: Option<SidecarTurn>,
    config: Option<&crate::transport::RunConfig>,
    brief: &str,
    index: usize,
) -> TurnState {
    let model = config.and_then(|c| c.roles.get("brief")).map_or_else(
        || "Council".into(),
        |r| format!("{} / {}", r.provider, r.model),
    );
    let seconds = saved.as_ref().and_then(|t| t.seconds).unwrap_or_default();
    TurnState {
        index,
        model,
        role: "Brief".into(),
        text: brief.into(),
        seconds,
        diagnostics: saved.and_then(|t| t.diagnostics).unwrap_or_default(),
        status: TurnStatus::Complete,
    }
}

fn load_from_md(path: &Path) -> Option<SessionDetail> {
    let content = fs::read_to_string(path).ok()?;
    let question = extract_question_from_md(&content).unwrap_or_default();
    let (brief, turns_text) = split_brief_and_turns(&content);
    let turns = parse_turns_from_text(&turns_text);
    Some(SessionDetail {
        question,
        brief,
        turns,
        run_config: None,
        diagnostics: serde_json::Value::Null,
    })
}

fn split_brief_and_turns(content: &str) -> (String, String) {
    if let Some((brief_part, rest)) = content.split_once("# Appendix: Full adversarial debate") {
        (brief_part.trim().to_string(), rest.to_string())
    } else {
        (content.trim().to_string(), String::new())
    }
}

fn parse_turns_from_text(turns_text: &str) -> Vec<TurnState> {
    let mut turns = Vec::new();
    let mut current_header = None;
    let mut current_body = Vec::new();

    for line in turns_text.lines() {
        if let Some(header) = line.strip_prefix("## Turn ") {
            if let Some(h) = current_header.take() {
                push_parsed_turn(&mut turns, h, &current_body);
                current_body.clear();
            }
            current_header = Some(header.to_string());
        } else if current_header.is_some() {
            current_body.push(line);
        }
    }
    if let Some(h) = current_header {
        push_parsed_turn(&mut turns, h, &current_body);
    }
    turns
}

fn push_parsed_turn(turns: &mut Vec<TurnState>, header: String, body: &[&str]) {
    let (model, role) = parse_model_and_role(&header);
    let index = turns.len() + 1;
    let text = body.join("\n").trim().to_string();
    turns.push(TurnState {
        index,
        model,
        role,
        text,
        seconds: 0.0,
        diagnostics: serde_json::Value::Null,
        status: TurnStatus::Complete,
    });
}

fn parse_model_and_role(header: &str) -> (String, String) {
    let after_num = header.split_once('—').map_or(header, |x| x.1).trim();
    if let Some((model, role)) = after_num.split_once(':') {
        (model.trim().to_string(), role.trim().to_string())
    } else {
        (after_num.to_string(), "Debater".to_string())
    }
}

fn restore_failed(t: SidecarTurn, index: usize) -> TurnState {
    TurnState {
        index: t.index.unwrap_or(index),
        model: t.model.unwrap_or_default(),
        role: t.role.unwrap_or_default(),
        text: t.text.unwrap_or_default(),
        seconds: t.seconds.unwrap_or_default(),
        status: t.status.unwrap_or(TurnStatus::Failed),
        diagnostics: t.diagnostics.unwrap_or_default(),
    }
}

fn refresh_diagnostics(path: &Path, saved: Option<serde_json::Value>) -> serde_json::Value {
    let mut diagnostics = saved.unwrap_or_default();
    if let Ok(text) = fs::read_to_string(path.with_extension("events.jsonl")) {
        let events: Vec<serde_json::Value> = text
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        if diagnostics.is_object() {
            diagnostics["events"] = serde_json::json!(events);
        }
    }
    attach_transport_events(path, &mut diagnostics);
    diagnostics
}

fn attach_transport_events(path: &Path, diagnostics: &mut serde_json::Value) {
    let Some(pid) = diagnostics["events"]
        .as_array()
        .and_then(|e| e.first())
        .and_then(|e| e["pid"].as_u64())
    else {
        return;
    };
    let start = diagnostics["events"][0]["time"].as_str().unwrap_or("");
    let text =
        fs::read_to_string(path.with_file_name("transport.events.jsonl")).unwrap_or_default();
    let events: Vec<serde_json::Value> = text
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|e| {
            e["pid"].as_u64() == Some(pid) && e["time"].as_str().is_some_and(|time| time >= start)
        })
        .collect();
    diagnostics["transport_events"] = serde_json::json!(events);
}
