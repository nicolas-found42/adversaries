//! Time-driven simulated debate shared by the native and browser demo modes.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;
use web_time::Instant;

use crate::model::BackendEvent;

struct MockStep {
    index: usize,
    model: &'static str,
    role: &'static str,
    text: &'static str,
    seconds: f64,
}

/// Replays the mock ladder on wall-clock time; `poll` is side-effect free
/// apart from advancing the event cursor, so any driver cadence works.
pub struct MockStream {
    question: String,
    steps: Vec<MockStep>,
    started: Instant,
    step_delay: Duration,
    /// 0 = Start pending, 1..=2n step events, 2n+1 = Done pending, above = finished.
    emitted: usize,
}

const EVENTS_PER_STEP: usize = 2;

impl MockStream {
    pub fn new(question: String, fast: bool) -> Self {
        let delay = if fast { 10 } else { 120 };
        Self {
            question,
            steps: mock_step_data(),
            started: Instant::now(),
            step_delay: Duration::from_millis(delay),
            emitted: 0,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.emitted > self.steps.len() * EVENTS_PER_STEP + 1
    }

    pub fn poll(&mut self, cancelled: &AtomicBool) -> Vec<BackendEvent> {
        if cancelled.load(Ordering::SeqCst) {
            self.emitted = self.steps.len() * EVENTS_PER_STEP + 2;
            return Vec::new();
        }
        let mut events = Vec::new();
        while let Some(event) = self.next_due_event() {
            events.push(event);
        }
        events
    }

    fn next_due_event(&mut self) -> Option<BackendEvent> {
        if self.emitted == 0 {
            self.emitted += 1;
            return Some(self.start_event());
        }
        if self.emitted <= self.steps.len() * EVENTS_PER_STEP {
            return self.next_step_event();
        }
        self.next_done_event()
    }

    fn start_event(&self) -> BackendEvent {
        BackendEvent::Start {
            question: self.question.clone(),
            rounds: 1,
            providers: ["Mistral", "OpenRouter", "Ollama-Cloud", "OpenCode-Go"]
                .iter()
                .map(|model| (*model).to_string())
                .collect(),
            total_calls: 6,
        }
    }

    fn next_step_event(&mut self) -> Option<BackendEvent> {
        let offset = self.emitted - 1;
        let step = &self.steps[offset / EVENTS_PER_STEP];
        let is_start = offset.is_multiple_of(EVENTS_PER_STEP);
        let slots = offset / EVENTS_PER_STEP + 1 + usize::from(!is_start);
        if self.started.elapsed() < self.step_delay * slots as u32 {
            return None;
        }
        self.emitted += 1;
        Some(build_step_event(step, is_start))
    }

    fn next_done_event(&mut self) -> Option<BackendEvent> {
        let slots = self.steps.len() + 2;
        if self.emitted == self.steps.len() * EVENTS_PER_STEP + 1
            && self.started.elapsed() >= self.step_delay * slots as u32
        {
            self.emitted += 1;
            return Some(BackendEvent::Done {
                report_path: "reports/demo_session.md".to_string(),
                html_path: Some("reports/demo_session.html".to_string()),
            });
        }
        None
    }
}

fn build_step_event(step: &MockStep, is_start: bool) -> BackendEvent {
    let index = step.index;
    let model = step.model.to_string();
    let role = step.role.to_string();
    if is_start && step.role != "Brief" {
        BackendEvent::TurnStart { index, model, role }
    } else if is_start {
        BackendEvent::BriefStart { index, model, role }
    } else if step.role != "Brief" {
        BackendEvent::TurnComplete {
            index,
            model,
            role,
            text: step.text.to_string(),
            seconds: step.seconds,
            diagnostics: serde_json::Value::Null,
        }
    } else {
        BackendEvent::BriefComplete {
            index,
            model,
            role,
            text: step.text.to_string(),
            seconds: step.seconds,
            diagnostics: serde_json::Value::Null,
        }
    }
}

/// Native driver: block on the mock clock inside a background thread.
pub fn drive_in_thread(stream: &mut MockStream, tx: &Sender<BackendEvent>, cancelled: &AtomicBool) {
    loop {
        for event in stream.poll(cancelled) {
            if tx.send(event).is_err() {
                return;
            }
        }
        if stream.is_finished() {
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn mock_step_data() -> Vec<MockStep> {
    vec![
        MockStep {
            index: 1,
            model: "Mistral",
            role: "Opening",
            text: OPENING_TEXT,
            seconds: 3.4,
        },
        MockStep {
            index: 2,
            model: "OpenRouter",
            role: "Red team",
            text: ATTACK_ONE_TEXT,
            seconds: 2.9,
        },
        MockStep {
            index: 3,
            model: "Ollama-Cloud",
            role: "Red team",
            text: ATTACK_TWO_TEXT,
            seconds: 3.1,
        },
        MockStep {
            index: 4,
            model: "OpenCode-Go",
            role: "Red team",
            text: ATTACK_THREE_TEXT,
            seconds: 2.7,
        },
        MockStep {
            index: 5,
            model: "Mistral",
            role: "Rebuttal",
            text: REBUTTAL_TEXT,
            seconds: 3.8,
        },
        MockStep {
            index: 6,
            model: "Mistral",
            role: "Brief",
            text: BRIEF_TEXT,
            seconds: 4.6,
        },
    ]
}

const OPENING_TEXT: &str = "C1: Expanding the engineering hub in Nairobi offers strong senior talent at 60% lower burn rate than London.\nC2: Timezone alignment with European operations is +1h to +2h, enabling smooth real-time collaboration.\nC3: Local compliance and tax registration require 90 days lead time and $25k initial setup.\nC4: Recommendation: establish the primary engineering center in Nairobi by Q2.";

const ATTACK_ONE_TEXT: &str = "Attacking C1: The 60% burn discount ignores high turnover base rates for specialized systems engineers.\nAttacking C3: 90 days lead time is overly optimistic; bureaucratic delays in local entity formation typically average 180 days.\nR1: Senior technical leadership must remain anchored in London for investor and customer access.";

const ATTACK_TWO_TEXT: &str = "Attacking R1: Remote executive leadership is already proven across 14 comparable growth-stage teams.\nAttacking C2: Power reliability and connectivity require private redundancy generators and dual fiber links ($15k capex).\nR2: Hybrid hub model mitigates single-region regulatory risk.";

const ATTACK_THREE_TEXT: &str = "Attacking C1 & R2: IP assignment across dual jurisdictions introduces cross-border tax withholding risks.\nR3: Pre-mortem: the initiative fails if entity registration stalls past Q3 budget cycle.";

const REBUTTAL_TEXT: &str = "Conceding R1 in part: keep two senior leads in London through the first two quarters.\nConceding C3 as revised: entity formation is now planned against a 180-day timeline, not 90 days.\nRebutting R3: the pre-mortem trigger becomes a tripwire — registration must clear by end of Q3 or the plan reverts to remote-first.\nDefending C1: the turnover-adjusted discount still favors Nairobi, at 50% rather than 60%.\nPartially conceding C2: the $15k power-redundancy capex is accepted and priced into the Q1 budget.\nDefending R2: the hybrid hub model stands.\nUpdated recommendation: Nairobi primary hub, Q2 start, with the London anchor and the Q3 registration tripwire attached.";

const BRIEF_TEXT: &str = "# Executive Decision Brief\n\n## Recommendation\nEstablish the primary engineering center in Nairobi by Q2, keeping two senior leads in London through the first two quarters.\n\n## Confidence\n72% — moderate-high.\n\n## What survived the attacks\n- C1, revised: talent cost advantage persists after turnover adjustment (50% discount, not 60%).\n- R2: hybrid hub model reduces single-region regulatory exposure.\n\n## What did not\n- The 90-day entity-formation estimate: rejected; plan against 180 days.\n\n## Tripwires\n- Registration not cleared by end of Q3 → revert to remote-first.\n- Attrition above 15% annualized in the Nairobi hub → reopen the London-vs-Nairobi balance.\n\n## Decide next\n- Confirm the 180-day registration clock with local counsel.\n- Price the $15k power-redundancy capex into the Q1 budget.";
