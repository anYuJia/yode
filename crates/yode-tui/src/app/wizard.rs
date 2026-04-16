/// Interactive wizard for multi-step command flows.
/// Renders in the TUI viewport with selection and text input support.
use std::collections::HashMap;

/// A single step in an interactive wizard.
pub enum WizardStep {
    /// Select from a list of options (up/down, Enter to confirm)
    Select {
        prompt: String,
        options: Vec<String>,
        default: usize,
        key: String,
    },
    /// Text input (type text, Enter to confirm)
    Input {
        prompt: String,
        default: Option<String>,
        key: String,
    },
}

impl WizardStep {
    pub fn key(&self) -> &str {
        match self {
            Self::Select { key, .. } => key,
            Self::Input { key, .. } => key,
        }
    }

    pub fn prompt(&self) -> &str {
        match self {
            Self::Select { prompt, .. } => prompt,
            Self::Input { prompt, .. } => prompt,
        }
    }
}

pub struct WizardCompletion {
    pub messages: Vec<String>,
    pub apply_provider: Option<String>,
    pub apply_model: Option<String>,
    pub next_wizard: Option<Box<Wizard>>,
}

impl WizardCompletion {
    pub fn messages(messages: Vec<String>) -> Self {
        Self {
            messages,
            apply_provider: None,
            apply_model: None,
            next_wizard: None,
        }
    }

    pub fn apply_provider(messages: Vec<String>, provider: impl Into<String>) -> Self {
        Self {
            messages,
            apply_provider: Some(provider.into()),
            apply_model: None,
            next_wizard: None,
        }
    }

    pub fn apply_model(messages: Vec<String>, model: impl Into<String>) -> Self {
        Self {
            messages,
            apply_provider: None,
            apply_model: Some(model.into()),
            next_wizard: None,
        }
    }

    pub fn apply_provider_and_model(
        messages: Vec<String>,
        provider: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            messages,
            apply_provider: Some(provider.into()),
            apply_model: Some(model.into()),
            next_wizard: None,
        }
    }

    pub fn next(wizard: Wizard) -> Self {
        Self {
            messages: Vec::new(),
            apply_provider: None,
            apply_model: None,
            next_wizard: Some(Box::new(wizard)),
        }
    }
}

/// Callback type for when a wizard completes.
pub type WizardCallback =
    Box<dyn FnOnce(&HashMap<String, String>) -> Result<WizardCompletion, String> + Send>;

/// Callback type for when a step completes — can modify subsequent steps' defaults.
pub type StepCallback = Box<dyn Fn(&str, &mut Vec<WizardStep>) + Send>;

/// Interactive wizard state.
pub struct Wizard {
    /// Title shown at the top
    pub title: String,
    /// Steps to complete
    pub steps: Vec<WizardStep>,
    /// Current step index
    pub current: usize,
    /// For Select steps: currently highlighted option
    pub select_index: usize,
    /// For Input steps: user's typed text
    pub input_buf: String,
    /// Collected answers so far
    pub answers: HashMap<String, String>,
    /// Callback to execute when all steps are done
    on_complete: Option<WizardCallback>,
    /// Called after each step to update subsequent steps' defaults
    on_step: Option<StepCallback>,
    /// Provider name to hot-reload after wizard completes (set by provider edit/add)
    pub reload_provider: Option<String>,
    /// Provider to apply immediately after wizard completion.
    pub apply_provider: Option<String>,
    /// Model to apply immediately after wizard completion.
    pub apply_model: Option<String>,
    /// Wizard to replace the current one after completion.
    pub next_wizard: Option<Box<Wizard>>,
    /// Error message to display (from validation)
    pub error: Option<String>,
}

impl Wizard {
    pub fn new(title: String, steps: Vec<WizardStep>, on_complete: WizardCallback) -> Self {
        let select_index = match steps.first() {
            Some(WizardStep::Select { default, .. }) => *default,
            _ => 0,
        };
        Self {
            title,
            steps,
            current: 0,
            select_index,
            input_buf: String::new(),
            answers: HashMap::new(),
            on_complete: Some(on_complete),
            on_step: None,
            reload_provider: None,
            apply_provider: None,
            apply_model: None,
            next_wizard: None,
            error: None,
        }
    }

    /// Set a callback that fires after each step, allowing dynamic default updates.
    pub fn with_step_callback(mut self, cb: StepCallback) -> Self {
        self.on_step = Some(cb);
        self
    }

    /// Set a provider name to hot-reload after wizard completes.
    pub fn with_reload_provider(mut self, name: String) -> Self {
        self.reload_provider = Some(name);
        self
    }

    pub fn is_active(&self) -> bool {
        self.current < self.steps.len()
    }

    pub fn current_step(&self) -> Option<&WizardStep> {
        self.steps.get(self.current)
    }

    pub fn step_label(&self) -> String {
        format!("[{}/{}]", self.current + 1, self.steps.len())
    }

    /// Move selection up (for Select steps), wraps to bottom
    pub fn select_up(&mut self) {
        if let Some(WizardStep::Select { options, .. }) = self.current_step() {
            let len = options.len();
            if len == 0 {
                return;
            }
            if self.select_index == 0 {
                self.select_index = len - 1;
            } else {
                self.select_index -= 1;
            }
        }
    }

    /// Move selection down (for Select steps), wraps to top
    pub fn select_down(&mut self) {
        if let Some(WizardStep::Select { options, .. }) = self.current_step() {
            let len = options.len();
            if len == 0 {
                return;
            }
            if self.select_index >= len - 1 {
                self.select_index = 0;
            } else {
                self.select_index += 1;
            }
        }
    }

    /// Handle character input (for Input steps)
    pub fn input_char(&mut self, c: char) {
        self.input_buf.push(c);
        self.error = None;
    }

    /// Handle backspace (for Input steps)
    pub fn input_backspace(&mut self) {
        self.input_buf.pop();
    }

    /// Submit the current step. Returns Ok(None) if more steps, Ok(Some(msgs)) if done.
    pub fn submit(&mut self) -> Result<Option<Vec<String>>, String> {
        let step = self
            .steps
            .get(self.current)
            .ok_or("Wizard already complete")?;

        let value = match step {
            WizardStep::Select { options, .. } => options[self.select_index].clone(),
            WizardStep::Input { default, .. } => {
                let v = self.input_buf.trim().to_string();
                if v.is_empty() {
                    match default {
                        Some(d) => d.clone(),
                        None => {
                            self.error = Some("This field is required.".into());
                            return Ok(None);
                        }
                    }
                } else {
                    v
                }
            }
        };

        let key = step.key().to_string();
        self.answers.insert(key.clone(), value.clone());
        self.current += 1;

        // Call step callback to update subsequent steps' defaults
        if let Some(ref cb) = self.on_step {
            cb(&value, &mut self.steps);
        }

        // Prepare next step state
        if let Some(next) = self.steps.get(self.current) {
            match next {
                WizardStep::Select { default, .. } => {
                    self.select_index = *default;
                    self.input_buf.clear();
                }
                WizardStep::Input { .. } => {
                    self.select_index = 0;
                    self.input_buf.clear(); // Keep empty — default shown as placeholder
                }
            }
        }
        self.error = None;

        if self.current >= self.steps.len() {
            // Auto-set reload_provider from answers if not explicitly set
            if self.reload_provider.is_none() {
                if let Some(name) = self.answers.get("name") {
                    self.reload_provider = Some(name.clone());
                }
            }
            if let Some(callback) = self.on_complete.take() {
                let result = callback(&self.answers)?;
                self.apply_provider = result.apply_provider;
                self.apply_model = result.apply_model;
                self.next_wizard = result.next_wizard;
                Ok(Some(result.messages))
            } else {
                Ok(Some(vec!["Done.".into()]))
            }
        } else {
            Ok(None)
        }
    }

    /// Total lines needed for rendering this wizard in the viewport.
    pub fn viewport_height(&self) -> u16 {
        let step = match self.current_step() {
            Some(s) => s,
            None => return 2,
        };
        let base = 2; // title + prompt line
        let error = if self.error.is_some() { 1 } else { 0 };
        match step {
            WizardStep::Select { options, .. } => base + options.len() as u16 + error,
            WizardStep::Input { .. } => base + 1 + error, // +1 for input line
        }
    }
}
