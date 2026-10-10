//! The debug window: a virtual controller to click on, and every model's drawing below it.

use std::{fmt, time::Duration};

use iced::{
    Alignment, Element, Point, Task,
    widget::{button, column, container, mouse_area, pick_list, row, scrollable, text},
};

use super::server::Session;
use crate::{
    config::{Button, Stick, Trigger},
    info::{Glyphs, PadFamily, PadModel},
    ipc::InputSnapshot,
    pad_svg::{self, Part},
    pad_widget::controller_drawing,
};

pub fn run(session: Session, socket: String) -> iced::Result {
    let app = iced::application(
        move || (Window { session: session.clone(), socket: socket.clone(), cursor: Point::ORIGIN, grab: None, problem: None }, Task::none()),
        Window::update,
        Window::view,
    )
    .title("Padwight debug")
    .theme(|_: &Window| iced::Theme::Dark)
    .subscription(|_: &Window| iced::time::every(Duration::from_millis(50)).map(|_| Message::Tick))
    .window(iced::window::Settings { size: iced::Size::new(1150.0, 900.0), icon: crate::icon::window(), ..Default::default() });
    crate::font::BUNDLED.iter().fold(app, |app, b| app.font(b.bytes)).run()
}

struct Window {
    session: Session,
    socket: String,
    /// The pointer over the virtual controller, in the drawing's coordinates.
    cursor: Point,
    /// What the pointer is holding.
    grab: Option<Grab>,
    /// Why the daemon didn't take the last change, when attached to it.
    problem: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum Grab {
    Button(Button),
    Stick(Stick),
    Trigger(Trigger),
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    Moved(Point),
    Down,
    Up,
    RightDown,
    Model(ModelChoice),
    Family(PadFamily),
    Reset,
}

/// A model in the picker; `None` is the generic controller.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ModelChoice(Option<PadModel>);

impl fmt::Display for ModelChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.map_or("Generic", PadModel::label))
    }
}

impl Window {
    #[expect(clippy::needless_pass_by_value, reason = "iced hands update its message by value")]
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                if self.session.quitting() {
                    return iced::exit();
                }
            }
            Message::Moved(p) => {
                self.cursor = p;
                self.drag();
            }
            Message::Down => self.grab_at_cursor(),
            Message::Up => self.let_go(),
            Message::RightDown => self.toggle_at_cursor(),
            Message::Model(ModelChoice(model)) => self.session.pad().set_model(model),
            Message::Family(family) => self.session.pad().family = family,
            Message::Reset => self.session.pad().reset(),
        }
        // Whatever changed reaches the daemon right away, when attached.
        self.problem = self.session.flush().err().map(|e| format!("{e:#}"));
        Task::none()
    }

    fn part_at_cursor(&self) -> Option<Part> {
        let model = self.session.pad().model;
        pad_svg::hit(model, self.cursor.x - pad_svg::MARGIN, self.cursor.y)
    }

    fn grab_at_cursor(&mut self) {
        self.let_go();
        let Some(part) = self.part_at_cursor() else { return };
        let mut pad = self.session.pad();
        self.grab = Some(match part {
            Part::Button(b) => {
                pad.press(b);
                Grab::Button(b)
            }
            Part::Stick(s) => Grab::Stick(s),
            Part::Trigger(t, _) => Grab::Trigger(t),
        });
        drop(pad);
        self.drag();
    }

    /// Moves a grabbed stick or trigger to the pointer.
    fn drag(&self) {
        let mut pad = self.session.pad();
        let (x, y) = (self.cursor.x - pad_svg::MARGIN, self.cursor.y);
        match self.grab {
            Some(Grab::Stick(s)) => {
                let value = pad_svg::stick_value(pad.model, s, x, y);
                pad.set_stick(s, value);
            }
            Some(Grab::Trigger(t)) => {
                let model = pad.model;
                pad.set_trigger(t, pad_svg::trigger_value(model, y));
            }
            _ => {}
        }
    }

    /// Lets go of what the pointer holds: buttons come up, sticks center, triggers release.
    fn let_go(&mut self) {
        let mut pad = self.session.pad();
        match self.grab.take() {
            Some(Grab::Button(b)) => pad.release(b),
            Some(Grab::Stick(s)) => pad.set_stick(s, (0.0, 0.0)),
            Some(Grab::Trigger(t)) => pad.set_trigger(t, 0.0),
            None => {}
        }
    }

    /// A right click keeps a part down: toggles a button, a stick's click or a trigger.
    fn toggle_at_cursor(&self) {
        let Some(part) = self.part_at_cursor() else { return };
        let mut pad = self.session.pad();
        match part {
            Part::Button(b) => pad.toggle(b),
            Part::Stick(Stick::Left) => pad.toggle(Button::LeftStick),
            Part::Stick(Stick::Right) => pad.toggle(Button::RightStick),
            Part::Trigger(t, _) => {
                let value = if pad.trigger(t) > 0.0 { 0.0 } else { 1.0 };
                pad.set_trigger(t, value);
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let pad = self.session.pad().clone();
        let snapshot = pad.snapshot();
        let glyphs = Glyphs { family: pad.family, nintendo_layout: false };

        let choices: Vec<ModelChoice> = std::iter::once(ModelChoice(None)).chain(PadModel::ALL.map(|m| ModelChoice(Some(m)))).collect();
        let controls = row![
            text("Virtual controller").size(20),
            pick_list(choices, Some(ModelChoice(pad.model)), Message::Model),
            pick_list(PadFamily::ALL, Some(pad.family), Message::Family),
            button("Release all").on_press(Message::Reset),
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let virtual_pad = mouse_area(controller_drawing(Some(&snapshot), pad.model, glyphs, &[], false))
            .on_move(Message::Moved)
            .on_press(Message::Down)
            .on_release(Message::Up)
            .on_exit(Message::Up)
            .on_right_press(Message::RightDown)
            .interaction(iced::mouse::Interaction::Pointer);

        let link = match (self.session.attached(), &self.problem) {
            (_, Some(problem)) => format!("Daemon: {problem}"),
            (Some(device), None) => format!("Attached to the daemon as {device}: the active profile maps this controller."),
            (None, None) => "Not attached to the daemon (padwight debug --attach).".to_string(),
        };
        let hint = text(format!(
            "Hold a button, or drag a stick or trigger. Right-click keeps a button, stick click or trigger held. \
             The same pad takes commands on {} (padwight debug help).",
            self.socket
        ))
        .size(12);

        let page = column![
            controls,
            hint,
            text(link).size(12),
            virtual_pad,
            text(readout(&snapshot)).size(13),
            text("Every model, showing the virtual controller's input").size(20),
            gallery(&snapshot),
        ]
        .spacing(12)
        .padding(16);
        scrollable(page).width(iced::Length::Fill).height(iced::Length::Fill).into()
    }
}

/// The state in words, for reading it without finding it on the drawing.
fn readout(s: &InputSnapshot) -> String {
    let held: Vec<String> = s.buttons.iter().map(|b| format!("{b:?}")).collect();
    let held = if held.is_empty() { "nothing held".to_string() } else { held.join(", ") };
    let mut line = format!(
        "{held}\nleft stick ({:.2}, {:.2})  right stick ({:.2}, {:.2})  triggers {:.2} / {:.2}",
        s.left_stick.0, s.left_stick.1, s.right_stick.0, s.right_stick.1, s.left_trigger, s.right_trigger
    );
    if let Some([p, y, r]) = s.gyro {
        line.push_str(&format!("  gyro {p:.0} / {y:.0} / {r:.0} °/s"));
    }
    line
}

/// Every model's drawing, each with the virtual controller's input.
fn gallery<'a>(input: &InputSnapshot) -> Element<'a, Message> {
    let cards = std::iter::once(None).chain(PadModel::ALL.map(Some)).map(|model| {
        let family = model.map_or(PadFamily::Xbox, PadModel::family);
        let shown = InputSnapshot { model, family: Some(family), ..input.clone() };
        let pick = button(text(ModelChoice(model).to_string()).size(13)).style(button::text).on_press(Message::Model(ModelChoice(model)));
        container(column![pick, controller_drawing(Some(&shown), model, Glyphs { family, nintendo_layout: false }, &[], false)].spacing(2))
            .into()
    });
    row(cards).spacing(12).wrap().vertical_spacing(12).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window() -> Window {
        Window { session: Session::default(), socket: String::new(), cursor: Point::ORIGIN, grab: None, problem: None }
    }

    fn move_to(w: &mut Window, x: f32, y: f32) {
        let _ = w.update(Message::Moved(Point::new(x + pad_svg::MARGIN, y)));
    }

    #[test]
    fn a_held_click_presses_and_letting_go_releases() {
        let mut w = window();
        let (x, y) = pad_svg::face_center(None, Button::South);
        move_to(&mut w, x, y);
        let _ = w.update(Message::Down);
        assert!(w.session.pad().is_pressed(Button::South));
        let _ = w.update(Message::Up);
        assert!(!w.session.pad().is_pressed(Button::South));
    }

    #[test]
    fn dragging_a_stick_moves_it_and_it_centers_on_release() {
        let mut w = window();
        let (x, y) = pad_svg::stick_center(None, Stick::Left);
        move_to(&mut w, x, y);
        let _ = w.update(Message::Down);
        move_to(&mut w, x + 100.0, y);
        assert_eq!(w.session.pad().stick(Stick::Left), (1.0, 0.0));
        let _ = w.update(Message::Up);
        assert_eq!(w.session.pad().stick(Stick::Left), (0.0, 0.0));
    }

    #[test]
    fn right_click_keeps_a_button_down() {
        let mut w = window();
        let (x, y) = pad_svg::face_center(None, Button::East);
        move_to(&mut w, x, y);
        let _ = w.update(Message::RightDown);
        assert!(w.session.pad().is_pressed(Button::East));
        let _ = w.update(Message::RightDown);
        assert!(!w.session.pad().is_pressed(Button::East));
    }
}
