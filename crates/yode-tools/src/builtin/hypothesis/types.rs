use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(super) enum HypothesisStatus {
    Pending,
    Verified,
    Refuted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(super) enum FindingType {
    Bug,
    Risk,
    Optimization,
    DesignChoice,
}

impl FindingType {
    pub(super) fn label(&self) -> &str {
        match self {
            FindingType::Bug => "BUG",
            FindingType::Risk => "RISK",
            FindingType::Optimization => "OPTIMIZATION",
            FindingType::DesignChoice => "DESIGN_CHOICE",
        }
    }

    pub(super) fn icon(&self) -> &str {
        match self {
            FindingType::Bug => "🔴",
            FindingType::Risk => "⚠️",
            FindingType::Optimization => "💡",
            FindingType::DesignChoice => "📐",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(super) enum Confidence {
    High,
    Medium,
    Low,
}

impl Confidence {
    pub(super) fn label(&self) -> &str {
        match self {
            Confidence::High => "HIGH",
            Confidence::Medium => "MEDIUM",
            Confidence::Low => "LOW",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Hypothesis {
    pub(super) id: String,
    pub(super) hypothesis: String,
    pub(super) evidence_needed: String,
    pub(super) finding_type: FindingType,
    pub(super) status: HypothesisStatus,
    pub(super) evidence: Option<String>,
    pub(super) confidence: Option<Confidence>,
}

pub(super) fn parse_finding_type(input: &str) -> FindingType {
    match input.to_uppercase().as_str() {
        "BUG" => FindingType::Bug,
        "RISK" => FindingType::Risk,
        "OPTIMIZATION" => FindingType::Optimization,
        "DESIGN_CHOICE" => FindingType::DesignChoice,
        _ => FindingType::Risk,
    }
}

pub(super) fn parse_confidence(input: &str) -> Confidence {
    match input.to_uppercase().as_str() {
        "HIGH" => Confidence::High,
        "LOW" => Confidence::Low,
        _ => Confidence::Medium,
    }
}
