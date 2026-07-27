//! The record panel — full detail for the selected unit.
//!
//! Rendered as a persistent side panel when the terminal is wide enough, and
//! as a centred modal when it is not. A master-detail pair beats an overlay:
//! the detail is visible while you move the cursor, so scanning the inventory
//! and reading a record are the same gesture instead of two.

use chrono::{DateTime, Local};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::cache;
use crate::model::AppEntry;
use crate::tui::components::table::truncate;
use crate::tui::i18n::{self, Lang};
use crate::tui::theme::Theme;

/// Indent every line carries, and the column the values start in.
///
/// `RECORD_PANEL_WIDTH` is 36; after the shadow, two borders and two padding
/// columns the interior is 31, so a value has about 15 columns to work with.
/// That is the budget every field is measured against.
const INDENT: usize = 3;
const LABEL_PAD: usize = 13;
const LABEL_COL: usize = INDENT + LABEL_PAD;

pub struct DetailPanel<'a> {
    pub entry: Option<&'a AppEntry>,
    pub lang: Lang,
}

impl DetailPanel<'_> {
    /// Persistent side panel.
    pub fn render_side(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Vertical padding is decoration, so it yields before the record does.
        // Once the source cards took their rows, a 24-row terminal leaves this
        // panel 8 lines against a 9-line record, and spending one of them on a
        // blank inset would cost INVOCATIONS instead.
        let body = self
            .entry
            .map(|e| self.body(e, area.width.saturating_sub(4), theme));
        let essential = body
            .as_ref()
            .map(|lines| {
                lines
                    .iter()
                    .filter(|(rank, _)| matches!(rank, Rank::Name | Rank::Field))
                    .count()
            })
            .unwrap_or(1);
        let top_pad = u16::from(area.height.saturating_sub(3) as usize >= essential);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(theme.panel_border())
            .title(Span::styled(self.lang.strings().panel_record, theme.panel_title()))
            .padding(Padding::new(1, 1, top_pad, 0))
            .style(theme.base());
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let lines = match body {
            Some(lines) => elide(lines, inner.height as usize),
            None => vec![Line::from(Span::styled(
                format!(" {}", self.lang.strings().no_selection),
                theme.muted(),
            ))],
        };

        frame.render_widget(Paragraph::new(lines).style(theme.base()), inner);
    }

    /// Centred modal, for terminals too narrow to carry a side panel.
    pub fn render_modal(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let Some(entry) = self.entry else { return };
        let lines = elide(self.body(entry, 60, theme), area.height.saturating_sub(2) as usize);

        let rect = centered(area, 60, lines.len() as u16 + 2);
        if rect.width < 24 || rect.height < 5 {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    self.lang.strings().too_small_record,
                    theme.masthead(),
                )))
                .style(theme.base()),
                area,
            );
            return;
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(theme.accented())
            .title(Span::styled(self.lang.strings().panel_record, theme.panel_title()))
            .padding(Padding::new(1, 1, 1, 1))
            .style(theme.overlay());

        frame.render_widget(Clear, rect);
        frame.render_widget(Paragraph::new(lines).block(block), rect);
    }

    fn body<'b>(&self, e: &AppEntry, width: u16, theme: &Theme) -> Vec<(Rank, Line<'b>)> {
        let t = self.lang.strings();
        let value_budget = (width as usize).saturating_sub(LABEL_COL);
        let indent_budget = (width as usize).saturating_sub(INDENT);

        // Padded by display width: `{label:<13}` counts characters, so a CJK
        // label would be padded to half the column count and collide with the
        // value beside it. Values are cut to what is left of the panel — this
        // is a 36-column column, and anything longer is clipped by the
        // terminal mid-glyph rather than elided honestly.
        let field = |label: &str, value: String| -> Line<'b> {
            vec![
                Span::styled(format!("   {}", i18n::pad(label, LABEL_PAD)), theme.muted()),
                Span::styled(truncate(&value, value_budget), theme.base()),
            ]
            .into()
        };

        let blank = || (Rank::Spacer, Line::from(Span::styled("", theme.base())));

        let mut lines = vec![
            (
                Rank::Name,
                Line::from(vec![
                    Span::styled(" ● ", theme.accented()),
                    Span::styled(truncate(&e.name, indent_budget), theme.heading()),
                ]),
            ),
            blank(),
            (Rank::SectionHead, section(t.sec_origin, theme)),
            (Rank::Field, field(t.f_source, e.source.to_string().to_uppercase())),
            (
                Rank::Field,
                field(
                    t.f_interface,
                    e.ui_kind
                        .map(|kind| kind.to_string())
                        .unwrap_or_else(|| "—".into()),
                ),
            ),
            (
                Rank::Field,
                field(
                    t.f_category,
                    e.category
                        .as_ref()
                        .map(|category| i18n::category_label(category, self.lang).into_owned())
                        .unwrap_or_else(|| "—".into()),
                ),
            ),
            (
                Rank::Field,
                field(
                    t.f_language,
                    e.language
                        .as_ref()
                        .map(|l| l.to_string())
                        .unwrap_or_else(|| "—".into()),
                ),
            ),
            (
                Rank::Field,
                field(t.f_version, e.version.clone().unwrap_or_else(|| "—".into())),
            ),
            blank(),
            (Rank::SectionHead, section(t.sec_activity, theme)),
        ];

        // A date and its relative age do not fit side by side here: the label
        // eats LABEL_COL of a ~31-column interior, and "2026-05-19" alone is
        // 10 of the ~15 that remain. Rendering both inline clipped the closing
        // bracket mid-string — `2026-05-19（69`. So the age drops to its own
        // line, aligned under the value, whenever the pair overruns.
        let value_budget = (width as usize).saturating_sub(LABEL_COL);
        for (label, date) in [(t.f_installed, e.install_date), (t.f_last_used, e.last_used)] {
            let (date_text, age) = dated_parts(date, self.lang);
            match age {
                Some(age) if date_text.width() + age.width() > value_budget => {
                    lines.push((Rank::Field, field(label, date_text)));
                    lines.push((
                        Rank::AgeDetail,
                        Line::from(Span::styled(
                            format!("{}{}", " ".repeat(LABEL_COL), age.trim_start()),
                            theme.muted(),
                        )),
                    ));
                }
                Some(age) => lines.push((Rank::Field, field(label, format!("{date_text}{age}")))),
                None => lines.push((Rank::Field, field(label, date_text))),
            }
        }

        lines.push((
            Rank::Field,
            field(
                t.f_invocations,
                if e.usage_count == 0 {
                    t.none_recorded.into()
                } else {
                    format!("{}", e.usage_count)
                },
            ),
        ));
        lines.push(blank());
        lines.push((Rank::Location, section(t.sec_location, theme)));
        lines.push((
            Rank::Location,
            Line::from(Span::styled(
                format!("   {}", truncate(e.path.as_deref().unwrap_or("—"), indent_budget)),
                theme.base(),
            )),
        ));

        if let Some(d) = &e.description {
            lines.push(blank());
            lines.push((Rank::Note, section(t.sec_note, theme)));
            // Wrapped here rather than by `Paragraph`, which would break the
            // continuation flush against the panel border and lose the indent.
            for chunk in wrap_indented(d, width.saturating_sub(3)) {
                lines.push((
                    Rank::Note,
                    Line::from(Span::styled(format!("   {chunk}"), theme.muted())),
                ));
            }
        }
        lines
    }
}

/// What a short panel gives up first.
///
/// The record grew past the height a 24-row terminal can spare once the source
/// cards took their rows, and truncating from the bottom silently ate
/// INVOCATIONS — the single figure this tool exists to surface. So the panel
/// sheds by value instead of by position, in this order. PATH goes early
/// because a 36-column panel elides it into uselessness anyway.
///
/// NAME and the fields never go. On a wide terminal this panel retitles itself
/// as the cursor moves, so the name is the only thing binding these values to a
/// row — a record reading `SOURCE CARGO / INTERFACE —` with nothing naming it
/// is worse than a shorter record, not a denser one.
/// `AgeDetail` is the wrapped relative age. It goes early because the absolute
/// date on the line above already answers the question, just less directly —
/// it is the only genuinely redundant line in the record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    Spacer,
    AgeDetail,
    Note,
    Location,
    SectionHead,
    Name,
    Field,
}

/// Drop whole ranks, cheapest first, until the record fits `height`.
///
/// Rank-at-a-time rather than line-at-a-time: losing one of two blank spacers
/// or one of two section headers reads as a rendering fault, while losing the
/// whole class reads as density. The trailing `take` is the floor — when even
/// the fields overrun, clipping is unavoidable, but it is bounded and it
/// happens after everything else has already gone — including at heights below
/// three, where nothing meaningful fits and the only correct behaviour is to
/// return fewer lines rather than index past the end.
fn elide(mut lines: Vec<(Rank, Line<'_>)>, height: usize) -> Vec<Line<'_>> {
    for rank in [Rank::Spacer, Rank::AgeDetail, Rank::Note, Rank::Location, Rank::SectionHead] {
        if lines.len() <= height {
            break;
        }
        lines.retain(|(r, _)| *r != rank);
    }
    lines.into_iter().map(|(_, line)| line).take(height).collect()
}

/// Break `text` into display-width-bounded chunks on word boundaries.
///
/// Width-aware rather than char-aware so CJK descriptions do not overrun the
/// panel; a single word longer than the budget is emitted whole and clipped by
/// the terminal rather than being silently dropped.
fn wrap_indented(text: &str, width: u16) -> Vec<String> {
    let budget = width.max(8) as usize;
    let mut out = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if candidate.width() > budget && !line.is_empty() {
            out.push(std::mem::take(&mut line));
            line = word.to_string();
        } else {
            line = candidate;
        }
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

/// `2026-03-11（120 天前）` — the absolute date plus how long ago it was.
///
/// The bare date answers "when", but the question this panel exists to serve is
/// "how stale", and reading that off a calendar date is work the interface
/// should have already done. Brackets come from the language because CJK uses
/// full-width `（）`; ASCII parens beside Chinese text sit on the wrong baseline.
///
/// Split so the caller can wrap the age onto its own line when the pair does
/// not fit — returning one pre-joined string is what clipped the bracket.
/// `cache::age_label` is reused rather than reimplemented; it also feeds the
/// masthead, so its wording is not this panel's to shorten.
fn dated_parts(date: Option<DateTime<Local>>, lang: Lang) -> (String, Option<String>) {
    let Some(date) = date else {
        return ("—".into(), None);
    };
    let t = lang.strings();
    (
        date.format("%Y-%m-%d").to_string(),
        Some(format!("{}{}{}", t.age_l, cache::age_label(date, lang), t.age_r)),
    )
}

fn section<'a>(label: &str, theme: &Theme) -> Line<'a> {
    Line::from(Span::styled(format!(" [ {label} ]"), theme.field_label()))
}

/// Centre a `w`x`h` rect inside `area`, shrinking to fit rather than
/// overflowing when the terminal is small.
pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let width = w.min(area.width);
    let height = h.min(area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The value column is padded past the longest label; if a longer label is
    /// ever added, this fails instead of rendering "INVOCATIONS96".
    #[test]
    fn label_padding_clears_the_longest_label() {
        // Both languages are covered by i18n::tests::record_labels_fit_their_column.
        const PAD: usize = 13;
        for label in [
            "SOURCE",
            "INTERFACE",
            "CATEGORY",
            "LANGUAGE",
            "VERSION",
            "INSTALLED",
            "LAST USED",
            "INVOCATIONS",
        ] {
            assert!(label.len() < PAD, "{label} needs padding > {}", label.len());
        }
    }

    /// Nothing the record renders may overrun the panel interior, in either
    /// language. The date fields broke this first — `2026-05-19（69` with the
    /// bracket clipped off — but the guard is deliberately written against
    /// every line, because the next overflow will come from somewhere else:
    /// a long path, a custom category, or a relative age that grew a unit.
    #[test]
    fn no_record_line_overruns_the_panel() {
        use crate::model::{Language, Source};
        use chrono::Duration;

        // RECORD_PANEL_WIDTH 36, less the shadow column, two borders and two
        // padding columns.
        const INNER: u16 = 31;
        let theme = Theme::tactical();

        // Ages chosen to reach every unit `cache::age_label` can emit — the
        // month and year forms are the long ones.
        for ago in [
            Duration::minutes(3),
            Duration::hours(5),
            Duration::days(69),
            Duration::days(512),
            Duration::days(900),
        ] {
            for lang in [Lang::ZhHant, Lang::En] {
                let entry = AppEntry {
                    name: "a-very-long-package-name-that-will-not-fit".into(),
                    version: Some("1.2.3-rc4+build567".into()),
                    source: Source::Homebrew,
                    language: Some(Language::JavaScript),
                    install_date: Some(Local::now() - ago),
                    last_used: Some(Local::now() - ago),
                    usage_count: 12_345,
                    path: Some("/Applications/Visual Studio Code.app/Contents/MacOS/Electron".into()),
                    description: Some(
                        "A deliberately long description that must wrap inside the panel \
                         rather than run past its border."
                            .into(),
                    ),
                    ui_kind: None,
                    category: None,
                };
                let panel = DetailPanel { entry: Some(&entry), lang };
                let body = panel.body(&entry, INNER, &theme);

                for (rank, line) in &body {
                    let rendered: usize =
                        line.spans.iter().map(|s| s.content.width()).sum();
                    assert!(
                        rendered <= INNER as usize,
                        "{rank:?} line renders {rendered} cols, panel interior is {INNER} \
                         ({lang:?}, {ago} ago)"
                    );
                }

                // Fitting is not enough: the age must still be there. Dropping
                // it silently would satisfy the width assertion above while
                // losing the staleness signal the panel exists to show.
                let age_lines: Vec<&str> = body
                    .iter()
                    .filter(|(rank, _)| *rank == Rank::AgeDetail)
                    .map(|(_, line)| line.spans[0].content.as_ref())
                    .collect();
                assert_eq!(
                    age_lines.len(),
                    2,
                    "both dates should wrap their age at {INNER} cols ({lang:?}, {ago} ago)"
                );
                for age in age_lines {
                    assert!(
                        age.trim_end().ends_with(lang.strings().age_r),
                        "age {age:?} lost its closing bracket ({lang:?})"
                    );
                }
            }
        }
    }

    /// The panel must shed decoration before data. Pinning the order stops a
    /// future change from quietly truncating INVOCATIONS — the figure the whole
    /// tool exists to surface — while blank rows and an elided path are still
    /// on screen.
    #[test]
    fn elision_drops_decoration_before_fields() {
        let ranks = [
            Rank::Name,
            Rank::Spacer,
            Rank::SectionHead,
            Rank::Field,
            Rank::Field,
            Rank::Spacer,
            Rank::Location,
            Rank::Location,
            Rank::Note,
        ];
        // Each line carries its rank as content so the assertions read back
        // through `elide` itself rather than reimplementing it — a test that
        // re-derives the logic it checks cannot catch a change to that logic.
        let tagged = || -> Vec<(Rank, Line<'static>)> {
            ranks.iter().map(|r| (*r, Line::from(format!("{r:?}")))).collect()
        };
        let survivors = |height: usize| -> Vec<String> {
            elide(tagged(), height)
                .iter()
                .map(|line| line.spans[0].content.to_string())
                .collect()
        };

        // Full height keeps everything.
        assert_eq!(survivors(9).len(), 9);

        let has = |height: usize, rank: Rank| survivors(height).contains(&format!("{rank:?}"));

        assert!(!has(7, Rank::Spacer), "spacers go first");
        assert!(has(7, Rank::Location), "path outlives spacers");
        assert!(!has(4, Rank::Location), "path goes before headers");
        assert!(has(4, Rank::SectionHead), "headers outlive the path");

        // The name and the fields are the record. Three rows is the floor that
        // can still hold this fixture's name plus both its fields.
        for height in 3..=9 {
            assert!(has(height, Rank::Name), "name dropped at height {height}");
            let fields = survivors(height)
                .iter()
                .filter(|content| *content == "Field")
                .count();
            assert_eq!(fields, 2, "a field was dropped at height {height}");
        }
    }

    /// Below three rows nothing meaningful fits, so `elide` must return fewer
    /// lines rather than index past the end — arithmetic that underflows here
    /// panics inside ratatui's buffer, the same failure
    /// `centered_never_escapes_parent` guards against.
    #[test]
    fn elision_truncates_rather_than_panicking_when_there_is_no_room() {
        let ranks = [Rank::Name, Rank::Spacer, Rank::Field, Rank::Location];
        for height in 0..=4 {
            let lines: Vec<(Rank, Line<'static>)> =
                ranks.iter().map(|r| (*r, Line::from(format!("{r:?}")))).collect();
            let kept = elide(lines, height);
            assert!(
                kept.len() <= height,
                "height {height} produced {} lines",
                kept.len()
            );
        }
        // An empty record is also legal and must not panic.
        assert!(elide(Vec::new(), 0).is_empty());
    }

    /// Wrapped continuation lines must never exceed the panel's inner width,
    /// or they break flush against the border and lose the indent.
    #[test]
    fn wrap_respects_the_width_budget() {
        for (text, w) in [
            ("Distributed revision control system", 30u16),
            ("分散式版本控制系統，用於追蹤原始碼變更", 20),
            ("supercalifragilisticexpialidocious", 10),
            ("", 30),
        ] {
            for chunk in wrap_indented(text, w) {
                assert!(
                    chunk.width() <= w.max(8) as usize || !chunk.contains(' '),
                    "{chunk:?} is {} cols, budget {w}",
                    chunk.width()
                );
            }
        }
    }

    /// Wrapping must not drop content.
    #[test]
    fn wrap_preserves_every_word() {
        let text = "Distributed revision control system";
        let joined = wrap_indented(text, 12).join(" ");
        assert_eq!(joined, text);
    }

    /// The overlay must always stay inside its parent — a rect that escapes
    /// `area` panics inside ratatui's buffer.
    #[test]
    fn centered_never_escapes_parent() {
        let parent = Rect { x: 3, y: 2, width: 40, height: 12 };
        for (w, h) in [(10u16, 4u16), (68, 20), (1, 1), (999, 999)] {
            let r = centered(parent, w, h);
            assert!(r.x >= parent.x && r.y >= parent.y, "{r:?}");
            assert!(r.x + r.width <= parent.x + parent.width, "{r:?}");
            assert!(r.y + r.height <= parent.y + parent.height, "{r:?}");
        }
    }
}
