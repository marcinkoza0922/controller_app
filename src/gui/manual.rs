//! The Manual page. The text is `docs/manual/*.md`, the same Markdown that GitHub shows, embedded at
//! build time. Each file starts with its `# Title`, and links between chapters are file names.

use iced::widget::{column, container, markdown, row, rule, text};

use super::*;

/// The chapters, in the order listed on the page.
const CHAPTERS: &[(&str, &str)] = &[
    ("welcome.md", include_str!("../../docs/manual/welcome.md")),
    ("settings-window.md", include_str!("../../docs/manual/settings-window.md")),
    ("first-change.md", include_str!("../../docs/manual/first-change.md")),
    ("words.md", include_str!("../../docs/manual/words.md")),
    ("setups.md", include_str!("../../docs/manual/setups.md")),
    ("buttons.md", include_str!("../../docs/manual/buttons.md")),
    ("sticks.md", include_str!("../../docs/manual/sticks.md")),
    ("gyro.md", include_str!("../../docs/manual/gyro.md")),
    ("layers.md", include_str!("../../docs/manual/layers.md")),
    ("macros.md", include_str!("../../docs/manual/macros.md")),
    ("menus.md", include_str!("../../docs/manual/menus.md")),
    ("overlays.md", include_str!("../../docs/manual/overlays.md")),
    ("guide-button.md", include_str!("../../docs/manual/guide-button.md")),
    ("in-game-menu.md", include_str!("../../docs/manual/in-game-menu.md")),
    ("sharing.md", include_str!("../../docs/manual/sharing.md")),
    ("troubleshooting.md", include_str!("../../docs/manual/troubleshooting.md")),
];

/// The chapters parsed for drawing, in the order of `CHAPTERS`.
pub(super) fn load() -> Vec<markdown::Content> {
    CHAPTERS.iter().map(|(_, source)| markdown::Content::parse(source)).collect()
}

/// The chapter a link points to, by the file it names.
pub(super) fn chapter_of(link: &str) -> Option<usize> {
    CHAPTERS.iter().position(|(file, _)| *file == link)
}

/// The chapter's title: the text of its first line.
fn title(source: &str) -> &str {
    source.lines().next().unwrap_or_default().trim_start_matches('#').trim()
}

impl App {
    /// The chapter list on the left, and the chosen chapter on the right.
    pub(super) fn view_manual(&self) -> Element<'_, Message> {
        let chosen = self.manual_chapter.min(CHAPTERS.len() - 1);
        let mut index = column![].spacing(4);
        for (i, (_, source)) in CHAPTERS.iter().enumerate() {
            index = index.push(
                button(text(title(source)).size(14))
                    .width(Length::Fill)
                    .padding([6, 10])
                    .style(style::nav(i == chosen))
                    .on_press(Message::SelectChapter(i)),
            );
        }
        let settings = markdown::Settings::from(&self.theme());
        let body = markdown::view(self.manual[chosen].items(), settings).map(Message::ManualLink);
        row![
            container(index).width(220),
            rule::vertical(1),
            container(body).width(Length::Fill).padding([0, 8]),
        ]
        .spacing(16)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapters_have_titles_and_text() {
        let mut titles = HashSet::new();
        for (file, source) in CHAPTERS {
            assert!(source.starts_with("# "), "{file} doesn't start with a title");
            assert!(titles.insert(title(source)), "two chapters are titled {:?}", title(source));
        }
        for content in load() {
            assert!(!content.items().is_empty());
        }
    }

    #[test]
    fn links_between_chapters_resolve() {
        let source: String = CHAPTERS.iter().map(|(_, s)| *s).collect();
        for link in source.split("](").skip(1).filter_map(|rest| rest.split(')').next()) {
            if link.starts_with("http") || link.is_empty() {
                continue;
            }
            assert!(chapter_of(link).is_some(), "link to unknown chapter {link:?}");
        }
    }
}
