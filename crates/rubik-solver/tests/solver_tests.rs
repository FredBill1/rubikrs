use rubik_core::{apply_turn_to_state, CubeOrder, CubeState, Face, RotationAmount, TurnCommand};
use rubik_solver::solve;
use rubik_solver::kociemba;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

// ---------------------------------------------------------------------------
// Helper: generate deterministic scramble for a given order and seed
// ---------------------------------------------------------------------------

fn generate_scramble(order: u32, length: usize, seed: u64) -> Vec<TurnCommand> {
    let mut rng = SimpleRng::new(seed);
    let mut scramble = Vec::with_capacity(length);
    let mut last_move_key: Option<(Face, u32)> = None;
    let max_start_layer = order.saturating_sub(1);

    while scramble.len() < length {
        let face = Face::ALL[rng.range(6)];
        let rotation = match rng.range(3) {
            0 => RotationAmount::Clockwise,
            1 => RotationAmount::HalfTurn,
            _ => RotationAmount::CounterClockwise,
        };
        let start_layer = if order <= 3 {
            0
        } else {
            rng.range(max_start_layer as usize + 1) as u32
        };
        let move_key = (face, start_layer);
        if Some(move_key) == last_move_key {
            continue;
        }

        scramble.push(TurnCommand {
            face,
            start_layer,
            width: 1,
            rotation,
        });
        last_move_key = Some(move_key);
    }

    scramble
}

fn apply_turns(state: &mut CubeState, turns: &[TurnCommand]) -> Result<(), String> {
    for &turn in turns {
        apply_turn_to_state(state, turn).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn is_solved(state: &CubeState) -> bool {
    state.is_solved()
}

fn verify_solution(initial: &CubeState, solution: &[TurnCommand]) -> bool {
    let mut state = initial.clone();
    apply_turns(&mut state, solution).is_ok() && is_solved(&state)
}

// ---------------------------------------------------------------------------
// Time-limited solve helper
// ---------------------------------------------------------------------------

fn solve_with_timeout(state: &CubeState, timeout: Duration) -> Result<Vec<TurnCommand>, String> {
    let state = state.clone();
    let done = Arc::new(AtomicBool::new(false));
    let result = Arc::new(std::sync::Mutex::new(None::<Result<Vec<TurnCommand>, String>>));

    let result_clone = Arc::clone(&result);
    let done_clone = Arc::clone(&done);

    let handle = thread::spawn(move || {
        let sol = solve(&state);
        let mut guard = result_clone.lock().expect("lock should not be poisoned");
        *guard = Some(sol.map_err(|e| e.to_string()));
        done_clone.store(true, Ordering::SeqCst);
    });

    let start = Instant::now();
    loop {
        if done.load(Ordering::SeqCst) {
            break;
        }
        if start.elapsed() > timeout {
            rubik_solver::request_cancel();
            // Wait a bit for cancellation to take effect
            thread::sleep(Duration::from_millis(100));
            // If still running after cancel, the thread will eventually finish or be abandoned
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    let _ = handle.join();
    let guard = result.lock().expect("lock should not be poisoned");
    match guard.as_ref() {
        Some(Ok(solution)) => {
            let elapsed = start.elapsed();
            if elapsed > timeout {
                Err(format!(
                    "solve took {:?} which exceeds timeout {:?}",
                    elapsed, timeout
                ))
            } else {
                Ok(solution.clone())
            }
        }
        Some(Err(e)) => Err(e.clone()),
        None => Err(format!(
            "solve timed out after {:?}",
            start.elapsed()
        )),
    }
}

// ---------------------------------------------------------------------------
// Kociemba solve helper (thread-based timeout, same pattern as solve_with_timeout)
// ---------------------------------------------------------------------------

fn solve_kociemba_with_timeout(state: &CubeState, timeout: Duration) -> Result<Vec<TurnCommand>, String> {
    let state = state.clone();
    let done = Arc::new(AtomicBool::new(false));
    let result = Arc::new(std::sync::Mutex::new(None::<Result<Vec<TurnCommand>, String>>));

    let result_clone = Arc::clone(&result);
    let done_clone = Arc::clone(&done);

    let handle = thread::spawn(move || {
        let sol = kociemba::solve(&state);
        let mut guard = result_clone.lock().expect("lock should not be poisoned");
        *guard = Some(sol.map_err(|e| e.to_string()));
        done_clone.store(true, Ordering::SeqCst);
    });

    let start = Instant::now();
    loop {
        if done.load(Ordering::SeqCst) {
            break;
        }
        if start.elapsed() > timeout {
            rubik_solver::request_cancel();
            thread::sleep(Duration::from_millis(100));
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    let _ = handle.join();
    let guard = result.lock().expect("lock should not be poisoned");
    match guard.as_ref() {
        Some(Ok(solution)) => {
            let elapsed = start.elapsed();
            if elapsed > timeout {
                Err(format!(
                    "kociemba solve took {:?} which exceeds timeout {:?}",
                    elapsed, timeout
                ))
            } else {
                Ok(solution.clone())
            }
        }
        Some(Err(e)) => Err(e.clone()),
        None => Err(format!(
            "kociemba solve timed out after {:?}",
            start.elapsed()
        )),
    }
}

// ---------------------------------------------------------------------------
// Helper: generate manually constructed complex states
// ---------------------------------------------------------------------------

fn apply_algorithm(state: &mut CubeState, algorithm: &[TurnCommand]) {
    for &turn in algorithm {
        apply_turn_to_state(state, turn).expect("valid turn");
    }
}

fn make_complex_3x3_state(order: u32) -> CubeState {
    let mut state = CubeState::solved(CubeOrder::new(order).expect("valid order"));
    // Apply a known complex scramble
    let scramble = [
        TurnCommand::outer(Face::Right, RotationAmount::Clockwise),
        TurnCommand::outer(Face::Up, RotationAmount::Clockwise),
        TurnCommand::outer(Face::Right, RotationAmount::CounterClockwise),
        TurnCommand::outer(Face::Front, RotationAmount::HalfTurn),
        TurnCommand::outer(Face::Up, RotationAmount::CounterClockwise),
        TurnCommand::outer(Face::Right, RotationAmount::Clockwise),
        TurnCommand::outer(Face::Front, RotationAmount::Clockwise),
        TurnCommand::outer(Face::Left, RotationAmount::CounterClockwise),
        TurnCommand::outer(Face::Down, RotationAmount::Clockwise),
        TurnCommand::outer(Face::Back, RotationAmount::HalfTurn),
        TurnCommand::outer(Face::Up, RotationAmount::CounterClockwise),
        TurnCommand::outer(Face::Right, RotationAmount::Clockwise),
        TurnCommand::outer(Face::Down, RotationAmount::CounterClockwise),
        TurnCommand::outer(Face::Front, RotationAmount::Clockwise),
        TurnCommand::outer(Face::Left, RotationAmount::HalfTurn),
    ];
    apply_algorithm(&mut state, &scramble);
    state
}

fn make_complex_4x4_state() -> CubeState {
    use Face::*;
    use RotationAmount::*;
    let mut state = CubeState::solved(CubeOrder::new(4).expect("valid order"));
    let scramble = [
        TurnCommand { face: Right, start_layer: 1, width: 1, rotation: Clockwise },
        TurnCommand { face: Up, start_layer: 0, width: 1, rotation: HalfTurn },
        TurnCommand { face: Front, start_layer: 1, width: 1, rotation: CounterClockwise },
        TurnCommand { face: Right, start_layer: 0, width: 1, rotation: Clockwise },
        TurnCommand { face: Up, start_layer: 1, width: 1, rotation: Clockwise },
        TurnCommand { face: Left, start_layer: 0, width: 1, rotation: CounterClockwise },
        TurnCommand { face: Down, start_layer: 1, width: 1, rotation: HalfTurn },
        TurnCommand { face: Back, start_layer: 0, width: 1, rotation: Clockwise },
        TurnCommand { face: Up, start_layer: 0, width: 1, rotation: CounterClockwise },
        TurnCommand { face: Front, start_layer: 0, width: 1, rotation: HalfTurn },
    ];
    apply_algorithm(&mut state, &scramble);
    state
}

fn make_complex_state(order: u32) -> CubeState {
    use RotationAmount::*;
    let mut state = CubeState::solved(CubeOrder::new(order).expect("valid order"));
    if order == 3 {
        return make_complex_3x3_state(order);
    }
    if order == 4 {
        return make_complex_4x4_state();
    }
    // For other orders, apply a mix of outer and inner layer turns
    let mut rng = SimpleRng::new(order as u64 * 137);
    for _ in 0..50 {
        let face = Face::ALL[rng.range(6)];
        let rotation = match rng.range(3) {
            0 => Clockwise,
            1 => HalfTurn,
            _ => CounterClockwise,
        };
        let start_layer = rng.range(order.saturating_sub(1).max(1) as usize) as u32;
        let turn = TurnCommand {
            face,
            start_layer,
            width: 1,
            rotation,
        };
        let _ = apply_turn_to_state(&mut state, turn);
    }
    state
}

// ---------------------------------------------------------------------------
// Simple RNG
// ---------------------------------------------------------------------------

struct SimpleRng {
    state: u64,
}

impl SimpleRng {
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
        if upper_bound == 0 {
            return 0;
        }
        (self.next_u64() % upper_bound as u64) as usize
    }
}

// ===========================================================================
// TEST 1: Simple short move combinations (2x2 through 7x7)
// ===========================================================================

mod simple_short_moves {
    use super::*;

    fn test_simple_moves_for_order(order: u32, turns: &[TurnCommand]) {
        let mut state = CubeState::solved(CubeOrder::new(order).expect("valid order"));
        apply_turns(&mut state, turns).expect("turns should apply");
        assert!(!is_solved(&state), "state should not be solved after applying turns");

        let solution =
            solve_with_timeout(&state, Duration::from_secs(1)).expect("solve should succeed");
        assert!(
            verify_solution(&state, &solution),
            "solution should restore solved state for {order}x{order}"
        );
    }

    #[test]
    fn test_2x2_single_outer_turn() {
        test_simple_moves_for_order(2, &[TurnCommand::outer(Face::Right, RotationAmount::Clockwise)]);
    }

    #[test]
    fn test_3x3_single_outer_turn() {
        test_simple_moves_for_order(3, &[TurnCommand::outer(Face::Up, RotationAmount::HalfTurn)]);
    }

    #[test]
    fn test_3x3_two_outer_turns() {
        test_simple_moves_for_order(
            3,
            &[
                TurnCommand::outer(Face::Right, RotationAmount::Clockwise),
                TurnCommand::outer(Face::Up, RotationAmount::CounterClockwise),
            ],
        );
    }

    #[test]
    fn test_4x4_single_outer_turn() {
        test_simple_moves_for_order(4, &[TurnCommand::outer(Face::Front, RotationAmount::HalfTurn)]);
    }

    #[test]
    fn test_4x4_inner_layer_turn() {
        test_simple_moves_for_order(
            4,
            &[TurnCommand {
                face: Face::Right,
                start_layer: 1,
                width: 1,
                rotation: RotationAmount::Clockwise,
            }],
        );
    }

    #[test]
    fn test_5x5_outer_and_inner() {
        test_simple_moves_for_order(
            5,
            &[
                TurnCommand::outer(Face::Up, RotationAmount::Clockwise),
                TurnCommand {
                    face: Face::Right,
                    start_layer: 2,
                    width: 1,
                    rotation: RotationAmount::HalfTurn,
                },
            ],
        );
    }

    #[test]
    fn test_6x6_wide_turn() {
        test_simple_moves_for_order(
            6,
            &[TurnCommand {
                face: Face::Front,
                start_layer: 0,
                width: 2,
                rotation: RotationAmount::CounterClockwise,
            }],
        );
    }

    #[test]
    fn test_7x7_multiple_turns() {
        test_simple_moves_for_order(
            7,
            &[
                TurnCommand::outer(Face::Right, RotationAmount::Clockwise),
                TurnCommand::outer(Face::Up, RotationAmount::HalfTurn),
                TurnCommand {
                    face: Face::Front,
                    start_layer: 3,
                    width: 1,
                    rotation: RotationAmount::CounterClockwise,
                },
                TurnCommand {
                    face: Face::Left,
                    start_layer: 1,
                    width: 2,
                    rotation: RotationAmount::Clockwise,
                },
            ],
        );
    }
}

// ===========================================================================
// TEST 2: Manually constructed complex states (2x2 through 7x7)
// ===========================================================================

mod complex_states {
    use super::*;

    fn test_complex_state(order: u32) {
        let state = make_complex_state(order);
        let solution =
            solve_with_timeout(&state, Duration::from_secs(1)).expect("solve should succeed");

        assert!(
            verify_solution(&state, &solution),
            "solution should restore solved state for {order}x{order} complex state. Solution length: {}",
            solution.len()
        );
    }

    #[test]
    fn test_2x2_complex() {
        test_complex_state(2);
    }

    #[test]
    fn test_3x3_complex() {
        test_complex_state(3);
    }

    #[test]
    fn test_4x4_complex() {
        test_complex_state(4);
    }

    #[test]
    fn test_5x5_complex() {
        test_complex_state(5);
    }

    #[test]
    fn test_6x6_complex() {
        test_complex_state(6);
    }

    #[test]
    fn test_7x7_complex() {
        test_complex_state(7);
    }
}

// ===========================================================================
// TEST 3: Random 200-move scrambles (2x2 through 7x7)
// ===========================================================================

mod random_200_scrambles {
    use super::*;

    fn test_scramble(order: u32, seed: u64) {
        let scramble = generate_scramble(order, 200, seed);
        let mut state = CubeState::solved(CubeOrder::new(order).expect("valid order"));
        apply_turns(&mut state, &scramble).expect("scramble should apply");
        assert!(
            !is_solved(&state),
            "scrambled state for {order}x{order} should not be solved"
        );

        let solution =
            solve_with_timeout(&state, Duration::from_secs(1)).expect("solve should succeed");

        assert!(
            verify_solution(&state, &solution),
            "solution should restore solved state for {order}x{order}. Seed: {seed}. Solution length: {}",
            solution.len()
        );
    }

    #[test]
    fn test_2x2_scramble_seed_1() {
        test_scramble(2, 1);
    }

    #[test]
    fn test_2x2_scramble_seed_42() {
        test_scramble(2, 42);
    }

    #[test]
    fn test_3x3_scramble_seed_1() {
        test_scramble(3, 1);
    }

    #[test]
    fn test_3x3_scramble_seed_42() {
        test_scramble(3, 42);
    }

    #[test]
    fn test_4x4_scramble_seed_1() {
        test_scramble(4, 1);
    }

    #[test]
    fn test_4x4_scramble_seed_42() {
        test_scramble(4, 42);
    }

    #[test]
    fn test_5x5_scramble_seed_1() {
        test_scramble(5, 1);
    }

    #[test]
    fn test_5x5_scramble_seed_42() {
        test_scramble(5, 42);
    }

    #[test]
    fn test_6x6_scramble_seed_1() {
        test_scramble(6, 1);
    }

    #[test]
    fn test_6x6_scramble_seed_42() {
        test_scramble(6, 42);
    }

    #[test]
    fn test_7x7_scramble_seed_1() {
        test_scramble(7, 1);
    }

    #[test]
    fn test_7x7_scramble_seed_42() {
        test_scramble(7, 42);
    }
}

// ===========================================================================
// TEST 4: Large cube (1024x1024) 2000-move scrambles
// ===========================================================================

mod large_cube_scrambles {
    use super::*;

    #[test]
    fn test_64x64_scramble_200_moves() {
        let order: u32 = 64;
        let scramble = generate_scramble(order, 200, 12345);
        let mut state =
            CubeState::solved(CubeOrder::new(order).expect("64 should be valid"));

        apply_turns(&mut state, &scramble).expect("scramble should apply");
        assert!(!is_solved(&state), "scrambled 64x64 state should not be solved");

        let solution =
            solve_with_timeout(&state, Duration::from_secs(3)).expect("solve should succeed");

        assert!(
            verify_solution(&state, &solution),
            "solution should restore solved state for 64x64. Solution length: {}",
            solution.len()
        );
    }

    #[test]
    fn test_64x64_scramble_200_moves_seed2() {
        let order: u32 = 64;
        let scramble = generate_scramble(order, 200, 99999);
        let mut state =
            CubeState::solved(CubeOrder::new(order).expect("64 should be valid"));
        apply_turns(&mut state, &scramble).expect("scramble should apply");

        let solution =
            solve_with_timeout(&state, Duration::from_secs(3)).expect("solve should succeed");

        assert!(
            verify_solution(&state, &solution),
            "solution should restore solved state for 64x64 (seed 99999)"
        );
    }
}

// ===========================================================================
// TEST 5: Edge cases
// ===========================================================================

mod edge_cases {
    use super::*;

    #[test]
    fn test_already_solved_2x2() {
        let state = CubeState::solved(CubeOrder::new(2).expect("valid order"));
        let solution =
            solve_with_timeout(&state, Duration::from_secs(1)).expect("solve should succeed");
        assert!(
            verify_solution(&state, &solution),
            "solving already-solved state should work"
        );
    }

    #[test]
    fn test_already_solved_3x3() {
        let state = CubeState::solved(CubeOrder::new(3).expect("valid order"));
        let solution =
            solve_with_timeout(&state, Duration::from_secs(1)).expect("solve should succeed");
        assert!(
            verify_solution(&state, &solution),
            "solving already-solved state should work"
        );
    }

    #[test]
    fn test_solve_inverse_of_solution() {
        // Solve a scrambled state
        let order = 3u32;
        let scramble = generate_scramble(order, 20, 777);
        let mut state = CubeState::solved(CubeOrder::new(order).expect("valid order"));
        apply_turns(&mut state, &scramble).expect("scramble should apply");

        let solution =
            solve_with_timeout(&state, Duration::from_secs(1)).expect("solve should succeed");
        assert!(verify_solution(&state, &solution));

        // Now apply the solution to the scrambled state and verify it's solved
        let mut verify = state.clone();
        for &turn in &solution {
            apply_turn_to_state(&mut verify, turn).expect("solution turn should be valid");
        }
        assert!(is_solved(&verify), "applying solution should restore solved state");
    }

    #[test]
    fn test_quarter_turn_cycle() {
        // Apply R four times to 3x3 - should return to solved
        let mut state = CubeState::solved(CubeOrder::standard());
        let turn = TurnCommand::outer(Face::Right, RotationAmount::Clockwise);
        for _ in 0..4 {
            apply_turn_to_state(&mut state, turn).expect("turn should apply");
        }
        assert!(is_solved(&state), "4x R should return to solved");

        let solution =
            solve_with_timeout(&state, Duration::from_secs(1)).expect("solve should succeed");
        assert!(verify_solution(&state, &solution));
    }
}

// ===========================================================================
// TEST 6: Kociemba (min2phase) 3x3 solver
// ===========================================================================

mod kociemba_3x3 {
    use super::*;

    fn test_kociemba_simple(order: u32, turns: &[TurnCommand]) {
        assert_eq!(order, 3, "kociemba solver is 3x3 only");
        let mut state = CubeState::solved(CubeOrder::new(order).expect("valid order"));
        apply_turns(&mut state, turns).expect("turns should apply");
        assert!(!is_solved(&state), "state should not be solved");

        let solution = solve_kociemba_with_timeout(&state, Duration::from_secs(1))
            .expect("kociemba solve should succeed");
        assert!(
            verify_solution(&state, &solution),
            "kociemba solution should restore solved state. Solution length: {}",
            solution.len()
        );
    }

    #[test]
    fn test_3x3_single_turn() {
        test_kociemba_simple(3, &[TurnCommand::outer(Face::Right, RotationAmount::Clockwise)]);
    }

    #[test]
    fn test_3x3_two_turns() {
        test_kociemba_simple(
            3,
            &[
                TurnCommand::outer(Face::Right, RotationAmount::Clockwise),
                TurnCommand::outer(Face::Up, RotationAmount::CounterClockwise),
            ],
        );
    }

    #[test]
    fn test_3x3_three_turns() {
        test_kociemba_simple(
            3,
            &[
                TurnCommand::outer(Face::Front, RotationAmount::HalfTurn),
                TurnCommand::outer(Face::Left, RotationAmount::Clockwise),
                TurnCommand::outer(Face::Down, RotationAmount::CounterClockwise),
            ],
        );
    }

    #[test]
    fn test_3x3_four_turns() {
        test_kociemba_simple(
            3,
            &[
                TurnCommand::outer(Face::Up, RotationAmount::Clockwise),
                TurnCommand::outer(Face::Right, RotationAmount::HalfTurn),
                TurnCommand::outer(Face::Front, RotationAmount::CounterClockwise),
                TurnCommand::outer(Face::Back, RotationAmount::Clockwise),
            ],
        );
    }

    #[test]
    fn test_3x3_complex_state() {
        let state = make_complex_3x3_state(3);
        let solution = solve_kociemba_with_timeout(&state, Duration::from_secs(1))
            .expect("kociemba solve should succeed");
        assert!(
            verify_solution(&state, &solution),
            "kociemba should solve complex 3x3 state. Solution length: {}",
            solution.len()
        );
    }

    #[test]
    fn test_3x3_scramble_seed_1() {
        let order = 3u32;
        let scramble = generate_scramble(order, 200, 1);
        let mut state = CubeState::solved(CubeOrder::new(order).expect("valid order"));
        apply_turns(&mut state, &scramble).expect("scramble should apply");

        let solution = solve_kociemba_with_timeout(&state, Duration::from_secs(1))
            .expect("kociemba solve should succeed");
        assert!(
            verify_solution(&state, &solution),
            "kociemba should solve 200-move 3x3 scramble (seed 1). Solution length: {}",
            solution.len()
        );
    }

    #[test]
    fn test_3x3_scramble_seed_42() {
        let order = 3u32;
        let scramble = generate_scramble(order, 200, 42);
        let mut state = CubeState::solved(CubeOrder::new(order).expect("valid order"));
        apply_turns(&mut state, &scramble).expect("scramble should apply");

        let solution = solve_kociemba_with_timeout(&state, Duration::from_secs(1))
            .expect("kociemba solve should succeed");
        assert!(
            verify_solution(&state, &solution),
            "kociemba should solve 200-move 3x3 scramble (seed 42). Solution length: {}",
            solution.len()
        );
    }

    #[test]
    fn test_3x3_already_solved() {
        let state = CubeState::solved(CubeOrder::new(3).expect("valid order"));
        let solution = solve_kociemba_with_timeout(&state, Duration::from_secs(1))
            .expect("kociemba solve should succeed");
        assert!(
            verify_solution(&state, &solution),
            "kociemba solving already-solved 3x3 should work"
        );
    }

    #[test]
    fn test_3x3_quarter_turn_cycle() {
        let mut state = CubeState::solved(CubeOrder::standard());
        let turn = TurnCommand::outer(Face::Right, RotationAmount::Clockwise);
        for _ in 0..4 {
            apply_turn_to_state(&mut state, turn).expect("turn should apply");
        }
        assert!(is_solved(&state), "4x R should return to solved");

        let solution = solve_kociemba_with_timeout(&state, Duration::from_secs(1))
            .expect("kociemba solve should succeed");
        assert!(verify_solution(&state, &solution));
    }

    #[test]
    fn test_3x3_inverse_of_solution() {
        let order = 3u32;
        let scramble = generate_scramble(order, 20, 777);
        let mut state = CubeState::solved(CubeOrder::new(order).expect("valid order"));
        apply_turns(&mut state, &scramble).expect("scramble should apply");

        let solution = solve_kociemba_with_timeout(&state, Duration::from_secs(1))
            .expect("kociemba solve should succeed");
        assert!(verify_solution(&state, &solution));

        let mut verify = state.clone();
        for &turn in &solution {
            apply_turn_to_state(&mut verify, turn).expect("solution turn should be valid");
        }
        assert!(is_solved(&verify), "applying solution should restore solved state");
    }
}

// ===========================================================================
// TEST 7: IDA* 2x2 solver
// ===========================================================================

mod ida2x2_2x2 {
    use super::*;

    fn solve_ida2x2_with_timeout(
        state: &CubeState,
        timeout: Duration,
    ) -> Result<Vec<TurnCommand>, String> {
        let state = state.clone();
        let done = Arc::new(AtomicBool::new(false));
        let result = Arc::new(std::sync::Mutex::new(None::<Result<Vec<TurnCommand>, String>>));

        let result_clone = Arc::clone(&result);
        let done_clone = Arc::clone(&done);

        let handle = thread::spawn(move || {
            let sol = rubik_solver::ida2x2::solve(&state);
            let mut guard = result_clone.lock().expect("lock should not be poisoned");
            *guard = Some(sol.map_err(|e| e.to_string()));
            done_clone.store(true, Ordering::SeqCst);
        });

        let start = Instant::now();
        loop {
            if done.load(Ordering::SeqCst) {
                break;
            }
            if start.elapsed() > timeout {
                rubik_solver::request_cancel();
                thread::sleep(Duration::from_millis(100));
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        let _ = handle.join();
        let guard = result.lock().expect("lock should not be poisoned");
        match guard.as_ref() {
            Some(Ok(solution)) => {
                let elapsed = start.elapsed();
                if elapsed > timeout {
                    Err(format!(
                        "ida2x2 solve took {:?} which exceeds timeout {:?}",
                        elapsed, timeout
                    ))
                } else {
                    Ok(solution.clone())
                }
            }
            Some(Err(e)) => Err(e.clone()),
            None => Err(format!("ida2x2 solve timed out after {:?}", start.elapsed())),
        }
    }

    // -----------------------------------------------------------------------
    // Simple turn tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_2x2_single_turn_r() {
        let mut state = CubeState::solved(CubeOrder::new(2).expect("valid"));
        apply_turns(
            &mut state,
            &[TurnCommand::outer(Face::Right, RotationAmount::Clockwise)],
        )
        .expect("turn");
        let solution = solve_ida2x2_with_timeout(&state, Duration::from_secs(5))
            .expect("solve should succeed");
        assert!(
            verify_solution(&state, &solution),
            "ida2x2 should solve R turn. Solution length: {}",
            solution.len()
        );
        assert!(solution.len() <= 11, "2x2 solution should be <= 11 HTM, got {}", solution.len());
    }

    #[test]
    fn test_2x2_single_turn_u() {
        let mut state = CubeState::solved(CubeOrder::new(2).expect("valid"));
        apply_turns(
            &mut state,
            &[TurnCommand::outer(Face::Up, RotationAmount::HalfTurn)],
        )
        .expect("turn");
        let solution = solve_ida2x2_with_timeout(&state, Duration::from_secs(5))
            .expect("solve should succeed");
        assert!(
            verify_solution(&state, &solution),
            "ida2x2 should solve U2 turn"
        );
    }

    #[test]
    fn test_2x2_single_turn_f() {
        let mut state = CubeState::solved(CubeOrder::new(2).expect("valid"));
        apply_turns(
            &mut state,
            &[TurnCommand::outer(Face::Front, RotationAmount::CounterClockwise)],
        )
        .expect("turn");
        let solution = solve_ida2x2_with_timeout(&state, Duration::from_secs(5))
            .expect("solve should succeed");
        assert!(
            verify_solution(&state, &solution),
            "ida2x2 should solve F' turn"
        );
    }

    // -----------------------------------------------------------------------
    // Multi-turn tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_2x2_two_turns() {
        let mut state = CubeState::solved(CubeOrder::new(2).expect("valid"));
        apply_turns(
            &mut state,
            &[
                TurnCommand::outer(Face::Right, RotationAmount::Clockwise),
                TurnCommand::outer(Face::Up, RotationAmount::CounterClockwise),
            ],
        )
        .expect("turns");
        let solution = solve_ida2x2_with_timeout(&state, Duration::from_secs(5))
            .expect("solve should succeed");
        assert!(
            verify_solution(&state, &solution),
            "ida2x2 should solve 2-turn scramble"
        );
    }

    #[test]
    fn test_2x2_three_turns() {
        let mut state = CubeState::solved(CubeOrder::new(2).expect("valid"));
        apply_turns(
            &mut state,
            &[
                TurnCommand::outer(Face::Right, RotationAmount::Clockwise),
                TurnCommand::outer(Face::Up, RotationAmount::CounterClockwise),
                TurnCommand::outer(Face::Front, RotationAmount::HalfTurn),
            ],
        )
        .expect("turns");
        let solution = solve_ida2x2_with_timeout(&state, Duration::from_secs(5))
            .expect("solve should succeed");
        assert!(
            verify_solution(&state, &solution),
            "ida2x2 should solve 3-turn scramble"
        );
    }

    // -----------------------------------------------------------------------
    // Complex state tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_2x2_complex_state() {
        let mut state = CubeState::solved(CubeOrder::new(2).expect("valid"));
        let scramble = [
            TurnCommand::outer(Face::Right, RotationAmount::Clockwise),
            TurnCommand::outer(Face::Up, RotationAmount::Clockwise),
            TurnCommand::outer(Face::Right, RotationAmount::CounterClockwise),
            TurnCommand::outer(Face::Front, RotationAmount::HalfTurn),
            TurnCommand::outer(Face::Up, RotationAmount::CounterClockwise),
            TurnCommand::outer(Face::Down, RotationAmount::Clockwise),
            TurnCommand::outer(Face::Up, RotationAmount::CounterClockwise),
            TurnCommand::outer(Face::Right, RotationAmount::Clockwise),
            TurnCommand::outer(Face::Down, RotationAmount::CounterClockwise),
        ];
        apply_algorithm(&mut state, &scramble);
        let solution = solve_ida2x2_with_timeout(&state, Duration::from_secs(5))
            .expect("solve should succeed");
        assert!(
            verify_solution(&state, &solution),
            "ida2x2 should solve complex state. Solution length: {}",
            solution.len()
        );
    }

    // -----------------------------------------------------------------------
    // 200-move random scrambles
    // -----------------------------------------------------------------------

    #[test]
    fn test_2x2_200_scramble_seed_1() {
        let order = 2u32;
        let scramble = generate_scramble(order, 200, 1);
        let mut state = CubeState::solved(CubeOrder::new(order).expect("valid"));
        apply_turns(&mut state, &scramble).expect("scramble should apply");

        let solution = solve_ida2x2_with_timeout(&state, Duration::from_secs(10))
            .expect("solve should succeed");
        assert!(
            verify_solution(&state, &solution),
            "ida2x2 should solve 200-move scramble (seed 1). Solution length: {}",
            solution.len()
        );
    }

    #[test]
    fn test_2x2_200_scramble_seed_42() {
        let order = 2u32;
        let scramble = generate_scramble(order, 200, 42);
        let mut state = CubeState::solved(CubeOrder::new(order).expect("valid"));
        apply_turns(&mut state, &scramble).expect("scramble should apply");

        let solution = solve_ida2x2_with_timeout(&state, Duration::from_secs(10))
            .expect("solve should succeed");
        assert!(
            verify_solution(&state, &solution),
            "ida2x2 should solve 200-move scramble (seed 42). Solution length: {}",
            solution.len()
        );
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_2x2_already_solved() {
        let state = CubeState::solved(CubeOrder::new(2).expect("valid"));
        let solution = solve_ida2x2_with_timeout(&state, Duration::from_secs(5))
            .expect("solve should succeed");
        assert!(
            verify_solution(&state, &solution),
            "ida2x2 solving already-solved state should work"
        );
    }

    #[test]
    fn test_2x2_quarter_turn_cycle() {
        let mut state = CubeState::solved(CubeOrder::new(2).expect("valid"));
        let turn = TurnCommand::outer(Face::Right, RotationAmount::Clockwise);
        for _ in 0..4 {
            apply_turn_to_state(&mut state, turn).expect("turn should apply");
        }
        assert!(is_solved(&state), "4x R should return to solved");

        let solution = solve_ida2x2_with_timeout(&state, Duration::from_secs(5))
            .expect("solve should succeed");
        assert!(verify_solution(&state, &solution));
    }

    #[test]
    fn test_2x2_inverse_of_solution() {
        let order = 2u32;
        let scramble = generate_scramble(order, 20, 777);
        let mut state = CubeState::solved(CubeOrder::new(order).expect("valid"));
        apply_turns(&mut state, &scramble).expect("scramble should apply");

        let solution = solve_ida2x2_with_timeout(&state, Duration::from_secs(5))
            .expect("solve should succeed");
        assert!(verify_solution(&state, &solution));

        let mut verify = state.clone();
        for &turn in &solution {
            apply_turn_to_state(&mut verify, turn).expect("solution turn should be valid");
        }
        assert!(is_solved(&verify), "applying solution should restore solved state");
    }
}
