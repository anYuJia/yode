#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Density {
    Wide,
    Medium,
    Narrow,
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
