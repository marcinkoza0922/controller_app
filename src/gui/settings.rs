//! The App settings page, and the controller list on the Overview.

use iced::widget::{column, row};

use super::*;

/// An entry in the auto-switch "When no setup matches, use" list.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct DefaultChoice(pub(super) Option<ProfileRef>);

impl fmt::Display for DefaultChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Some(at) => at.fmt(f),
            None => f.write_str("(keep current profile)"),
        }
    }
}

/// The motion-sensor udev rule shipped in dist/, built in so the command works from anywhere.
pub(super) const MOTION_RULE_FILE: &str = include_str!("../../dist/70-padwight-motion.rules");

/// Shell command (bash or fish) that installs the motion-sensor udev rule and applies it.
pub(super) fn motion_rule_command() -> String {
    let rules: Vec<&str> = MOTION_RULE_FILE
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    format!(
        "echo '{}' | sudo tee /etc/udev/rules.d/70-padwight-motion.rules >/dev/null \
         && sudo udevadm control --reload && sudo udevadm trigger",
        rules.join("\n")
    )
}

impl App {
    /// Light, dark or the desktop's choice. Only the settings window follows it; the overlays keep
    /// their own colors.
    fn view_appearance(&self) -> Element<'_, Message> {
        let choice = |label: &'static str, appearance: Appearance| {
            button(text(label).size(14))
                .style(style::segment(self.config.appearance == appearance))
                .padding([5, 14])
                .on_press(Message::SetAppearance(appearance))
        };
        section(
            "Appearance",
            Some("Auto follows the desktop's light or dark setting.".into()),
            vec![
                container(row![choice("Auto", Appearance::Auto), choice("Light", Appearance::Light), choice("Dark", Appearance::Dark)].spacing(2))
                    .padding(3)
                    .style(style::segments)
                    .into(),
            ],
        )
    }

    /// The Nintendo layout's switch: which label each face button shows. The bindings don't move.
    fn view_nintendo_layout(&self) -> Element<'_, Message> {
        section(
            "Button layout",
            Some(
                "Swaps A with B and X with Y, to match prompts that a game draws in the Nintendo layout. On a \
                 Nintendo pad the face buttons send the letter on their label, so the B button sends B. A face \
                 button whose plain press is changed turns that off, and the buttons send their positions again. \
                 On other pads only the labels change. The swapped labels carry a swap icon wherever they show here."
                    .into(),
            ),
            vec![toggler(self.config.nintendo_layout).label("Nintendo button layout").on_toggle(Message::SetNintendoLayout).into()],
        )
    }

    /// The App settings page's sounds: the set every setup without its own set uses.
    fn view_default_sounds(&self) -> Element<'_, Message> {
        let set = self.config.sounds;
        let mut rows: Vec<Element<'_, Message>> = SoundOverlay::ALL.iter().map(|&overlay| overlay_sounds(set, overlay, self.open_sounds.contains(&overlay), Message::SetSounds)).collect();
        rows.push(button(text("Turn every sound off").size(13)).style(style::secondary).on_press(Message::SetSounds(set.silenced())).into());
        section(
            "Overlay sounds",
            Some("A faint sound as the cursor moves in an overlay, another when something is picked, and a softer one as it pops in and out. These apply to every setup that doesn't set its own on its Details tab. Any overlay can be turned off.".into()),
            rows,
        )
    }

    pub(super) fn view_settings(&self) -> Element<'_, Message> {
        let glyphs = section(
            "Info overlays",
            None,
            vec![labeled(
                "Fallback glyphs",
                row![
                    dropdown(PadFamily::ALL, Some(self.config.info_glyphs), Message::SetInfoGlyphs).width(SETTING_WIDTH),
                    help(
                        "Glyphs follow the controller in use: Xbox, PlayStation or Nintendo labels. For a \
                         controller that can't be recognized, they're drawn like this kind instead. Also \
                         used for previews."
                            .into(),
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            )],
        );
        column![
            self.view_appearance(),
            self.view_nintendo_layout(),
            self.view_auto_switch(),
            self.view_font_card(),
            self.view_color_card(),
            self.view_motion_card(),
            self.view_default_sounds(),
            self.view_keyboard_card(),
            self.view_numpad_card(),
            self.view_media_card(),
            self.view_in_game_menu_card(),
            glyphs,
        ]
            .spacing(16)
            .into()
    }

    #[expect(clippy::too_many_lines, reason = "predates the size lints")]
    pub(super) fn view_devices(&self) -> Element<'_, Message> {
        let mut list = column![text("Controllers").size(20)].spacing(8);
        match &self.status {
            Some(status) if !status.devices.is_empty() => {
                for d in &status.devices {
                    let state = if d.managed {
                        "remapping"
                    } else if d.ignored {
                        "ignored"
                    } else if !status.enabled {
                        "paused (remapping is off)"
                    } else {
                        "not remapped"
                    };
                    let state = if d.analog_triggers { state.to_string() } else { format!("{state} · on/off triggers") };
                    let state = if d.rumble { state } else { format!("{state} · no rumble") };
                    let state = if d.gyro { format!("{state} · gyro") } else { state };
                    let state = if d.paddles { format!("{state} · back paddles") } else { state };
                    let calibrate: Element<'_, Message> = if d.gyro {
                        button(text("Calibrate gyro").size(13))
                            .style(style::secondary)
                            .on_press(Message::CalibrateGyro(d.path.clone()))
                            .into()
                    } else {
                        space().into()
                    };
                    let name = d.name.clone();
                    let rumble: Element<'_, Message> = if d.rumble {
                        button(text("Test rumble").size(13))
                            .style(style::secondary)
                            .on_press(Message::TestRumble(d.path.clone()))
                            .into()
                    } else {
                        space().into()
                    };
                    list = list.push(
                        row![
                            column![text(&d.name), text(format!("{} · {state}", d.path)).size(12).color(MUTED_COLOR)]
                                .width(Length::Fill),
                            calibrate,
                            rumble,
                            checkbox(!d.ignored)
                                .label("Manage")
                                .on_toggle(move |on| Message::SetIgnored(name.clone(), !on)),
                        ]
                        .spacing(12)
                        .align_y(Alignment::Center),
                    );
                }
            }
            Some(_) => list = list.push(text("No controllers detected.").color(MUTED_COLOR)),
            None => list = list.push(text(NEEDS_DAEMON).size(13).color(MUTED_COLOR)),
        }
        if let Some(status) = &self.status
            && !status.motion_access_denied.is_empty()
        {
            let names = status.motion_access_denied.join(", ");
            list = list.push(
                column![
                    text(format!(
                        "Can't use the gyro on {names}: this app isn't allowed to read its motion sensors. \
                         Installing a udev rule fixes this (it asks for your password once):"
                    ))
                    .size(12)
                    .color(ERROR_COLOR),
                    row![
                        text(motion_rule_command()).size(11).font(iced::Font::MONOSPACE).width(Length::Fill),
                        button(text("Copy command").size(13))
                            .style(style::secondary)
                            .on_press(Message::CopyMotionRuleCommand),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(6),
            );
        }
        list.into()
    }

    pub(super) fn view_auto_switch(&self) -> Element<'_, Message> {
        let auto = &self.config.auto_switch;
        let how = match self.status.as_ref().map(|s| s.focus_backend) {
            Some(backend) => match backend.desktop() {
                Some(desktop) => format!("Profiles switch as the focused window changes ({desktop})."),
                None => "Window tracking isn't available on this desktop, so rules apply while a matching \
                         process is running."
                    .to_string(),
            },
            None => NEEDS_DAEMON.to_string(),
        };
        let mut defaults = vec![DefaultChoice(None)];
        defaults.extend(
            self.config.all_games().flat_map(|(key, g)| g.profiles.iter().map(move |p| DefaultChoice(Some(ProfileRef::new(key, &p.name))))),
        );
        let default = DefaultChoice(auto.default_profile.clone());
        section(
            "Per-setup profiles",
            Some("On its Details tab, each setup's rules decide which windows switch to which of its profiles.".into()),
            vec![
                row![
                    toggler(auto.enabled).label("Switch profiles automatically").on_toggle(Message::SetAutoSwitch),
                    space::horizontal(),
                    text("When no setup matches, use"),
                    dropdown(defaults, Some(default), Message::SetDefaultProfile).width(SETTING_WIDTH),
                ]
                .spacing(12)
                .align_y(Alignment::Center)
                .into(),
                text(how).size(13).color(MUTED_COLOR).into(),
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motion_rule_command_installs_the_shipped_rule() {
        let cmd = motion_rule_command();
        assert!(cmd.contains("ENV{ID_INPUT_ACCELEROMETER}==\"1\""), "{cmd}");
        assert!(cmd.contains("TAG+=\"uaccess\""), "{cmd}");
        // Single-quoted for the shell, so the rule itself must not contain a quote.
        let quoted = cmd.split('\'').nth(1).unwrap();
        assert!(!quoted.is_empty() && !quoted.contains('#'), "comments are left out: {quoted}");
        assert_eq!(cmd.matches('\'').count(), 2, "{cmd}");
    }
}
