#![forbid(unsafe_code)]

mod engine;
mod order;
mod schema;

pub use engine::{
    CubeEngine, CubeEngineError, HistoryError, apply_turn_to_state, apply_turn_to_state_unchecked,
};
pub use order::{CubeOrder, CubeOrderError, MAX_CUBE_ORDER, MIN_CUBE_ORDER};
pub use schema::{
    all_face_permutations, rotate_x, rotate_y, rotate_z, CubeState, CubeStateParseError,
    CubeStateValidationError, Face, RotationAmount, StickerColor, TurnCommand,
    TurnCommandValidationError, CANONICAL_COLORS, CUBE_STATE_SCHEMA_VERSION,
};
