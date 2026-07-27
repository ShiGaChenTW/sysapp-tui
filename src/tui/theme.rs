//! Semantic color system.
//!
//! Colors are declared by *function*, never by appearance, and widget code
//! references slots only — no hex literals outside this file (tui-design §4).
//!
//! Visual direction is Tactical Telemetry from industrial-brutalist-ui §4.2:
//! near-black CRT substrate, white phosphor text, aviation-hazard red as the
//! single accent. Terminal green is reserved for exactly one element (the
//! activity meter) and appears nowhere else.
//!
//! Three tiers degrade gracefully: true color → 16 ANSI → monochrome. The
//! interface must stay *usable* at the bottom tier; color only enhances.

use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    TrueColor,
    Ansi16,
    Mono,
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub tier: Tier,

    // Substrate
    pub bg_base: Color,
    pub bg_overlay: Color,
    pub bg_selection: Color,

    // Text
    pub fg_default: Color,
    pub fg_muted: Color,
    pub fg_emphasis: Color,

    // The single accent — hazard red
    pub accent: Color,

    // Structure
    pub rule: Color,

    // The one permitted green (activity meter only)
    pub meter: Color,

    // Masthead band field and its text.
    pub band: Color,
    pub fg_on_band: Color,

    // Drop shadow cast by the cards.
    pub shadow: Color,
}

impl Theme {
    /// Pick a tier from the environment, honouring `NO_COLOR` above all else.
    pub fn detect() -> Self {
        if std::env::var_os("NO_COLOR").is_some() {
            return Self::mono();
        }
        match std::env::var("COLORTERM").as_deref() {
            Ok("truecolor") | Ok("24bit") => return Self::tactical(),
            _ => {}
        }
        if std::env::var("TERM").is_ok_and(|t| t.contains("256color")) {
            return Self::tactical();
        }
        Self::ansi16()
    }

    /// True-color Tactical Telemetry palette.
    pub fn tactical() -> Self {
        Self {
            tier: Tier::TrueColor,
            bg_base: Color::Rgb(0x0A, 0x0A, 0x0A),
            bg_overlay: Color::Rgb(0x1A, 0x1A, 0x1A),
            bg_selection: Color::Rgb(0x2E, 0x14, 0x16),
            fg_default: Color::Rgb(0xEA, 0xEA, 0xEA),
            fg_muted: Color::Rgb(0x6E, 0x6E, 0x6E),
            fg_emphasis: Color::Rgb(0xFF, 0xFF, 0xFF),
            accent: Color::Rgb(0xE6, 0x19, 0x19),
            rule: Color::Rgb(0x3A, 0x3A, 0x3A),
            meter: Color::Rgb(0x4A, 0xF6, 0x26),
            band: Color::Rgb(0x6E, 0x0D, 0x10),
            fg_on_band: Color::Rgb(0xFF, 0xFF, 0xFF),
            shadow: Color::Rgb(0x00, 0x00, 0x00),
        }
    }

    /// 16-color fallback. The terminal theme owns the actual hues.
    pub fn ansi16() -> Self {
        Self {
            tier: Tier::Ansi16,
            bg_base: Color::Black,
            bg_overlay: Color::Black,
            bg_selection: Color::DarkGray,
            fg_default: Color::Gray,
            fg_muted: Color::DarkGray,
            fg_emphasis: Color::White,
            accent: Color::Red,
            rule: Color::DarkGray,
            meter: Color::Green,
            band: Color::Red,
            fg_on_band: Color::White,
            shadow: Color::Black,
        }
    }

    /// No color at all — hierarchy carried entirely by weight and reverse video.
    pub fn mono() -> Self {
        Self {
            tier: Tier::Mono,
            bg_base: Color::Reset,
            bg_overlay: Color::Reset,
            bg_selection: Color::Reset,
            fg_default: Color::Reset,
            fg_muted: Color::Reset,
            fg_emphasis: Color::Reset,
            accent: Color::Reset,
            rule: Color::Reset,
            meter: Color::Reset,
            band: Color::Reset,
            fg_on_band: Color::Reset,
            shadow: Color::Reset,
        }
    }

    // ---- Derived styles ----------------------------------------------------
    //
    // Every style pairs color with a non-color signal (bold / dim / reverse) so
    // meaning survives the mono tier. tui-design §7.5: strip the color and the
    // interface must still read.

    pub fn base(&self) -> Style {
        Style::default().fg(self.fg_default).bg(self.bg_base)
    }

    pub fn muted(&self) -> Style {
        Style::default()
            .fg(self.fg_muted)
            .bg(self.bg_base)
            .add_modifier(Modifier::DIM)
    }

    /// Macro-typography: uppercase structural headers.
    pub fn heading(&self) -> Style {
        Style::default()
            .fg(self.fg_emphasis)
            .bg(self.bg_base)
            .add_modifier(Modifier::BOLD)
    }

    pub fn accented(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .bg(self.bg_base)
            .add_modifier(Modifier::BOLD)
    }

    /// Structural rules and box drawing.
    pub fn rule_style(&self) -> Style {
        Style::default().fg(self.rule).bg(self.bg_base)
    }

    /// Selected row.
    ///
    /// A filled accent bar across a full-width row reads as an alarm, not a
    /// cursor — on a 7-column grid it floods a third of the screen. The
    /// cursor is carried by a left marker plus weight instead, with a surface
    /// tint for position. Mono keeps REVERSED, which is all it has.
    pub fn selection(&self) -> Style {
        let s = Style::default()
            .fg(self.fg_emphasis)
            .bg(self.bg_selection)
            .add_modifier(Modifier::BOLD);
        if self.tier == Tier::Mono {
            s.add_modifier(Modifier::REVERSED)
        } else {
            s
        }
    }

    /// Column header of the data grid.
    pub fn column_header(&self) -> Style {
        Style::default()
            .fg(self.fg_muted)
            .bg(self.bg_base)
            .add_modifier(Modifier::BOLD)
    }

    /// The sorted column, marked by accent text rather than a filled block.
    pub fn column_header_active(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .bg(self.bg_base)
            .add_modifier(Modifier::BOLD)
    }

    /// Panel chrome. Borders recede; the content is the interface.
    pub fn panel_border(&self) -> Style {
        Style::default().fg(self.rule).bg(self.bg_base)
    }

    /// Panel title, sitting on the border line.
    pub fn panel_title(&self) -> Style {
        Style::default()
            .fg(self.fg_muted)
            .bg(self.bg_base)
            .add_modifier(Modifier::BOLD)
    }

    /// Section label inside a panel — `[ SOURCE ]`.
    pub fn field_label(&self) -> Style {
        Style::default().fg(self.accent).bg(self.bg_base)
    }

    pub fn overlay(&self) -> Style {
        Style::default().fg(self.fg_default).bg(self.bg_overlay)
    }

    /// Masthead band: deep red field, white text.
    ///
    /// Distinct from `accent` — a full-width bar of hazard red at maximum
    /// chroma dominates the screen and fights the data. The band is the darker
    /// oxide; hazard red stays reserved for marks and cursors, where being
    /// loud is the point.
    pub fn masthead(&self) -> Style {
        Style::default()
            .fg(self.fg_on_band)
            .bg(self.band)
            .add_modifier(Modifier::BOLD)
    }

    /// Drop shadow beneath a card.
    ///
    /// Pure black against the near-black substrate — deliberately subtle. A
    /// heavier shadow on a dark terminal reads as a rendering fault rather
    /// than depth, and it degrades to nothing at the 16-colour and monochrome
    /// tiers, which is the correct behaviour: depth is decoration, and the
    /// interface must not depend on it.
    pub fn shadow_style(&self) -> Style {
        Style::default().bg(self.shadow)
    }

    /// The one green thing in the entire interface.
    pub fn meter_style(&self) -> Style {
        Style::default().fg(self.meter).bg(self.bg_base)
    }


}

#[cfg(test)]
mod tests {
    use super::*;

    /// NO_COLOR must win over every other signal. Guarded by a mutex because
    /// env vars are process-global and Rust runs tests in parallel.
    #[test]
    fn no_color_forces_mono() {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let prev_no = std::env::var_os("NO_COLOR");
        let prev_ct = std::env::var_os("COLORTERM");
        unsafe {
            std::env::set_var("NO_COLOR", "1");
            std::env::set_var("COLORTERM", "truecolor");
        }
        let tier = Theme::detect().tier;
        unsafe {
            match prev_no {
                Some(v) => std::env::set_var("NO_COLOR", v),
                None => std::env::remove_var("NO_COLOR"),
            }
            match prev_ct {
                Some(v) => std::env::set_var("COLORTERM", v),
                None => std::env::remove_var("COLORTERM"),
            }
        }
        assert_eq!(tier, Tier::Mono);
    }

    /// Selection must stay legible without color — reverse video carries it.
    #[test]
    fn mono_selection_is_reversed() {
        assert!(
            Theme::mono()
                .selection()
                .add_modifier
                .contains(Modifier::REVERSED)
        );
    }
}
