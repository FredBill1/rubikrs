use core::fmt;

use serde::{Deserialize, Serialize};

use crate::CubeOrder;

pub const CUBE_STATE_SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Face {
    Up,
    Right,
    Front,
    Down,
    Left,
    Back,
}

impl Face {
    pub const ALL: [Self; 6] = [
        Self::Up,
        Self::Right,
        Self::Front,
        Self::Down,
        Self::Left,
        Self::Back,
    ];

    pub const fn solved_color(self) -> StickerColor {
        match self {
            Self::Up => StickerColor::White,
            Self::Right => StickerColor::Red,
            Self::Front => StickerColor::Green,
            Self::Down => StickerColor::Yellow,
            Self::Left => StickerColor::Orange,
            Self::Back => StickerColor::Blue,
        }
    }

    pub const fn opposite(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Right => Self::Left,
            Self::Front => Self::Back,
            Self::Down => Self::Up,
            Self::Left => Self::Right,
            Self::Back => Self::Front,
        }
    }

    pub const fn direction(self) -> (i32, i32, i32) {
        match self {
            Self::Up => (0, 1, 0),
            Self::Right => (1, 0, 0),
            Self::Front => (0, 0, 1),
            Self::Down => (0, -1, 0),
            Self::Left => (-1, 0, 0),
            Self::Back => (0, 0, -1),
        }
    }

    pub fn from_solved_color(color: StickerColor) -> Self {
        match color {
            StickerColor::White => Self::Up,
            StickerColor::Red => Self::Right,
            StickerColor::Green => Self::Front,
            StickerColor::Yellow => Self::Down,
            StickerColor::Orange => Self::Left,
            StickerColor::Blue => Self::Back,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StickerColor {
    White,
    Red,
    Green,
    Yellow,
    Orange,
    Blue,
}

impl StickerColor {
    pub const ALL: [Self; 6] = [
        Self::White,
        Self::Red,
        Self::Green,
        Self::Yellow,
        Self::Orange,
        Self::Blue,
    ];

    const fn index(self) -> usize {
        match self {
            Self::White => 0,
            Self::Red => 1,
            Self::Green => 2,
            Self::Yellow => 3,
            Self::Orange => 4,
            Self::Blue => 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RotationAmount {
    Clockwise,
    HalfTurn,
    CounterClockwise,
}

impl RotationAmount {
    pub const fn inverse(self) -> Self {
        match self {
            Self::Clockwise => Self::CounterClockwise,
            Self::HalfTurn => Self::HalfTurn,
            Self::CounterClockwise => Self::Clockwise,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnCommand {
    pub face: Face,
    pub start_layer: u32,
    pub width: u32,
    pub rotation: RotationAmount,
}

impl TurnCommand {
    pub const fn outer(face: Face, rotation: RotationAmount) -> Self {
        Self {
            face,
            start_layer: 0,
            width: 1,
            rotation,
        }
    }

    pub const fn inverse(self) -> Self {
        Self {
            face: self.face,
            start_layer: self.start_layer,
            width: self.width,
            rotation: self.rotation.inverse(),
        }
    }

    pub fn validate_for(&self, order: CubeOrder) -> Result<(), TurnCommandValidationError> {
        if self.width == 0 {
            return Err(TurnCommandValidationError::ZeroWidth);
        }

        let end = self.start_layer as usize + self.width as usize;
        if end > order.get() as usize {
            return Err(TurnCommandValidationError::OutOfBounds {
                order: order.get(),
                start_layer: self.start_layer,
                width: self.width,
            });
        }

        if self.start_layer == 0 && self.width == order.get() {
            return Err(TurnCommandValidationError::WholeCubeRotationUnsupported {
                order: order.get(),
            });
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CubeState {
    pub version: u8,
    pub order: CubeOrder,
    /// Stickers are stored in canonical `U, R, F, D, L, B` face order.
    ///
    /// Within each face, stickers are laid out in row-major order as viewed
    /// directly from outside that face: top-left to bottom-right.
    pub stickers: Vec<StickerColor>,
}

impl CubeState {
    pub fn solved(order: CubeOrder) -> Self {
        let stickers_per_face = order.get() as usize * order.get() as usize;
        let stickers = Face::ALL
            .into_iter()
            .flat_map(|face| core::iter::repeat_n(face.solved_color(), stickers_per_face))
            .collect();

        Self {
            version: CUBE_STATE_SCHEMA_VERSION,
            order,
            stickers,
        }
    }

    /// Checks whether the cube is solved, regardless of its overall
    /// orientation.
    ///
    /// A cube is solved if every face is uniformly colored, the six face
    /// colors are distinct, opposite face pairs are preserved, and the
    /// orientation is right-handed (R × B == U).
    pub fn is_solved(&self) -> bool {
        let order = self.order.get() as usize;
        let face_size = order * order;

        // Determine each face's color from its first sticker.
        let face_colors: [StickerColor; 6] = {
            let mut colors = [StickerColor::White; 6];
            for (i, color) in colors.iter_mut().enumerate() {
                *color = self.stickers[i * face_size];
            }
            colors
        };

        // Check each face is uniformly colored.
        for face_idx in 0..6usize {
            let face_start = face_idx * face_size;
            let expected = self.stickers[face_start];
            for i in 1..face_size {
                if self.stickers[face_start + i] != expected {
                    return false;
                }
            }
        }

        // Check all six face colors are distinct.
        {
            let mut seen = [false; 6];
            for &color in face_colors.iter() {
                let idx = color.index();
                if seen[idx] {
                    return false;
                }
                seen[idx] = true;
            }
        }

        // Map each physical face to the canonical face whose solved color it
        // currently displays.
        let cf: [Face; 6] = face_colors.map(Face::from_solved_color);

        // Opposite pairs must be preserved.
        if cf[0].opposite() != cf[3] {
            return false;
        } // U ↔ D
        if cf[1].opposite() != cf[4] {
            return false;
        } // R ↔ L
        if cf[2].opposite() != cf[5] {
            return false;
        } // F ↔ B

        // Right-handed orientation: R × B == U
        let (rx, ry, rz) = cf[1].direction();
        let (bx, by, bz) = cf[5].direction();
        let cross = (ry * bz - rz * by, rz * bx - rx * bz, rx * by - ry * bx);
        cross == cf[0].direction()
    }

    pub fn validate(&self) -> Result<(), CubeStateValidationError> {
        if self.version != CUBE_STATE_SCHEMA_VERSION {
            return Err(CubeStateValidationError::UnsupportedVersion {
                expected: CUBE_STATE_SCHEMA_VERSION,
                actual: self.version,
            });
        }

        let stickers_per_face = self.order.get() as usize * self.order.get() as usize;
        let expected_len = stickers_per_face * Face::ALL.len();
        if self.stickers.len() != expected_len {
            return Err(CubeStateValidationError::UnexpectedStickerCount {
                expected: expected_len,
                actual: self.stickers.len(),
            });
        }

        let mut counts = [0usize; 6];
        for color in &self.stickers {
            counts[color.index()] += 1;
        }

        for color in StickerColor::ALL {
            let actual = counts[color.index()];
            if actual != stickers_per_face {
                return Err(CubeStateValidationError::UnexpectedColorCount {
                    color,
                    expected: stickers_per_face,
                    actual,
                });
            }
        }

        Ok(())
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(input: &str) -> Result<Self, CubeStateParseError> {
        let state: Self = serde_json::from_str(input).map_err(CubeStateParseError::Json)?;
        state.validate().map_err(CubeStateParseError::Invalid)?;
        Ok(state)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CubeStateValidationError {
    UnsupportedVersion {
        expected: u8,
        actual: u8,
    },
    UnexpectedStickerCount {
        expected: usize,
        actual: usize,
    },
    UnexpectedColorCount {
        color: StickerColor,
        expected: usize,
        actual: usize,
    },
}

impl fmt::Display for CubeStateValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion { expected, actual } => {
                write!(
                    f,
                    "state schema version {actual} is unsupported; expected version {expected}"
                )
            }
            Self::UnexpectedStickerCount { expected, actual } => {
                write!(
                    f,
                    "state contains {actual} stickers but {expected} were expected"
                )
            }
            Self::UnexpectedColorCount {
                color,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "state contains {actual} stickers of color {:?} but {expected} were expected",
                    color
                )
            }
        }
    }
}

impl std::error::Error for CubeStateValidationError {}

#[derive(Debug)]
pub enum CubeStateParseError {
    Json(serde_json::Error),
    Invalid(CubeStateValidationError),
}

impl fmt::Display for CubeStateParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(f, "failed to parse cube state JSON: {error}"),
            Self::Invalid(error) => write!(f, "parsed cube state is invalid: {error}"),
        }
    }
}

impl std::error::Error for CubeStateParseError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnCommandValidationError {
    ZeroWidth,
    OutOfBounds {
        order: u32,
        start_layer: u32,
        width: u32,
    },
    WholeCubeRotationUnsupported {
        order: u32,
    },
}

impl fmt::Display for TurnCommandValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroWidth => write!(f, "turn width must be at least one layer"),
            Self::OutOfBounds {
                order,
                start_layer,
                width,
            } => write!(
                f,
                "turn range start={} width={} exceeds cube order {}",
                start_layer, width, order
            ),
            Self::WholeCubeRotationUnsupported { order } => write!(
                f,
                "whole-cube rotations are not represented as face turns; width cannot equal order {}",
                order
            ),
        }
    }
}

impl std::error::Error for TurnCommandValidationError {}

// ---------------------------------------------------------------------------
// Whole-cube rotation helpers (face-permutation space)
// ---------------------------------------------------------------------------

/// X-axis rotation (around R-L): U→B, B→D, D→F, F→U, R/L stay.
pub fn rotate_x(perm: [usize; 6]) -> [usize; 6] {
    [perm[5], perm[1], perm[0], perm[2], perm[4], perm[3]]
}

/// Y-axis rotation (around U-D): F→R, R→B, B→L, L→F, U/D stay.
pub fn rotate_y(perm: [usize; 6]) -> [usize; 6] {
    [perm[0], perm[2], perm[4], perm[3], perm[5], perm[1]]
}

/// Z-axis rotation (around F-B): U→R, R→D, D→L, L→U, F/B stay.
pub fn rotate_z(perm: [usize; 6]) -> [usize; 6] {
    [perm[4], perm[0], perm[2], perm[1], perm[3], perm[5]]
}

/// Generate all 24 valid face permutations reachable from identity via X, Y, Z
/// rotations.
pub fn all_face_permutations() -> Vec<[usize; 6]> {
    let identity = [0usize, 1, 2, 3, 4, 5];
    let mut seen = std::collections::BTreeSet::new();
    let mut queue = vec![identity];
    seen.insert(identity);

    let mut i = 0;
    while i < queue.len() {
        let perm = queue[i];
        i += 1;
        for next in [rotate_x(perm), rotate_y(perm), rotate_z(perm)] {
            if seen.insert(next) {
                queue.push(next);
            }
        }
    }
    queue
}

/// Canonical solved colors in face-index order: U=White, R=Red, F=Green,
/// D=Yellow, L=Orange, B=Blue.
pub const CANONICAL_COLORS: [StickerColor; 6] = [
    StickerColor::White,
    StickerColor::Red,
    StickerColor::Green,
    StickerColor::Yellow,
    StickerColor::Orange,
    StickerColor::Blue,
];

#[cfg(test)]
mod tests {
    use super::{
        CUBE_STATE_SCHEMA_VERSION, CubeState, CubeStateParseError, CubeStateValidationError, Face,
        RotationAmount, StickerColor, TurnCommand, TurnCommandValidationError,
    };
    use crate::{apply_turn_to_state, CubeOrder};

    #[test]
    fn solved_state_uses_canonical_face_order_and_counts() {
        let state = CubeState::solved(CubeOrder::new(3).expect("3x3 should be valid"));
        state.validate().expect("solved state should be valid");

        assert_eq!(state.version, CUBE_STATE_SCHEMA_VERSION);
        assert_eq!(state.stickers.len(), 54);
        assert_eq!(state.stickers[0], StickerColor::White);
        assert_eq!(state.stickers[9], StickerColor::Red);
        assert_eq!(state.stickers[18], StickerColor::Green);
        assert_eq!(state.stickers[27], StickerColor::Yellow);
        assert_eq!(state.stickers[36], StickerColor::Orange);
        assert_eq!(state.stickers[45], StickerColor::Blue);
    }

    #[test]
    fn clockwise_rotation_has_an_inverse() {
        let turn = TurnCommand::outer(Face::Front, RotationAmount::Clockwise);
        assert_eq!(turn.inverse().rotation, RotationAmount::CounterClockwise);
        assert_eq!(turn.inverse().inverse(), turn);
    }

    #[test]
    fn state_json_round_trip_preserves_the_schema() {
        let order = CubeOrder::new(4).expect("4x4 should be valid");
        let state = CubeState::solved(order);
        let json = state.to_json().expect("state should serialize");
        let decoded = CubeState::from_json(&json).expect("state should round-trip");

        assert_eq!(decoded, state);
    }

    #[test]
    fn invalid_sticker_count_is_rejected() {
        let mut state = CubeState::solved(CubeOrder::standard());
        state.stickers.pop();

        let error = state.validate().expect_err("state should be rejected");
        assert_eq!(
            error,
            CubeStateValidationError::UnexpectedStickerCount {
                expected: 54,
                actual: 53,
            }
        );
    }

    #[test]
    fn invalid_color_distribution_is_rejected() {
        let mut state = CubeState::solved(CubeOrder::standard());
        state.stickers[0] = StickerColor::Red;

        let error = state.validate().expect_err("state should be rejected");
        assert_eq!(
            error,
            CubeStateValidationError::UnexpectedColorCount {
                color: StickerColor::White,
                expected: 9,
                actual: 8,
            }
        );
    }

    #[test]
    fn invalid_json_state_is_reported_after_parsing() {
        let json = r#"{"version":1,"order":3,"stickers":["white"]}"#;
        let error = CubeState::from_json(json).expect_err("state should be rejected");

        assert!(matches!(
            error,
            CubeStateParseError::Invalid(CubeStateValidationError::UnexpectedStickerCount {
                expected: 54,
                actual: 1,
            })
        ));
    }

    #[test]
    fn outer_turn_is_valid_for_supported_orders() {
        let order = CubeOrder::standard();
        TurnCommand::outer(Face::Front, RotationAmount::Clockwise)
            .validate_for(order)
            .expect("outer turn should be valid");
    }

    #[test]
    fn turn_ranges_cannot_exceed_the_cube_order() {
        let order = CubeOrder::new(4).expect("4x4 should be valid");
        let turn = TurnCommand {
            face: Face::Right,
            start_layer: 3,
            width: 2,
            rotation: RotationAmount::HalfTurn,
        };

        let error = turn
            .validate_for(order)
            .expect_err("turn should be rejected");
        assert_eq!(
            error,
            TurnCommandValidationError::OutOfBounds {
                order: 4,
                start_layer: 3,
                width: 2,
            }
        );
    }

    #[test]
    fn whole_cube_rotations_are_rejected_from_face_turn_schema() {
        let order = CubeOrder::standard();
        let turn = TurnCommand {
            face: Face::Up,
            start_layer: 0,
            width: order.get(),
            rotation: RotationAmount::Clockwise,
        };

        let error = turn
            .validate_for(order)
            .expect_err("whole cube rotations should be rejected");
        assert_eq!(
            error,
            TurnCommandValidationError::WholeCubeRotationUnsupported { order: 3 }
        );
    }

    // -----------------------------------------------------------------------
    // is_solved tests
    // -----------------------------------------------------------------------

    fn make_rotated_solved(order: u32, face_perm: &[usize; 6]) -> CubeState {
        let order_usize = order as usize;
        let face_size = order_usize * order_usize;
        let mut stickers = Vec::with_capacity(6 * face_size);
        for i in 0..6 {
            let color = super::CANONICAL_COLORS[face_perm[i]];
            stickers.extend(core::iter::repeat_n(color, face_size));
        }
        CubeState {
            version: CUBE_STATE_SCHEMA_VERSION,
            order: CubeOrder::new(order).expect("valid order"),
            stickers,
        }
    }

    #[test]
    fn all_24_orientations_produce_24_perms() {
        let perms = super::all_face_permutations();
        assert_eq!(perms.len(), 24, "there are exactly 24 cube orientations");
    }

    #[test]
    fn is_solved_accepts_all_24_orientations_3x3() {
        for perm in super::all_face_permutations() {
            let state = make_rotated_solved(3, &perm);
            state.validate().expect("rotated solved state should be valid");
            assert!(
                state.is_solved(),
                "3x3 should be solved for perm {perm:?}"
            );
        }
    }

    #[test]
    fn is_solved_accepts_all_24_orientations_2x2() {
        for perm in super::all_face_permutations() {
            let state = make_rotated_solved(2, &perm);
            state.validate().expect("rotated solved state should be valid");
            assert!(
                state.is_solved(),
                "2x2 should be solved for perm {perm:?}"
            );
        }
    }

    #[test]
    fn is_solved_accepts_all_24_orientations_4x4() {
        for perm in super::all_face_permutations() {
            let state = make_rotated_solved(4, &perm);
            state.validate().expect("rotated solved state should be valid");
            assert!(
                state.is_solved(),
                "4x4 should be solved for perm {perm:?}"
            );
        }
    }

    #[test]
    fn is_solved_rejects_scrambled_3x3() {
        let mut state = CubeState::solved(CubeOrder::standard());
        apply_turn_to_state(
            &mut state,
            TurnCommand::outer(Face::Right, RotationAmount::Clockwise),
        )
        .expect("valid turn");
        assert!(!state.is_solved());
    }

    #[test]
    fn is_solved_rejects_scrambled_2x2() {
        let mut state = CubeState::solved(CubeOrder::new(2).expect("valid"));
        apply_turn_to_state(
            &mut state,
            TurnCommand::outer(Face::Up, RotationAmount::HalfTurn),
        )
        .expect("valid turn");
        assert!(!state.is_solved());
    }
}
