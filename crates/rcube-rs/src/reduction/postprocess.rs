use rubik_core::{RotationAmount, TurnCommand};

/// Post-process a sequence of turns: cancel opposites, combine rotations,
/// and merge adjacent layers into wide turns. Repeats until fixed point.
pub fn postprocess(turns: &[TurnCommand]) -> Vec<TurnCommand> {
    let mut result = turns.to_vec();
    loop {
        let len_before = result.len();
        result = merge_wide_turns(&result);
        result = cancel_and_combine(&result);
        if result.len() == len_before {
            break;
        }
    }
    result
}

/// Cancel adjacent opposite moves and combine same-layer same-rotation moves.
fn cancel_and_combine(turns: &[TurnCommand]) -> Vec<TurnCommand> {
    if turns.is_empty() {
        return vec![];
    }
    let mut result: Vec<TurnCommand> = Vec::with_capacity(turns.len());

    for &turn in turns {
        if result.is_empty() {
            result.push(turn);
            continue;
        }
        let prev = result.last().unwrap();

        // Same face and same layer range → try to combine or cancel
        if prev.face == turn.face
            && prev.start_layer == turn.start_layer
            && prev.width == turn.width
        {
            match combine_rotations(prev.rotation, turn.rotation) {
                Ok(Some(new_rot)) => {
                    result.last_mut().unwrap().rotation = new_rot;
                    continue;
                }
                Ok(None) => {
                    // Cancelled (e.g., CW + CCW)
                    result.pop();
                    continue;
                }
                Err(()) => {
                    // Cannot combine, push as-is
                }
            }
        }
        result.push(turn);
    }

    result
}

/// Merge adjacent single-layer moves on the same face into wide turns.
fn merge_wide_turns(turns: &[TurnCommand]) -> Vec<TurnCommand> {
    if turns.is_empty() {
        return vec![];
    }
    let mut result: Vec<TurnCommand> = Vec::with_capacity(turns.len());

    for &turn in turns {
        if result.is_empty() {
            result.push(turn);
            continue;
        }
        let prev = result.last().unwrap();

        // Merge if: same face, same rotation, adjacent layers
        if prev.face == turn.face
            && prev.rotation == turn.rotation
            && prev.start_layer + prev.width == turn.start_layer
        {
            let merged = TurnCommand {
                face: prev.face,
                start_layer: prev.start_layer,
                width: prev.width + turn.width,
                rotation: prev.rotation,
            };
            result.pop();
            result.push(merged);
            continue;
        }
        result.push(turn);
    }

    result
}

/// Combine two rotations on the same layer.
/// Returns:
///   Ok(Some(rot)) — combined into a single rotation
///   Ok(None) — cancelled (e.g., CW + CCW = identity)
///   Err(()) — cannot combine (e.g., CW + CW on same face)
fn combine_rotations(
    a: RotationAmount,
    b: RotationAmount,
) -> Result<Option<RotationAmount>, ()> {
    match (a, b) {
        // Same direction → half turn
        (RotationAmount::Clockwise, RotationAmount::Clockwise)
        | (RotationAmount::CounterClockwise, RotationAmount::CounterClockwise) => {
            Ok(Some(RotationAmount::HalfTurn))
        }
        // Opposite directions → cancel
        (RotationAmount::Clockwise, RotationAmount::CounterClockwise)
        | (RotationAmount::CounterClockwise, RotationAmount::Clockwise) => Ok(None),
        // Half turn + CW = CCW, Half turn + CCW = CW
        (RotationAmount::HalfTurn, RotationAmount::Clockwise) => {
            Ok(Some(RotationAmount::CounterClockwise))
        }
        (RotationAmount::HalfTurn, RotationAmount::CounterClockwise) => {
            Ok(Some(RotationAmount::Clockwise))
        }
        (RotationAmount::Clockwise, RotationAmount::HalfTurn) => {
            Ok(Some(RotationAmount::CounterClockwise))
        }
        (RotationAmount::CounterClockwise, RotationAmount::HalfTurn) => {
            Ok(Some(RotationAmount::Clockwise))
        }
        // Two half turns → cancel
        (RotationAmount::HalfTurn, RotationAmount::HalfTurn) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rubik_core::Face;

    fn turn(face: Face, start: u32, width: u32, rot: RotationAmount) -> TurnCommand {
        TurnCommand {
            face,
            start_layer: start,
            width,
            rotation: rot,
        }
    }

    #[test]
    fn cancel_opposite_rotations() {
        let input = vec![
            turn(Face::Right, 0, 1, RotationAmount::Clockwise),
            turn(Face::Right, 0, 1, RotationAmount::CounterClockwise),
        ];
        let result = postprocess(&input);
        assert!(result.is_empty());
    }

    #[test]
    fn combine_same_rotations() {
        let input = vec![
            turn(Face::Up, 0, 1, RotationAmount::Clockwise),
            turn(Face::Up, 0, 1, RotationAmount::Clockwise),
        ];
        let result = postprocess(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].rotation, RotationAmount::HalfTurn);
        assert_eq!(result[0].width, 1);
    }

    #[test]
    fn half_turn_then_cw() {
        let input = vec![
            turn(Face::Front, 0, 1, RotationAmount::HalfTurn),
            turn(Face::Front, 0, 1, RotationAmount::Clockwise),
        ];
        let result = postprocess(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].rotation, RotationAmount::CounterClockwise);
    }

    #[test]
    fn merge_adjacent_wide() {
        let input = vec![
            turn(Face::Right, 0, 1, RotationAmount::Clockwise),
            turn(Face::Right, 1, 1, RotationAmount::Clockwise),
        ];
        let result = postprocess(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].start_layer, 0);
        assert_eq!(result[0].width, 2);
        assert_eq!(result[0].rotation, RotationAmount::Clockwise);
    }

    #[test]
    fn no_merge_different_face() {
        let input = vec![
            turn(Face::Right, 0, 1, RotationAmount::Clockwise),
            turn(Face::Up, 0, 1, RotationAmount::Clockwise),
        ];
        let result = postprocess(&input);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn cancel_then_merge() {
        // U U' on layer 0, then U and U on adjacent layers should merge
        let input = vec![
            turn(Face::Up, 0, 1, RotationAmount::Clockwise),
            turn(Face::Up, 0, 1, RotationAmount::CounterClockwise),
            turn(Face::Up, 0, 1, RotationAmount::Clockwise),
            turn(Face::Up, 1, 1, RotationAmount::Clockwise),
        ];
        let result = postprocess(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].start_layer, 0);
        assert_eq!(result[0].width, 2);
    }

    #[test]
    fn empty_input() {
        let result = postprocess(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn single_move_passes_through() {
        let input = vec![turn(
            Face::Right,
            0,
            1,
            RotationAmount::HalfTurn,
        )];
        let result = postprocess(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].rotation, RotationAmount::HalfTurn);
    }
}
