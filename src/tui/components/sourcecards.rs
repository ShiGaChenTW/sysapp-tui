//! Per-source totals between the masthead and the main panels.
//!
//! This row is optional chrome: it gives density at a glance, but the
//! inventory grid always wins when vertical space is tight. Width handling is
//! strict because a card rect that escapes its parent panics inside ratatui's
//! buffer, and the overflow case is the one most likely to regress.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::tui::theme::Theme;

use super::super::cast_shadow;

// The label is the card: no decorative blank row above or below it.
const CARD_HEIGHT: u16 = 1;
const SHADOW_DEPTH: u16 = 1;
const GAP_X: u16 = 1;
const GAP_Y: u16 = 1;
// Two blank cells on both sides of every label. The extra breathing room keeps
// dense source names and counts from reading as if they touch the card edges.
const PAD_X: u16 = 2;
const MIN_TOTAL_HEIGHT: u16 = CARD_HEIGHT + SHADOW_DEPTH;
const HEIGHT_GATE: u16 = 20;

/// Card, shadow, and one blank separator row below it.
pub const HEIGHT: u16 = CARD_HEIGHT + SHADOW_DEPTH + GAP_Y;

#[derive(Debug, Clone, PartialEq, Eq)]
enum CardKind<'a> {
    Source(&'a str, usize),
    Overflow(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CardLayout<'a> {
    area: Rect,
    kind: CardKind<'a>,
}

pub struct SourceCards<'a> {
    pub counts: &'a [(String, usize)],
}

impl SourceCards<'_> {
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if area.width == 0 || area.height < MIN_TOTAL_HEIGHT {
            return;
        }

        let row = Rect {
            height: MIN_TOTAL_HEIGHT.min(area.height),
            ..area
        };

        for card in layout_cards(row, self.counts) {
            let card_area = cast_shadow(frame, card.area, theme);
            frame.render_widget(Paragraph::new("").style(theme.masthead()), card_area);
            let text_row = Rect {
                y: card_area.y + card_area.height.saturating_sub(1) / 2,
                height: 1,
                ..card_area
            };
            let line = match card.kind {
                CardKind::Source(name, count) => Line::from(vec![
                    Span::styled(name, theme.masthead()),
                    Span::styled(format!(" {count}"), theme.masthead()),
                ]),
                CardKind::Overflow(omitted) => {
                    Line::from(Span::styled(format!("+{omitted}"), theme.masthead()))
                }
            };
            frame.render_widget(
                Paragraph::new(line).style(theme.masthead()).centered(),
                text_row,
            );
        }
    }
}

pub fn fits(area_height: u16) -> bool {
    area_height >= HEIGHT_GATE
}

fn layout_cards<'a>(area: Rect, counts: &'a [(String, usize)]) -> Vec<CardLayout<'a>> {
    if area.width == 0 || area.height < MIN_TOTAL_HEIGHT || counts.is_empty() {
        return Vec::new();
    }

    let widths: Vec<u16> = counts
        .iter()
        .map(|(name, count)| card_width(&format!("{name} {count}")))
        .collect();

    if let Some(shown) = all_sources_that_fit(area.width, &widths) {
        return build_source_layout(area, counts, &widths, shown);
    }

    for shown in (0..counts.len()).rev() {
        let omitted = counts.len() - shown;
        let needed = prefix_width(&widths[..shown]) + gap_count(shown + 1) + overflow_width(omitted);
        if needed <= area.width {
            return build_with_overflow(area, counts, &widths, shown, omitted);
        }
    }

    build_source_layout(area, counts, &widths, all_sources_that_fit_without_overflow(area.width, &widths))
}

fn build_source_layout<'a>(
    area: Rect,
    counts: &'a [(String, usize)],
    widths: &[u16],
    shown: usize,
) -> Vec<CardLayout<'a>> {
    let mut out = Vec::with_capacity(shown);
    let mut x = area.x;
    for ((name, count), width) in counts.iter().zip(widths).take(shown) {
        out.push(CardLayout {
            area: Rect {
                x,
                y: area.y,
                width: *width,
                height: MIN_TOTAL_HEIGHT,
            },
            kind: CardKind::Source(name.as_str(), *count),
        });
        x = x.saturating_add(*width).saturating_add(GAP_X);
    }
    out
}

fn build_with_overflow<'a>(
    area: Rect,
    counts: &'a [(String, usize)],
    widths: &[u16],
    shown: usize,
    omitted: usize,
) -> Vec<CardLayout<'a>> {
    let mut out = build_source_layout(area, counts, widths, shown);
    let x = out
        .last()
        .map(|card| card.area.x + card.area.width + GAP_X)
        .unwrap_or(area.x);
    out.push(CardLayout {
        area: Rect {
            x,
            y: area.y,
            width: overflow_width(omitted),
            height: MIN_TOTAL_HEIGHT,
        },
        kind: CardKind::Overflow(omitted),
    });
    out
}

fn all_sources_that_fit(area_width: u16, widths: &[u16]) -> Option<usize> {
    let needed = prefix_width(widths) + gap_count(widths.len());
    (needed <= area_width).then_some(widths.len())
}

fn all_sources_that_fit_without_overflow(area_width: u16, widths: &[u16]) -> usize {
    let mut shown = 0usize;
    let mut used = 0u16;
    for width in widths {
        let next = if shown == 0 {
            *width
        } else {
            used.saturating_add(GAP_X).saturating_add(*width)
        };
        if next > area_width {
            break;
        }
        used = next;
        shown += 1;
    }
    shown
}

fn prefix_width(widths: &[u16]) -> u16 {
    widths.iter().copied().sum()
}

fn gap_count(cards: usize) -> u16 {
    cards.saturating_sub(1) as u16 * GAP_X
}

fn overflow_width(omitted: usize) -> u16 {
    card_width(&format!("+{omitted}"))
}

fn card_width(text: &str) -> u16 {
    let text_width: u16 = text.width().try_into().unwrap_or(u16::MAX);
    text_width
        .saturating_add(PAD_X * 2)
        .saturating_add(SHADOW_DEPTH)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(width: u16) -> Rect {
        Rect {
            x: 4,
            y: 2,
            width,
            height: MIN_TOTAL_HEIGHT,
        }
    }

    fn counts() -> Vec<(String, usize)> {
        vec![
            ("BREW".into(), 323),
            ("CASK".into(), 88),
            ("APP".into(), 42),
            ("NPM".into(), 9),
            ("PIP".into(), 4),
        ]
    }

    #[test]
    fn fits_gates_short_viewports() {
        assert!(!fits(19));
        assert!(fits(20));
    }

    #[test]
    fn cards_leave_two_blank_cells_around_the_label() {
        // "BREW 323" is 8 cells, plus two cells at each end and the shadow
        // column that layout reserves outside the painted card.
        assert_eq!(card_width("BREW 323"), 13);
        assert_eq!(overflow_width(4), 7);
    }

    #[test]
    fn cards_have_no_blank_rows_around_the_label() {
        assert_eq!(CARD_HEIGHT, 1);
        assert_eq!(MIN_TOTAL_HEIGHT, 2, "one label row plus one shadow row");
        assert_eq!(HEIGHT, 3, "card, shadow, and separator row");
    }

    #[test]
    fn layout_rects_stay_inside_the_parent_and_overflow_count_is_exact() {
        let counts = counts();
        for width in 0..80u16 {
            let area = area(width);
            let cards = layout_cards(area, &counts);
            let shown = cards
                .iter()
                .filter(|card| matches!(card.kind, CardKind::Source(_, _)))
                .count();
            let overflow = cards.iter().find_map(|card| match card.kind {
                CardKind::Overflow(omitted) => Some(omitted),
                CardKind::Source(_, _) => None,
            });

            for card in &cards {
                assert!(card.area.x >= area.x && card.area.y >= area.y, "{card:?}");
                assert!(
                    card.area.x + card.area.width <= area.x + area.width,
                    "{card:?} escapes {area:?}"
                );
                assert!(
                    card.area.y + card.area.height <= area.y + area.height,
                    "{card:?} escapes {area:?}"
                );
            }

            if let Some(omitted) = overflow {
                assert_eq!(omitted, counts.len() - shown, "width {width}");
            }
        }
    }
}
