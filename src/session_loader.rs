use std::fs;
use std::path::{Path, PathBuf};

use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use serde::Deserialize;
use std::ops::Range;

use crate::model::{SessionDetail, SessionSummary, StageInterruption, TurnState, TurnStatus};

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

#[derive(Debug, Default, Deserialize)]
struct SidecarTurn {
    index: Option<usize>,
    model: Option<String>,
    role: Option<String>,
    text: Option<String>,
    seconds: Option<f64>,
    status: Option<TurnStatus>,
    diagnostics: Option<serde_json::Value>,
}

/// Filesystem permissions and slow storage must not block the native UI thread.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn scan_sessions_async(
    reports_dir: PathBuf,
) -> std::sync::mpsc::Receiver<Result<Vec<SessionSummary>, String>> {
    read_on_worker(move || scan_sessions_checked(&reports_dir))
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn load_session_async(path: String) -> std::sync::mpsc::Receiver<Option<SessionDetail>> {
    read_on_worker(move || load_session(&path))
}

#[cfg(not(target_arch = "wasm32"))]
fn read_on_worker<T: Send + 'static>(
    read: impl FnOnce() -> T + Send + 'static,
) -> std::sync::mpsc::Receiver<T> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let _ = std::thread::Builder::new()
        .name("council-storage".into())
        .spawn(move || {
            let _ = sender.send(read());
        });
    receiver
}

pub fn scan_sessions(reports_dir: &Path) -> Vec<SessionSummary> {
    scan_sessions_checked(reports_dir).unwrap_or_default()
}

/// A missing history directory is an empty first launch; unreadable storage is an error.
pub fn scan_sessions_checked(reports_dir: &Path) -> Result<Vec<SessionSummary>, String> {
    let entries = match fs::read_dir(reports_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("Could not read saved decisions: {error}")),
    };
    let mut sessions = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|error| format!("Could not list saved decisions: {error}"))?
            .path();
        if let Some(summary) = summarize_candidate(&path) {
            sessions.push(summary);
        }
    }
    sessions.sort_by(|a, b| b.file_name.cmp(&a.file_name));
    Ok(sessions)
}

fn summarize_candidate(path: &Path) -> Option<SessionSummary> {
    match path.extension()?.to_str()? {
        "md" => summarize_report(path),
        "json" if !path.with_extension("md").is_file() => {
            summarize_from_json(path, path.file_name()?.to_str()?)
        }
        _ => None,
    }
}

fn saved_brief(data: &SidecarJson) -> Option<&str> {
    let status = data.brief_turn.as_ref().and_then(|turn| turn.status);
    if status.is_some_and(|status| status != TurnStatus::Complete) {
        return None;
    }
    data.brief
        .as_deref()
        .or_else(|| {
            data.brief_turn
                .as_ref()
                .and_then(|turn| turn.text.as_deref())
        })
        .filter(|text| crate::brief::is_available(text))
}

fn recorded_question(data: &SidecarJson) -> Option<&str> {
    data.question
        .as_deref()
        .filter(|text| !text.trim().is_empty())
        .or_else(|| {
            data.run_config
                .as_ref()
                .map(|config| config.question.as_str())
                .filter(|text| !text.trim().is_empty())
        })
}

fn question_label(question: Option<&str>, path: &Path) -> String {
    question.filter(|text| !text.trim().is_empty()).map_or_else(
        || {
            format!(
                "Question not recorded ({})",
                path.file_stem().unwrap_or_default().to_string_lossy()
            )
        },
        str::to_owned,
    )
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
    let mut parsed: SidecarJson = serde_json::from_str(&text).ok()?;
    if parsed.question.is_none() && parsed.turns.is_none() && parsed.brief.is_none() {
        return None;
    }
    let question = question_label(recorded_question(&parsed), json_path);
    let (brief, turns) = restore_content(&mut parsed);
    let has_brief = !brief.is_empty();
    let turn_count = turns
        .iter()
        .filter(|turn| turn.status == TurnStatus::Complete)
        .count();
    let interruption = StageInterruption::from_turns(&turns);
    Some(SessionSummary {
        file_name: file_name.to_string(),
        path_str: json_path.to_string_lossy().to_string(),
        created_at: parsed.created_at.unwrap_or_else(|| "Unknown".to_string()),
        question,
        rounds: parsed.rounds.unwrap_or(1),
        turn_count,
        has_brief,
        interruption,
        dry_run: parsed.run_config.as_ref().map(|c| c.dry_run),
    })
}

fn summarize_from_md(md_path: &Path, file_name: &str, stem: &str) -> Option<SessionSummary> {
    let text = fs::read_to_string(md_path).ok()?;
    let question = question_label(extract_question_from_md(&text).as_deref(), md_path);
    let (brief, turns) = split_brief_and_turns(&text);
    let turn_count = parse_turns_from_text(&turns).len();
    Some(SessionSummary {
        file_name: file_name.to_string(),
        path_str: md_path.to_string_lossy().to_string(),
        created_at: stem.get(..15).unwrap_or("Unknown").to_string(),
        question,
        rounds: 1,
        turn_count,
        has_brief: crate::brief::is_available(&brief),
        interruption: None,
        dry_run: None,
    })
}

fn extract_question_from_md(content: &str) -> Option<String> {
    let headings = report_headings(content);
    let text = if let Some(appendix) = find_heading(
        &headings,
        HeadingLevel::H1,
        "Appendix: Full adversarial debate",
    ) {
        &content[appendix.end..]
    } else {
        let end = find_heading(&headings, HeadingLevel::H1, "Decision Brief")
            .map_or(content.len(), |range| range.start);
        &content[..end]
    };
    let end = find_heading(&report_headings(text), HeadingLevel::H2, "Turn")
        .map_or(text.len(), |range| range.start);
    text[..end].lines().find_map(|line| {
        line.strip_prefix("**Question:** ")
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
    })
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
    let mut data: SidecarJson = serde_json::from_str(&content).ok()?;
    let question = recorded_question(&data).unwrap_or_default().to_owned();
    let (brief, turns) = restore_content(&mut data);
    Some(SessionDetail {
        question,
        brief,
        turns,
        diagnostics: refresh_diagnostics(path, data.diagnostics),
        run_config: data.run_config,
    })
}

/// Only a completed brief with body content is returned as `brief`; other
/// available output remains in its incomplete stage record.
fn restore_content(data: &mut SidecarJson) -> (String, Vec<TurnState>) {
    let brief = saved_brief(data).unwrap_or_default().to_owned();
    let raw_brief = data.brief.as_deref().unwrap_or_default();
    let mut turns = Vec::new();
    for (i, t) in data
        .turns
        .take()
        .unwrap_or_default()
        .into_iter()
        .enumerate()
    {
        turns.push(TurnState {
            index: t.index.unwrap_or(i + 1),
            model: t.model.unwrap_or_else(|| "Unknown".to_string()),
            role: t.role.unwrap_or_else(|| "Debater".to_string()),
            text: t.text.unwrap_or_default(),
            seconds: t.seconds.unwrap_or(0.0),
            status: match t.status.unwrap_or(TurnStatus::Complete) {
                TurnStatus::Running => TurnStatus::Incomplete,
                status => status,
            },
            diagnostics: t.diagnostics.unwrap_or_default(),
        });
    }
    if !brief.is_empty() {
        turns.push(restore_brief(
            data.brief_turn.take(),
            data.run_config.as_ref(),
            &brief,
            turns.len() + 1,
        ));
    } else if let Some(partial) =
        restore_unavailable_brief(data.brief_turn.take(), raw_brief, turns.len() + 1)
    {
        turns.push(partial);
    }
    if let Some(failed) = data.failed_turn.take() {
        turns.push(restore_failed(failed, turns.len() + 1));
    }
    (brief, turns)
}

fn restore_unavailable_brief(
    saved: Option<SidecarTurn>,
    raw: &str,
    index: usize,
) -> Option<TurnState> {
    let mut saved = saved.unwrap_or_default();
    let text = saved
        .text
        .take()
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| raw.into());
    if text.trim().is_empty() {
        return None;
    }
    saved.text = Some(text);
    saved.role.get_or_insert_with(|| "Brief".into());
    if saved
        .status
        .is_none_or(|status| status == TurnStatus::Complete)
    {
        saved.status = Some(TurnStatus::Incomplete);
    }
    Some(restore_failed(saved, index))
}

fn restore_brief(
    saved: Option<SidecarTurn>,
    config: Option<&crate::transport::RunConfig>,
    brief: &str,
    index: usize,
) -> TurnState {
    let mut saved = saved.unwrap_or_default();
    let model = saved.model.take().unwrap_or_else(|| {
        config.and_then(|c| c.roles.get("brief")).map_or_else(
            || "Model not recorded".into(),
            |r| format!("{} / {}", r.provider, r.model),
        )
    });
    TurnState {
        index: saved.index.unwrap_or(index),
        model,
        role: saved.role.unwrap_or_else(|| "Brief".into()),
        text: brief.into(),
        seconds: saved.seconds.unwrap_or_default(),
        diagnostics: saved.diagnostics.unwrap_or_default(),
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
    let headings = report_headings(content);
    if let Some(appendix) = find_heading(
        &headings,
        HeadingLevel::H1,
        "Appendix: Full adversarial debate",
    ) {
        let prefix = content[..appendix.start].trim_end();
        let candidate = prefix.strip_suffix("---").unwrap_or(prefix).trim();
        return (available_text(candidate), content[appendix.end..].into());
    }
    let turn_start = find_heading(&headings, HeadingLevel::H2, "Turn")
        .map_or(content.len(), |range| range.start);
    let brief = find_heading(&headings, HeadingLevel::H1, "Decision Brief")
        .filter(|range| range.start < turn_start)
        .map_or_else(String::new, |range| {
            available_text(content[range.start..turn_start].trim())
        });
    (brief, content[turn_start..].into())
}

fn available_text(text: &str) -> String {
    if crate::brief::is_available(text) {
        text.into()
    } else {
        String::new()
    }
}

type ReportHeading = (HeadingLevel, Range<usize>, String);

fn report_headings(content: &str) -> Vec<ReportHeading> {
    let mut headings = Vec::new();
    let mut active = None;
    for (event, range) in Parser::new(content).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                active = Some((level, range, String::new()));
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, _, title)) = &mut active {
                    title.push_str(&text);
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, mut bounds, title)) = active.take() {
                    bounds.end = range.end;
                    if content[..bounds.start]
                        .rsplit('\n')
                        .next()
                        .unwrap_or_default()
                        .trim()
                        .is_empty()
                    {
                        headings.push((level, bounds, title));
                    }
                }
            }
            _ => {}
        }
    }
    headings
}

fn find_heading<'a>(
    headings: &'a [ReportHeading],
    level: HeadingLevel,
    prefix: &str,
) -> Option<&'a Range<usize>> {
    headings
        .iter()
        .find(|(kind, _, title)| {
            *kind == level
                && (title == prefix
                    || title
                        .strip_prefix(prefix)
                        .is_some_and(|suffix| suffix.starts_with([' ', ':'])))
        })
        .map(|(_, range, _)| range)
}

fn parse_turns_from_text(turns_text: &str) -> Vec<TurnState> {
    let turns_text = turns_text.trim_end();
    let turns_text = turns_text
        .strip_suffix(
            "\n---\n*AI-generated decision support. Verify facts and figures before acting.*",
        )
        .unwrap_or(turns_text);
    let headings = report_headings(turns_text);
    let headers: Vec<_> = headings
        .iter()
        .filter(|(level, _, title)| *level == HeadingLevel::H2 && turn_index(title).is_some())
        .collect();
    headers
        .iter()
        .enumerate()
        .map(|(position, (_, range, title))| {
            let end = headers
                .get(position + 1)
                .map_or(turns_text.len(), |(_, range, _)| range.start);
            let header = title.strip_prefix("Turn ").unwrap_or(title);
            let (model, role) = parse_model_and_role(header);
            TurnState {
                index: turn_index(title).unwrap_or(position + 1),
                model,
                role,
                text: turns_text[range.end..end].trim().into(),
                seconds: 0.0,
                diagnostics: serde_json::Value::Null,
                status: TurnStatus::Complete,
            }
        })
        .collect()
}

fn turn_index(title: &str) -> Option<usize> {
    title
        .strip_prefix("Turn ")?
        .split(['—', ':'])
        .next()?
        .trim()
        .parse()
        .ok()
        .filter(|index| *index > 0)
}

fn parse_model_and_role(header: &str) -> (String, String) {
    if let Some((number, text)) = header.split_once('—')
        && number.trim().parse::<usize>().is_ok()
    {
        return attribution(text, ':');
    }
    if let Some((number, text)) = header.split_once(':')
        && number.trim().parse::<usize>().is_ok()
    {
        return attribution(text, '—');
    }
    (header.trim().into(), "Role not recorded".into())
}

fn attribution(text: &str, separator: char) -> (String, String) {
    text.rsplit_once(separator).map_or_else(
        || (text.trim().into(), "Role not recorded".into()),
        |(model, role)| (model.trim().into(), role.trim().into()),
    )
}

fn restore_failed(t: SidecarTurn, index: usize) -> TurnState {
    TurnState {
        index: t.index.unwrap_or(index),
        model: t.model.unwrap_or_default(),
        role: t.role.unwrap_or_default(),
        text: t.text.unwrap_or_default(),
        seconds: t.seconds.unwrap_or_default(),
        status: match t.status {
            Some(TurnStatus::Failed) => TurnStatus::Failed,
            _ => TurnStatus::Incomplete,
        },
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod worker_tests;
