/// Interactive wizard for multi-step command flows.

use std::collections::HashMap;

/// A single step in an interactive wizard.
pub struct WizardStep {
    /// Prompt to display to the user
    pub prompt: String,
    /// Default value (press Enter to accept)
    pub default: Option<String>,
    /// Allowed values (empty = freeform)
    pub options: Vec<String>,
    /// Key to store the answer under
    pub key: String,
}

/// Callback type for when a wizard completes.
/// Takes the collected answers and returns messages to display.
pub type WizardCallback = Box<dyn FnOnce(&HashMap<String, String>) -> Result<Vec<String>, String> + Send>;

/// Interactive wizard state.
pub struct Wizard {
    /// Title shown at the start
    pub title: String,
    /// Steps to complete
    steps: Vec<WizardStep>,
    /// Current step index
    current: usize,
    /// Collected answers so far
    pub answers: HashMap<String, String>,
    /// Callback to execute when all steps are done
    on_complete: Option<WizardCallback>,
}

impl Wizard {
    pub fn new(title: String, steps: Vec<WizardStep>, on_complete: WizardCallback) -> Self {
        Self {
            title,
            steps,
            current: 0,
            answers: HashMap::new(),
            on_complete: Some(on_complete),
        }
    }

    /// Get the current step, or None if wizard is done.
    pub fn current_step(&self) -> Option<&WizardStep> {
        self.steps.get(self.current)
    }

    /// Format the current prompt for display.
    pub fn prompt_text(&self) -> Option<String> {
        let step = self.current_step()?;
        let step_num = self.current + 1;
        let total = self.steps.len();

        let mut text = format!("[{}/{}] {}", step_num, total, step.prompt);

        if !step.options.is_empty() {
            text.push_str(&format!(" ({})", step.options.join("/")));
        }
        if let Some(ref default) = step.default {
            text.push_str(&format!(" [default: {}]", default));
        }

        Some(text)
    }

    /// Submit an answer for the current step.
    /// Returns Ok(None) if more steps remain, Ok(Some(messages)) if wizard is complete,
    /// or Err if the input is invalid.
    pub fn submit(&mut self, input: &str) -> Result<Option<Vec<String>>, String> {
        let step = self.steps.get(self.current)
            .ok_or_else(|| "Wizard already complete".to_string())?;

        let value = if input.trim().is_empty() {
            match &step.default {
                Some(d) => d.clone(),
                None => return Err(format!("This field is required. {}", step.prompt)),
            }
        } else {
            input.trim().to_string()
        };

        // Validate against options if specified
        if !step.options.is_empty() && !step.options.iter().any(|o| o == &value) {
            return Err(format!("Invalid value '{}'. Options: {}", value, step.options.join(", ")));
        }

        self.answers.insert(step.key.clone(), value);
        self.current += 1;

        if self.current >= self.steps.len() {
            // All steps done, run callback
            if let Some(callback) = self.on_complete.take() {
                let result = callback(&self.answers)?;
                Ok(Some(result))
            } else {
                Ok(Some(vec!["Wizard complete.".into()]))
            }
        } else {
            Ok(None)
        }
    }

    /// Check if the wizard is still active.
    pub fn is_active(&self) -> bool {
        self.current < self.steps.len()
    }
}
