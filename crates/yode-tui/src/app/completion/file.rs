/// State for @file path completion.
pub struct FileCompletion {
    pub candidates: Vec<String>,
    pub selected: Option<usize>,
    pub window_start: usize,
}

impl FileCompletion {
    pub fn new() -> Self {
        Self {
            candidates: Vec::new(),
            selected: None,
            window_start: 0,
        }
    }

    pub fn is_active(&self) -> bool {
        !self.candidates.is_empty()
    }

    /// Update file completions based on text after the last @.
    pub fn update(&mut self, full_text: &str) {
        self.window_start = 0;
        if let Some(at_pos) = full_text.rfind('@') {
            let after_at = &full_text[at_pos + 1..];
            if !after_at.contains(' ') && after_at.len() < 200 {
                let prefix = after_at;
                let (dir, file_prefix) = if prefix.contains('/') {
                    let last_slash = prefix.rfind('/').unwrap();
                    (&prefix[..=last_slash], &prefix[last_slash + 1..])
                } else {
                    (".", prefix)
                };

                self.candidates.clear();
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.starts_with(file_prefix) && !name.starts_with('.') {
                            let full = if dir == "." {
                                name
                            } else {
                                format!("{}{}", dir, name)
                            };
                            let display = if entry.file_type().map_or(false, |kind| kind.is_dir()) {
                                format!("{}/", full)
                            } else {
                                full
                            };
                            self.candidates.push(display);
                        }
                    }
                }
                self.candidates.sort();
                self.selected = if self.candidates.is_empty() {
                    None
                } else {
                    Some(0)
                };
                return;
            }
        }
        self.close();
    }

    pub fn close(&mut self) {
        self.candidates.clear();
        self.selected = None;
        self.window_start = 0;
    }

    /// Accept selected file path. Returns the file path to insert.
    pub fn accept(&mut self) -> Option<String> {
        let result = self
            .selected
            .and_then(|index| self.candidates.get(index).cloned());
        self.close();
        result
    }

    pub fn cycle(&mut self) {
        cycle_file_selection(
            &self.candidates,
            &mut self.selected,
            &mut self.window_start,
            10,
            true,
        );
    }

    pub fn cycle_back(&mut self) {
        cycle_file_selection(
            &self.candidates,
            &mut self.selected,
            &mut self.window_start,
            10,
            false,
        );
    }
}

fn cycle_file_selection(
    candidates: &[String],
    selected: &mut Option<usize>,
    window_start: &mut usize,
    max_visible: usize,
    forward: bool,
) {
    if candidates.is_empty() {
        return;
    }
    let current = selected.unwrap_or(0);
    let total = candidates.len();

    if *window_start >= total {
        *window_start = 0;
    }

    if forward {
        let visible_end = *window_start + max_visible;
        if current + 1 >= total {
            *window_start = 0;
            *selected = Some(0);
        } else if selected.map_or(false, |index| index >= visible_end - 1) && visible_end < total {
            *window_start += 1;
            *selected = Some(current + 1);
        } else {
            *selected = Some(current + 1);
        }
    } else if current == 0 {
        *window_start = if total > max_visible {
            total - max_visible
        } else {
            0
        };
        *selected = Some(total - 1);
    } else if selected.map_or(false, |index| index == *window_start) && *window_start > 0 {
        *window_start -= 1;
        *selected = Some(current - 1);
    } else {
        *selected = Some(current - 1);
    }
}
