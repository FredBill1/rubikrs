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

    /// Returns a solved-state target where each face's uniform color is
    /// determined by the true center sticker of `self`.
    ///
    /// For odd-order cubes: reads the center sticker of each face and fills
    /// the entire face with that color. This supports non-standard color
    /// schemes where centers may not be in their canonical positions.
    ///
    /// For even-order cubes: delegates to [`Self::solved`] because there is
    /// no unique true center sticker.
    pub fn solved_with_centers_from(&self) -> Self {
        let order = self.order.get() as usize;
        let face_size = order * order;

        if order % 2 == 0 {
            return Self::solved(self.order);
        }

        let mid = order / 2;
        let center_offset = mid * order + mid;

        let stickers: Vec<StickerColor> = Face::ALL
            .into_iter()
            .enumerate()
            .flat_map(|(face_idx, _face)| {
                let face_start = face_idx * face_size;
                let center_color = self.stickers[face_start + center_offset];
                core::iter::repeat_n(center_color, face_size)
            })
            .collect();

        Self {
            version: CUBE_STATE_SCHEMA_VERSION,
            order: self.order,
            stickers,
        }
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

#[cfg(test)]
mod tests {
    use super::{
        CUBE_STATE_SCHEMA_VERSION, CubeState, CubeStateParseError, CubeStateValidationError, Face,
        RotationAmount, StickerColor, TurnCommand, TurnCommandValidationError,
    };
    use crate::CubeOrder;

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
}
