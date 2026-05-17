use core::fmt;

use crate::{
    CubeOrder, CubeState, CubeStateValidationError, Face, RotationAmount, TurnCommand,
    StickerColor, TurnCommandValidationError,
};

#[derive(Debug, Clone)]
pub struct CubeEngine {
    initial_state: CubeState,
    current_state: CubeState,
    turn_history: Vec<TurnCommand>,
    redo_stack: Vec<TurnCommand>,
}

impl CubeEngine {
    pub fn new(order: CubeOrder) -> Self {
        Self::from_state(CubeState::solved(order)).expect("solved state should be valid")
    }

    pub fn from_state(state: CubeState) -> Result<Self, CubeEngineError> {
        state.validate().map_err(CubeEngineError::InvalidState)?;

        Ok(Self {
            initial_state: state.clone(),
            current_state: state,
            turn_history: Vec::new(),
            redo_stack: Vec::new(),
        })
    }

    pub fn state(&self) -> &CubeState {
        &self.current_state
    }

    pub fn order(&self) -> CubeOrder {
        self.current_state.order
    }

    pub fn turn_history(&self) -> &[TurnCommand] {
        &self.turn_history
    }

    pub fn redo_depth(&self) -> usize {
        self.redo_stack.len()
    }

    pub fn move_count(&self) -> usize {
        self.turn_history.len()
    }

    pub fn is_solved(&self) -> bool {
        self.current_state == CubeState::solved(self.current_state.order)
    }

    pub fn apply_turn(&mut self, turn: TurnCommand) -> Result<(), CubeEngineError> {
        apply_turn_to_state(&mut self.current_state, turn)?;
        self.turn_history.push(turn);
        self.redo_stack.clear();
        Ok(())
    }

    pub fn apply_turns<I>(&mut self, turns: I) -> Result<(), CubeEngineError>
    where
        I: IntoIterator<Item = TurnCommand>,
    {
        for turn in turns {
            self.apply_turn(turn)?;
        }

        Ok(())
    }

    pub fn undo(&mut self) -> Result<TurnCommand, HistoryError> {
        let Some(turn) = self.turn_history.pop() else {
            return Err(HistoryError::NothingToUndo);
        };

        apply_turn_to_state(&mut self.current_state, turn.inverse())
            .map_err(HistoryError::Engine)?;
        self.redo_stack.push(turn);
        Ok(turn)
    }

    pub fn redo(&mut self) -> Result<TurnCommand, HistoryError> {
        let Some(turn) = self.redo_stack.pop() else {
            return Err(HistoryError::NothingToRedo);
        };

        apply_turn_to_state(&mut self.current_state, turn).map_err(HistoryError::Engine)?;
        self.turn_history.push(turn);
        Ok(turn)
    }

    pub fn reset(&mut self) {
        self.current_state = self.initial_state.clone();
        self.turn_history.clear();
        self.redo_stack.clear();
    }

    pub fn scramble_with_seed(
        &mut self,
        length: usize,
        seed: u64,
    ) -> Result<Vec<TurnCommand>, CubeEngineError> {
        let scramble = generate_scramble(self.current_state.order, length, seed);
        self.apply_turns(scramble.iter().copied())?;
        Ok(scramble)
    }
}

pub fn generate_scramble(_order: CubeOrder, length: usize, seed: u64) -> Vec<TurnCommand> {
    let mut rng = DeterministicRng::new(seed);
    let mut scramble = Vec::with_capacity(length);
    let mut last_face = None;

    while scramble.len() < length {
        let face = Face::ALL[rng.range(Face::ALL.len())];
        if Some(face) == last_face {
            continue;
        }

        let rotation = match rng.range(3) {
            0 => RotationAmount::Clockwise,
            1 => RotationAmount::HalfTurn,
            _ => RotationAmount::CounterClockwise,
        };

        scramble.push(TurnCommand::outer(face, rotation));
        last_face = Some(face);
    }

    scramble
}

pub fn apply_turn_to_state(state: &mut CubeState, turn: TurnCommand) -> Result<(), CubeEngineError> {
    let mut scratch = state.stickers.clone();
    apply_turn_to_state_unchecked(state, turn, &mut scratch).map_err(CubeEngineError::InvalidTurn)?;
    state.validate().map_err(CubeEngineError::InvalidState)?;
    Ok(())
}

pub fn apply_turn_to_state_unchecked(
    state: &mut CubeState,
    turn: TurnCommand,
    scratch: &mut [StickerColor],
) -> Result<(), TurnCommandValidationError> {
    turn.validate_for(state.order)?;
    let order = usize::from(state.order.get());
    apply_turn_to_items_with_scratch(&mut state.stickers, scratch, order, turn);
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CubeEngineError {
    InvalidState(CubeStateValidationError),
    InvalidTurn(TurnCommandValidationError),
}

impl fmt::Display for CubeEngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidState(error) => write!(f, "{error}"),
            Self::InvalidTurn(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for CubeEngineError {}

#[derive(Debug)]
pub enum HistoryError {
    Engine(CubeEngineError),
    NothingToUndo,
    NothingToRedo,
}

impl fmt::Display for HistoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Engine(error) => write!(f, "{error}"),
            Self::NothingToUndo => write!(f, "there is no turn to undo"),
            Self::NothingToRedo => write!(f, "there is no turn to redo"),
        }
    }
}

impl std::error::Error for HistoryError {}

#[derive(Debug, Clone, Copy)]
struct StickerPosition {
    x: usize,
    y: usize,
    z: usize,
    face: Face,
}

impl StickerPosition {
    fn from_index(order: usize, index: usize) -> Self {
        let face_len = order * order;
        let face = Face::ALL[index / face_len];
        let local = index % face_len;
        let row = local / order;
        let col = local % order;

        match face {
            Face::Up => Self {
                x: col,
                y: order - 1,
                z: row,
                face,
            },
            Face::Right => Self {
                x: order - 1,
                y: order - 1 - row,
                z: order - 1 - col,
                face,
            },
            Face::Front => Self {
                x: col,
                y: order - 1 - row,
                z: order - 1,
                face,
            },
            Face::Down => Self {
                x: col,
                y: 0,
                z: order - 1 - row,
                face,
            },
            Face::Left => Self {
                x: 0,
                y: order - 1 - row,
                z: col,
                face,
            },
            Face::Back => Self {
                x: order - 1 - col,
                y: order - 1 - row,
                z: 0,
                face,
            },
        }
    }

    fn to_index(self, order: usize) -> usize {
        let face_offset = match self.face {
            Face::Up => 0,
            Face::Right => 1,
            Face::Front => 2,
            Face::Down => 3,
            Face::Left => 4,
            Face::Back => 5,
        } * order
            * order;

        let (row, col) = match self.face {
            Face::Up => (self.z, self.x),
            Face::Right => (order - 1 - self.y, order - 1 - self.z),
            Face::Front => (order - 1 - self.y, self.x),
            Face::Down => (order - 1 - self.z, self.x),
            Face::Left => (order - 1 - self.y, self.z),
            Face::Back => (order - 1 - self.y, order - 1 - self.x),
        };

        face_offset + (row * order) + col
    }
}

fn is_affected(sticker: StickerPosition, turn: TurnCommand, order: usize) -> bool {
    let depth = match turn.face {
        Face::Up => (order - 1) - sticker.y,
        Face::Right => (order - 1) - sticker.x,
        Face::Front => (order - 1) - sticker.z,
        Face::Down => sticker.y,
        Face::Left => sticker.x,
        Face::Back => sticker.z,
    };

    let start = usize::from(turn.start_layer);
    let end = start + usize::from(turn.width);
    depth >= start && depth < end
}

fn rotate_sticker(mut sticker: StickerPosition, turn: TurnCommand, order: usize) -> StickerPosition {
    for _ in 0..quarter_turns(turn.rotation) {
        sticker = rotate_clockwise(sticker, turn.face, order);
    }

    sticker
}

fn rotate_clockwise(sticker: StickerPosition, face: Face, order: usize) -> StickerPosition {
    let max = order - 1;

    match face {
        Face::Front => StickerPosition {
            x: sticker.y,
            y: max - sticker.x,
            z: sticker.z,
            face: rotate_face_clockwise(sticker.face, face),
        },
        Face::Back => StickerPosition {
            x: max - sticker.y,
            y: sticker.x,
            z: sticker.z,
            face: rotate_face_clockwise(sticker.face, face),
        },
        Face::Right => StickerPosition {
            x: sticker.x,
            y: sticker.z,
            z: max - sticker.y,
            face: rotate_face_clockwise(sticker.face, face),
        },
        Face::Left => StickerPosition {
            x: sticker.x,
            y: max - sticker.z,
            z: sticker.y,
            face: rotate_face_clockwise(sticker.face, face),
        },
        Face::Up => StickerPosition {
            x: max - sticker.z,
            y: sticker.y,
            z: sticker.x,
            face: rotate_face_clockwise(sticker.face, face),
        },
        Face::Down => StickerPosition {
            x: sticker.z,
            y: sticker.y,
            z: max - sticker.x,
            face: rotate_face_clockwise(sticker.face, face),
        },
    }
}

fn rotate_face_clockwise(face: Face, axis_face: Face) -> Face {
    match axis_face {
        Face::Front => match face {
            Face::Up => Face::Right,
            Face::Right => Face::Down,
            Face::Down => Face::Left,
            Face::Left => Face::Up,
            other => other,
        },
        Face::Back => match face {
            Face::Up => Face::Left,
            Face::Left => Face::Down,
            Face::Down => Face::Right,
            Face::Right => Face::Up,
            other => other,
        },
        Face::Right => match face {
            Face::Up => Face::Back,
            Face::Back => Face::Down,
            Face::Down => Face::Front,
            Face::Front => Face::Up,
            other => other,
        },
        Face::Left => match face {
            Face::Up => Face::Front,
            Face::Front => Face::Down,
            Face::Down => Face::Back,
            Face::Back => Face::Up,
            other => other,
        },
        Face::Up => match face {
            Face::Front => Face::Left,
            Face::Left => Face::Back,
            Face::Back => Face::Right,
            Face::Right => Face::Front,
            other => other,
        },
        Face::Down => match face {
            Face::Front => Face::Right,
            Face::Right => Face::Back,
            Face::Back => Face::Left,
            Face::Left => Face::Front,
            other => other,
        },
    }
}

fn quarter_turns(rotation: RotationAmount) -> usize {
    match rotation {
        RotationAmount::Clockwise => 1,
        RotationAmount::HalfTurn => 2,
        RotationAmount::CounterClockwise => 3,
    }
}

#[derive(Debug, Clone)]
struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    fn new(seed: u64) -> Self {
        let state = if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        };

        Self { state }
    }

    fn next_u64(&mut self) -> u64 {
        let mut value = self.state;
        value ^= value >> 12;
        value ^= value << 25;
        value ^= value >> 27;
        self.state = value;
        value.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn range(&mut self, upper_bound: usize) -> usize {
        debug_assert!(upper_bound > 0);
        (self.next_u64() % upper_bound as u64) as usize
    }
}

fn apply_turn_to_items_with_scratch<T: Copy>(
    items: &mut [T],
    scratch: &mut [T],
    order: usize,
    turn: TurnCommand,
) {
    assert_eq!(
        items.len(),
        scratch.len(),
        "scratch buffer must match the number of stickers"
    );
    scratch.copy_from_slice(items);

    for (index, item) in scratch.iter().copied().enumerate() {
        let sticker = StickerPosition::from_index(order, index);
        let moved = if is_affected(sticker, turn, order) {
            rotate_sticker(sticker, turn, order)
        } else {
            sticker
        };
        items[moved.to_index(order)] = item;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CubeEngine, HistoryError, apply_turn_to_items_with_scratch, apply_turn_to_state,
        generate_scramble,
    };
    use crate::{CubeOrder, CubeState, Face, RotationAmount, TurnCommand};

    #[test]
    fn turn_followed_by_its_inverse_returns_to_solved() {
        let mut state = CubeState::solved(CubeOrder::standard());
        let turn = TurnCommand::outer(Face::Front, RotationAmount::Clockwise);

        apply_turn_to_state(&mut state, turn).expect("turn should apply");
        apply_turn_to_state(&mut state, turn.inverse()).expect("inverse should apply");

        assert_eq!(state, CubeState::solved(CubeOrder::standard()));
    }

    #[test]
    fn four_quarter_turns_cycle_back_to_solved() {
        let mut state = CubeState::solved(CubeOrder::standard());
        let turn = TurnCommand::outer(Face::Right, RotationAmount::Clockwise);

        for _ in 0..4 {
            apply_turn_to_state(&mut state, turn).expect("turn should apply");
        }

        assert_eq!(state, CubeState::solved(CubeOrder::standard()));
    }

    #[test]
    fn front_turn_moves_the_left_strip_to_the_up_face_in_view_order() {
        let mut labels: Vec<usize> = (0..54).collect();
        let mut scratch = labels.clone();
        apply_turn_to_items_with_scratch(
            &mut labels,
            &mut scratch,
            3,
            TurnCommand::outer(Face::Front, RotationAmount::Clockwise),
        );

        assert_eq!(&labels[6..9], &[44, 41, 38]);
    }

    #[test]
    fn up_turn_moves_the_front_strip_to_the_left_face_in_view_order() {
        let mut labels: Vec<usize> = (0..54).collect();
        let mut scratch = labels.clone();
        apply_turn_to_items_with_scratch(
            &mut labels,
            &mut scratch,
            3,
            TurnCommand::outer(Face::Up, RotationAmount::Clockwise),
        );

        assert_eq!(&labels[36..39], &[18, 19, 20]);
    }

    #[test]
    fn down_turn_moves_the_front_strip_to_the_right_face_in_view_order() {
        let mut labels: Vec<usize> = (0..54).collect();
        let mut scratch = labels.clone();
        apply_turn_to_items_with_scratch(
            &mut labels,
            &mut scratch,
            3,
            TurnCommand::outer(Face::Down, RotationAmount::Clockwise),
        );

        assert_eq!(&labels[15..18], &[24, 25, 26]);
    }

    #[test]
    fn wide_turns_are_supported_for_larger_orders() {
        let order = CubeOrder::new(4).expect("4x4 should be valid");
        let mut state = CubeState::solved(order);
        let turn = TurnCommand {
            face: Face::Up,
            start_layer: 0,
            width: 2,
            rotation: RotationAmount::CounterClockwise,
        };

        apply_turn_to_state(&mut state, turn).expect("wide turn should apply");
        apply_turn_to_state(&mut state, turn.inverse()).expect("inverse should apply");

        assert_eq!(state, CubeState::solved(order));
    }

    #[test]
    fn undo_and_redo_restore_the_expected_state() {
        let mut engine = CubeEngine::new(CubeOrder::standard());
        let turn = TurnCommand::outer(Face::Up, RotationAmount::HalfTurn);

        engine.apply_turn(turn).expect("turn should apply");
        let after_turn = engine.state().clone();

        engine.undo().expect("undo should be available");
        assert_eq!(engine.state(), &CubeState::solved(CubeOrder::standard()));

        engine.redo().expect("redo should be available");
        assert_eq!(engine.state(), &after_turn);
        assert_eq!(engine.move_count(), 1);
    }

    #[test]
    fn reset_clears_history_and_restores_the_initial_state() {
        let mut engine = CubeEngine::new(CubeOrder::standard());
        engine
            .apply_turn(TurnCommand::outer(Face::Left, RotationAmount::Clockwise))
            .expect("turn should apply");

        engine.reset();

        assert_eq!(engine.state(), &CubeState::solved(CubeOrder::standard()));
        assert_eq!(engine.move_count(), 0);
        assert_eq!(engine.redo_depth(), 0);
    }

    #[test]
    fn scramble_generation_is_deterministic_for_a_seed() {
        let order = CubeOrder::standard();
        let a = generate_scramble(order, 12, 7);
        let b = generate_scramble(order, 12, 7);

        assert_eq!(a, b);
        assert_eq!(a.len(), 12);
    }

    #[test]
    fn sexy_move_six_times_cycles_back_to_solved() {
        let mut state = CubeState::solved(CubeOrder::standard());
        let algorithm = [
            TurnCommand::outer(Face::Right, RotationAmount::Clockwise),
            TurnCommand::outer(Face::Up, RotationAmount::Clockwise),
            TurnCommand::outer(Face::Right, RotationAmount::CounterClockwise),
            TurnCommand::outer(Face::Up, RotationAmount::CounterClockwise),
        ];

        for _ in 0..6 {
            for turn in algorithm {
                apply_turn_to_state(&mut state, turn).expect("turn should be valid");
            }
        }

        assert_eq!(state, CubeState::solved(CubeOrder::standard()));
    }

    #[test]
    fn scramble_application_updates_history() {
        let mut engine = CubeEngine::new(CubeOrder::standard());
        let scramble = engine
            .scramble_with_seed(8, 99)
            .expect("scramble should apply");

        assert_eq!(engine.move_count(), 8);
        assert_eq!(engine.turn_history(), scramble.as_slice());
    }

    #[test]
    fn undo_reports_when_history_is_empty() {
        let mut engine = CubeEngine::new(CubeOrder::standard());
        let error = engine.undo().expect_err("undo should fail");

        assert!(matches!(error, HistoryError::NothingToUndo));
    }
}
