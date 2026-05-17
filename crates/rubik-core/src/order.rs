use core::fmt;

use serde::{Deserialize, Serialize};

pub const MIN_CUBE_ORDER: u8 = 2;
pub const MAX_CUBE_ORDER: u8 = 17;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CubeOrder(u8);

impl CubeOrder {
    pub fn new(value: u8) -> Result<Self, CubeOrderError> {
        if value < MIN_CUBE_ORDER {
            return Err(CubeOrderError::TooSmall {
                attempted: value,
                min: MIN_CUBE_ORDER,
            });
        }

        if value > MAX_CUBE_ORDER {
            return Err(CubeOrderError::TooLarge {
                attempted: value,
                max: MAX_CUBE_ORDER,
            });
        }

        Ok(Self(value))
    }

    pub const fn standard() -> Self {
        Self(3)
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CubeOrderError {
    TooSmall { attempted: u8, min: u8 },
    TooLarge { attempted: u8, max: u8 },
}

impl fmt::Display for CubeOrderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooSmall { attempted, min } => {
                write!(f, "cube order {attempted} is below the minimum supported order {min}")
            }
            Self::TooLarge { attempted, max } => {
                write!(f, "cube order {attempted} exceeds the maximum supported order {max}")
            }
        }
    }
}

impl std::error::Error for CubeOrderError {}

#[cfg(test)]
mod tests {
    use super::{CubeOrder, CubeOrderError, MAX_CUBE_ORDER, MIN_CUBE_ORDER};

    #[test]
    fn accepts_the_smallest_supported_order() {
        let order = CubeOrder::new(MIN_CUBE_ORDER).expect("minimum order should be valid");
        assert_eq!(order.get(), MIN_CUBE_ORDER);
    }

    #[test]
    fn rejects_orders_smaller_than_the_minimum() {
        let error = CubeOrder::new(MIN_CUBE_ORDER - 1).expect_err("order should be rejected");
        assert_eq!(
            error,
            CubeOrderError::TooSmall {
                attempted: MIN_CUBE_ORDER - 1,
                min: MIN_CUBE_ORDER,
            }
        );
    }

    #[test]
    fn rejects_orders_larger_than_the_maximum() {
        let error = CubeOrder::new(MAX_CUBE_ORDER + 1).expect_err("order should be rejected");
        assert_eq!(
            error,
            CubeOrderError::TooLarge {
                attempted: MAX_CUBE_ORDER + 1,
                max: MAX_CUBE_ORDER,
            }
        );
    }

    #[test]
    fn exposes_the_standard_order() {
        assert_eq!(CubeOrder::standard().get(), 3);
    }
}
