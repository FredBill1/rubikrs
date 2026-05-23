use rcube_rs::cube::Cube;
use std::env;
use std::sync::atomic::AtomicBool;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 { eprintln!("Usage: {} <N> <seed>", args[0]); std::process::exit(1); }
    let n: u32 = args[1].parse().expect("N");
    let seed: u32 = args[2].parse().expect("seed");

    let mut cube = Cube::new(n);
    cube.scramble(seed);
    let cancelled = AtomicBool::new(false);
    let before = std::time::Instant::now();
    cube.solve(&cancelled);
    let dt = before.elapsed().as_secs_f64();
    let ok = cube.is_cube_solved();
    eprintln!("N={} seed={} moves={} solved={} {:.3}s", n, seed, cube.move_count, ok, dt);
    println!("{},{},{},{}", n, seed, cube.move_count, ok);
    if !ok { std::process::exit(2); }
}
