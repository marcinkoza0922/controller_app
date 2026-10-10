//! The macro pages of Edit Controls: a macro's steps, which can be added to and removed. A step
//! row removes that step when chosen; the rows after the steps add to the macro.

use crate::{
    config::{Button, ButtonAction, Config, Game, ItemKind, Macro, MacroStep, Stick},
    menu::ItemView,
    pack::game_references,
};

use super::views::{add_item, item, remove_item};
use super::EditStep;

/// Hold time of a quick tap added to a macro, and the length of a quick wait.
const TAP_HOLD_MS: u64 = 50;
const WAIT_MS: u64 = 100;

/// What a step row adds to a macro, once its button is picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    Tap,
    Press,
    Release,
}

impl StepKind {
    /// The kind of an add row's offset, from `ADD_LABELS`.
    pub fn from_offset(offset: usize) -> Self {
        match offset {
            0 => StepKind::Tap,
            1 => StepKind::Press,
            _ => StepKind::Release,
        }
    }

    /// The step this kind makes for `b`.
    pub fn step(self, b: Button) -> MacroStep {
        let action = ButtonAction::Gamepad(b);
        match self {
            StepKind::Tap => MacroStep::Tap { action, hold_ms: TAP_HOLD_MS },
            StepKind::Press => MacroStep::Press(action),
            StepKind::Release => MacroStep::Release(action),
        }
    }
}

/// The rows after a macro's steps: a tap, press and release (each picks a button), a wait, and
/// removing the last step. The delete row comes after them.
const ADD_LABELS: [&str; 6] = ["Tap…", "Press…", "Release…", "Stick…", "Wait 100 ms", "Remove last step"];

/// Directions a stick step can point, in row order: up, up right, right, down right, down, down
/// left, left, up left and center. Each stick has one row per direction.
const DIRECTIONS: [(&str, f32, f32); 9] = [
    ("Up", 0.0, -1.0),
    ("Up Right", 0.7, -0.7),
    ("Right", 1.0, 0.0),
    ("Down Right", 0.7, 0.7),
    ("Down", 0.0, 1.0),
    ("Down Left", -0.7, 0.7),
    ("Left", -1.0, 0.0),
    ("Up Left", -0.7, -0.7),
    ("Center", 0.0, 0.0),
];

/// The stick step rows: the left stick's directions, then the right stick's.
pub const STICK_STEPS: usize = 2 * DIRECTIONS.len();

/// The stick step a row of the stick page makes.
pub fn stick_step(row: usize) -> MacroStep {
    let (stick, (_, x, y)) = if row < DIRECTIONS.len() {
        (Stick::Left, DIRECTIONS[row])
    } else {
        (Stick::Right, DIRECTIONS[row - DIRECTIONS.len()])
    };
    MacroStep::Stick { stick, x, y }
}

/// The label of a stick step row.
pub fn stick_row_label(row: usize) -> String {
    let (side, (name, _, _)) = if row < DIRECTIONS.len() {
        ("Left", DIRECTIONS[row])
    } else {
        ("Right", DIRECTIONS[row - DIRECTIONS.len()])
    };
    format!("{side} Stick {name}")
}

/// Adds `step` to the end of macro `i`.
pub fn push_step(config: &mut Config, i: usize, step: MacroStep) {
    if let Some(m) = game_macros_mut(config).and_then(|ms| ms.get_mut(i)) {
        m.steps.push(step);
    }
}

/// Which add row `row` of macro `i` is, counted from the first add row; `None` for a step row or
/// the delete row.
pub fn add_offset(config: &Config, i: usize, row: usize) -> Option<usize> {
    let steps = config.active_game().macros.get(i)?.steps.len();
    let offset = row.checked_sub(steps)?;
    (offset < ADD_LABELS.len()).then_some(offset)
}

pub fn macro_rows(config: &Config, i: usize) -> usize {
    config.active_game().macros.get(i).map_or(0, |m| m.steps.len()) + ADD_LABELS.len() + 1
}

/// Whether row `row` of macro `i` is its delete row.
pub fn is_delete_row(config: &Config, i: usize, row: usize) -> bool {
    row + 1 == macro_rows(config, i)
}

/// Whether a button, a layer or a menu of the active game uses macro `i`.
pub fn macro_in_use(config: &Config, i: usize) -> bool {
    let game = config.active_game();
    game.macros.get(i).is_some_and(|m| game_references(game).contains(&(ItemKind::Macro, m.name.clone())))
}

/// Deletes macro `i` unless something uses it. True when it was deleted.
pub fn delete_macro(config: &mut Config, i: usize) -> bool {
    if macro_in_use(config, i) {
        return false;
    }
    match game_macros_mut(config) {
        Some(macros) if i < macros.len() => {
            macros.remove(i);
            true
        }
        _ => false,
    }
}

pub fn macro_page(config: &Config, i: usize) -> (Vec<ItemView>, &'static str) {
    let Some(m) = config.active_game().macros.get(i) else {
        return (Vec::new(), "B back");
    };
    let delete_item = if macro_in_use(config, i) { item("Used by a button: can't delete".into()) } else { remove_item("Delete macro".into()) };
    let rows = m
        .steps
        .iter()
        .map(|s| item(step_label(s)))
        .chain(ADD_LABELS.iter().map(|l| if l.starts_with("Remove") { remove_item((*l).into()) } else { add_item((*l).into()) }))
        .chain(std::iter::once(delete_item))
        .collect();
    (rows, "A remove a step or add one · B back")
}

fn step_label(step: &MacroStep) -> String {
    match step {
        MacroStep::Tap { action, hold_ms } => format!("Tap {} ({hold_ms} ms)", action.summary()),
        MacroStep::Press(action) => format!("Press {}", action.summary()),
        MacroStep::Release(action) => format!("Release {}", action.summary()),
        MacroStep::Wait(ms) => format!("Wait {ms} ms"),
        MacroStep::Stick { stick, x, y } => format!("{} stick ({x:.1}, {y:.1})", if *stick == Stick::Left { "Left" } else { "Right" }),
    }
}

/// A choose on macro `i`'s `row`: a step row removes that step, an add row adds to the macro.
pub fn choose_macro_row(config: &mut Config, i: usize, row: usize) -> EditStep {
    let Some(m) = game_macros_mut(config).and_then(|ms| ms.get_mut(i)) else { return EditStep::Stay };
    if row < m.steps.len() {
        m.steps.remove(row);
        return EditStep::Changed;
    }
    match row - m.steps.len() {
        // Tap, press, release and stick pick their step on their own page.
        4 => m.steps.push(MacroStep::Wait(WAIT_MS)),
        5 => {
            m.steps.pop();
        }
        _ => {}
    }
    EditStep::Changed
}

/// Adds a macro with a name no other macro of the game has. Returns its index.
pub fn add_macro(config: &mut Config) -> Option<usize> {
    let macros = game_macros_mut(config)?;
    let name = (1..)
        .map(|n| format!("Macro {n}"))
        .find(|name| !macros.iter().any(|m| &m.name == name))
        .unwrap_or_default();
    macros.push(Macro { name, steps: Vec::new() });
    Some(macros.len() - 1)
}

/// The macros of the game the active profile is in (General's when there is no game).
fn game_macros_mut(config: &mut Config) -> Option<&mut Vec<Macro>> {
    let at = config.active_ref();
    config.game_mut(at.game.as_deref()).map(|g: &mut Game| &mut g.macros)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_macro_gets_steps_and_loses_them_again() {
        let mut config = Config::default();
        let i = add_macro(&mut config).unwrap();
        assert_eq!(config.active_game().macros[i].name, "Macro 1");
        // Add a tap of South (the first add row picks a button), then a wait (the fifth add row).
        push_step(&mut config, i, StepKind::Tap.step(Button::South));
        assert_eq!(choose_macro_row(&mut config, i, 1 + 4), EditStep::Changed);
        assert_eq!(config.active_game().macros[i].steps.len(), 2);
        assert!(matches!(config.active_game().macros[i].steps[1], MacroStep::Wait(WAIT_MS)));
        // A step row removes that step: the wait is row 1.
        choose_macro_row(&mut config, i, 1);
        assert_eq!(config.active_game().macros[i].steps.len(), 1);
    }

    #[test]
    fn new_macros_are_named_in_turn() {
        let mut config = Config::default();
        add_macro(&mut config);
        let second = add_macro(&mut config).unwrap();
        assert_eq!(config.active_game().macros[second].name, "Macro 2");
    }

    #[test]
    fn a_stick_step_points_a_stick_and_is_labeled() {
        let mut config = Config::default();
        let i = add_macro(&mut config).unwrap();
        push_step(&mut config, i, stick_step(STICK_STEPS / 2 + 6));
        assert_eq!(config.active_game().macros[i].steps[0], MacroStep::Stick { stick: Stick::Right, x: -1.0, y: 0.0 });
        assert_eq!(stick_row_label(2), "Left Stick Right");
    }
}

#[cfg(test)]
mod tone_tests {
    use super::*;
    use crate::menu::Tone;

    #[test]
    fn steps_are_plain_and_the_rows_that_add_or_remove_are_tinted() {
        let mut config = Config::default();
        let i = add_macro(&mut config).unwrap();
        push_step(&mut config, i, StepKind::Tap.step(Button::South));
        let (rows, _) = macro_page(&config, i);
        // One step, then the add rows, then the delete row.
        assert_eq!(rows[0].tone, Tone::Normal);
        assert_eq!(rows[1].tone, Tone::Add);
        assert_eq!(rows[1 + 4].tone, Tone::Add, "the wait adds");
        assert_eq!(rows[1 + 5].tone, Tone::Remove, "removing the last step removes");
        assert_eq!(rows.last().unwrap().tone, Tone::Remove, "deleting removes");
    }
}
