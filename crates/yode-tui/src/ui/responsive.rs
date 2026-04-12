#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Density {
    Wide,
    Medium,
    Narrow,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StatusSectionMode {
    Full,
    Compact,
    Collapsed,
}

pub fn density_from_width(width: u16, narrow_max: u16, medium_max: u16) -> Density {
    if width < narrow_max {
        Density::Narrow
    } else if width < medium_max {
        Density::Medium
    } else {
        Density::Wide
    }
}

pub fn status_section_mode(width: u16) -> StatusSectionMode {
    match density_from_width(width, 68, 96) {
        Density::Wide => StatusSectionMode::Full,
        Density::Medium => StatusSectionMode::Compact,
        Density::Narrow => StatusSectionMode::Collapsed,
    }
}

#[cfg(test)]
mod tests {
    use super::{density_from_width, status_section_mode, Density, StatusSectionMode};

    #[test]
    fn status_section_mode_collapses_at_narrow_widths() {
        assert!(matches!(density_from_width(120, 68, 96), Density::Wide));
        assert!(matches!(status_section_mode(120), StatusSectionMode::Full));
        assert!(matches!(status_section_mode(80), StatusSectionMode::Compact));
        assert!(matches!(status_section_mode(50), StatusSectionMode::Collapsed));
    }
}
