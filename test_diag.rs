use rubik_core::{CubeState, CubeOrder, Face, RotationAmount, TurnCommand, apply_turn_to_state};

fn main() {
    let mut state = CubeState::solved(CubeOrder::new(4).expect("valid order"));
    let scramble = [
        TurnCommand { face: Face::Right, start_layer: 1, width: 1, rotation: RotationAmount::Clockwise },
        TurnCommand { face: Face::Up, start_layer: 0, width: 1, rotation: RotationAmount::HalfTurn },
        TurnCommand { face: Face::Front, start_layer: 1, width: 1, rotation: RotationAmount::CounterClockwise },
        TurnCommand { face: Face::Right, start_layer: 0, width: 1, rotation: RotationAmount::Clockwise },
        TurnCommand { face: Face::Up, start_layer: 1, width: 1, rotation: RotationAmount::Clockwise },
        TurnCommand { face: Face::Left, start_layer: 0, width: 1, rotation: RotationAmount::CounterClockwise },
        TurnCommand { face: Face::Down, start_layer: 1, width: 1, rotation: RotationAmount::HalfTurn },
        TurnCommand { face: Face::Back, start_layer: 0, width: 1, rotation: RotationAmount::Clockwise },
        TurnCommand { face: Face::Up, start_layer: 0, width: 1, rotation: RotationAmount::CounterClockwise },
        TurnCommand { face: Face::Front, start_layer: 0, width: 1, rotation: RotationAmount::HalfTurn },
    ];
    for t in &scramble { apply_turn_to_state(&mut state, *t).unwrap(); }

    eprintln!("Initial correct centers: U={}, D={}",
        rubik_reduction::center_types::center_positions_on_face(Face::Up, 4).iter().filter(|p| p.color_in(&state) == Face::Up.solved_color()).count(),
        rubik_reduction::center_types::center_positions_on_face(Face::Down, 4).iter().filter(|p| p.color_in(&state) == Face::Down.solved_color()).count(),
    );

    let start = std::time::Instant::now();
    match rubik_reduction::center_solver::solve_centers(&state) {
        Ok(_turns) => eprintln!("Centers solved in {:?}, {} turns", start.elapsed(), _turns.len()),
        Err(e) => eprintln!("Centers FAILED: {} in {:?}", e, start.elapsed()),
    }

    // Apply center solution and check edges
    let mut current = state.clone();
    match rubik_reduction::center_solver::solve_centers(&state) {
        Ok(ref turns) => {
            for &t in turns { apply_turn_to_state(&mut current, t).unwrap(); }
            let paired = rubik_reduction::edge_types::count_paired_edge_groups(&current);
            eprintln!("After centers: paired edge groups = {}", paired);
            
            let start2 = std::time::Instant::now();
            match rubik_reduction::edge_solver::pair_edges(&current) {
                Ok(_turns2) => eprintln!("Edges solved in {:?}, {} turns", start2.elapsed(), _turns2.len()),
                Err(e) => eprintln!("Edges FAILED: {} in {:?}", e, start2.elapsed()),
            }
        }
        _ => {}
    }
}
