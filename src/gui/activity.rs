//! The Overview's "What's happening": the latest presses of the controller in use with what each
//! did, and the profile switches the daemon made, each with the reason for it.

use iced::widget::{column, row};

use super::*;

/// Switches listed; the daemon keeps more.
const SHOWN_SWITCHES: usize = 8;

/// How long ago a switch was, in words.
fn ago(ms: u64) -> String {
    let secs = ms / 1000;
    match secs {
        0..=4 => "just now".into(),
        5..=59 => format!("{secs} s ago"),
        60..=3599 => format!("{} min ago", secs / 60),
        _ => format!("{} h ago", secs / 3600),
    }
}

impl App {
    pub(super) fn view_activity(&self) -> Element<'_, Message> {
        let presses: Element<'_, Message> = match &self.feed {
            Some(feed) if !feed.lines.is_empty() => crate::overlay::draw::log_panel(feed, self.preview_font()),
            _ => {
                let hint = if self.status.is_some() { "Press a button on a managed controller." } else { "Needs the daemon." };
                text(hint).size(13).color(MUTED_COLOR).into()
            }
        };

        let switches = self.status.as_ref().map(|s| s.switches.as_slice()).unwrap_or_default();
        let mut list = column![].spacing(10);
        if switches.is_empty() {
            list = list.push(text("No profile has switched since the daemon started.").size(13).color(MUTED_COLOR));
        }
        for s in switches.iter().take(SHOWN_SWITCHES) {
            let setup = s.game.as_deref().unwrap_or("General");
            list = list.push(
                row![
                    text(ago(s.age_ms)).size(12).color(MUTED_COLOR).width(Length::Fixed(90.0)),
                    column![text(format!("{setup} › {}", s.profile)), text(&s.why).size(12).color(MUTED_COLOR)]
                        .spacing(2)
                        .width(Length::Fill),
                ]
                .spacing(12),
            );
        }

        column![
            row![
                text("What's happening").size(20),
                help(
                    "The latest presses on the controller in use, one line per burst, with what each did when it's \
                     mapped to something else. Below, each profile switch and the rule or action that made it."
                        .into(),
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            text("Last presses").size(14),
            presses,
            text("Profile switches").size(14),
            list,
        ]
        .spacing(10)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        gui::tests::*,
        inputlog::{LogCell, LogLine},
        ipc::SwitchEvent,
    };

    #[test]
    fn ages_read_in_words() {
        assert_eq!(ago(0), "just now");
        assert_eq!(ago(42_000), "42 s ago");
        assert_eq!(ago(125_000), "2 min ago");
        assert_eq!(ago(7_300_000), "2 h ago");
    }

    #[test]
    fn overview_shows_the_feed_and_the_switches_with_their_reasons() {
        let mut app = app();
        app.status = Some(Status {
            switches: vec![SwitchEvent {
                age_ms: 12_000,
                game: Some("Doom".into()),
                profile: "Play".into(),
                why: "Doom matches Executable “doom.exe”".into(),
            }],
            ..Default::default()
        });
        let cell = LogCell { glyph: Vec::new(), label: Some("Fire".into()), hold_ms: Some(80), count: 1, held: false };
        let feed = LogView {
            style: OverlayStyle::default(),
            opacity: 1.0,
            lines: vec![LogLine { opacity: 1.0, cells: vec![cell] }],
            show_labels: true,
            show_holds: true,
        };
        let _ = app.update(Message::LiveFeed(Some(feed.clone())));
        assert_eq!(app.feed, Some(feed));
        let _ = app.view();

        let _ = app.update(Message::LiveFeed(None));
        assert_eq!(app.feed, None);
        let _ = app.view();
    }
}
