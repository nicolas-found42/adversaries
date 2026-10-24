use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
            Self::Debate => "Debate Timeline",
            Self::Brief => "Decision Brief",
            Self::Claims => "Claim Ledger",
            Self::Sessions => "Session History",
            Self::Help => "Help & Keys",
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
    pub attackers: Vec<String>,
    pub verdict: ClaimVerdict,
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
    Done {
        report_path: String,
        html_path: Option<String>,
    },
    Error {
        message: String,
    },
}
