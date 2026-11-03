use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tab {
    Debate = 0,
    Brief = 1,
    Claims = 2,
    Sessions = 3,
    Help = 4,
    Diagnostics = 5,
}

impl Tab {
    pub const ALL: [Tab; 6] = [
        Tab::Debate,
        Tab::Brief,
        Tab::Claims,
        Tab::Sessions,
        Tab::Help,
        Tab::Diagnostics,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Debate => "Arguments",
            Self::Brief => "Decision brief",
            Self::Claims => "Claims & sources",
            Self::Sessions => "All decisions",
            Self::Help => "How this works",
            Self::Diagnostics => "Diagnostics",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    Queued,
    Running,
    Complete,
    Failed,
    Incomplete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnState {
    pub index: usize,
    pub model: String,
    pub role: String,
    pub text: String,
    pub seconds: f64,
    #[serde(default)]
    pub diagnostics: serde_json::Value,
    #[serde(default = "complete_status")]
    pub status: TurnStatus,
}

fn complete_status() -> TurnStatus {
    TurnStatus::Complete
}

impl TurnState {
    pub fn new(index: usize, model: String, role: String) -> Self {
        Self {
            index,
            model,
            role,
            text: String::new(),
            seconds: 0.0,
            diagnostics: serde_json::Value::Null,
            status: TurnStatus::Queued,
        }
    }

    pub fn word_count(&self) -> usize {
        self.text.split_whitespace().count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimVerdict {
    Open,
    Survived,
    Conceded,
    PartiallyConceded,
    Rejected,
    Disputed,
}

impl ClaimVerdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "UNCHALLENGED",
            Self::Survived => "UNCHALLENGED",
            Self::Conceded => "CONCEDED",
            Self::PartiallyConceded => "PARTIAL",
            Self::Rejected => "ATTACK REBUTTED",
            Self::Disputed => "DISPUTED",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub id: String,
    pub author_model: String,
    pub turn_index: usize,
    pub text: String,
    #[serde(default)]
    pub source_start: usize,
    #[serde(default)]
    pub source_end: usize,
    #[serde(default)]
    pub ambiguous_references: Vec<usize>,
    pub attackers: Vec<String>,
    pub verdict: ClaimVerdict,
}

impl Claim {
    pub fn origin_key(&self) -> String {
        format!("{}:{}:{}", self.turn_index, self.id, self.source_start)
    }
}

/// A recorded unfinished stage, not a failure of the act of opening a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageInterruption {
    pub stage_index: usize,
    pub role: String,
    pub model: String,
    pub reason: Option<String>,
}

impl StageInterruption {
    pub fn from_turns(turns: &[TurnState]) -> Option<Self> {
        turns
            .iter()
            .rev()
            .find(|turn| matches!(turn.status, TurnStatus::Failed | TurnStatus::Incomplete))
            .map(|turn| Self {
                stage_index: turn.index,
                role: turn.role.clone(),
                model: turn.model.clone(),
                reason: turn.diagnostics["message"]
                    .as_str()
                    .filter(|reason| !reason.trim().is_empty())
                    .map(str::to_owned),
            })
    }

    pub fn recorded_reason(&self) -> &str {
        self.reason.as_deref().unwrap_or(
            "No failure reason was recorded. This is a saved snapshot, not live progress.",
        )
    }

    pub fn description(&self) -> String {
        format!(
            "Saved stage {} · {} · {} has incomplete output. {}",
            self.stage_index,
            self.role,
            self.model,
            self.recorded_reason()
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub file_name: String,
    pub path_str: String,
    pub created_at: String,
    pub question: String,
    pub rounds: u32,
    pub turn_count: usize,
    pub has_brief: bool,
    #[serde(default)]
    pub interruption: Option<StageInterruption>,
    #[serde(default)]
    pub dry_run: Option<bool>,
}

/// A fully loaded past session, ready for replay in the debate viewer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDetail {
    pub question: String,
    pub brief: String,
    pub turns: Vec<TurnState>,
    #[serde(default)]
    pub run_config: Option<crate::transport::RunConfig>,
    #[serde(default)]
    pub diagnostics: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BackendEvent {
    Start {
        question: String,
        rounds: u32,
        providers: Vec<String>,
        total_calls: usize,
    },
    TurnStart {
        index: usize,
        model: String,
        role: String,
    },
    TurnComplete {
        index: usize,
        model: String,
        role: String,
        text: String,
        seconds: f64,
        #[serde(default)]
        diagnostics: serde_json::Value,
    },
    BriefStart {
        index: usize,
        model: String,
        role: String,
    },
    BriefComplete {
        index: usize,
        model: String,
        role: String,
        text: String,
        seconds: f64,
        #[serde(default)]
        diagnostics: serde_json::Value,
    },
    TurnFailed {
        #[serde(flatten)]
        turn: TurnState,
    },
    Diagnostics {
        diagnostics: serde_json::Value,
    },
    CheckpointSaved {
        report_path: String,
        completed_stages: usize,
    },
    Done {
        report_path: String,
        html_path: Option<String>,
    },
    Error {
        message: String,
    },
}
