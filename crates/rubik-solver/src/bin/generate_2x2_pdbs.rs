// generate_2x2_pdbs — pre-generate pattern databases for the 2x2 IDA* solver.
//
// Usage:
//   cargo run --bin generate_2x2_pdbs -- <perm_output> <orient_output>
//
// Generates two BFS-based PDBs:
//   - perm_pdb.bin (40,320 bytes): min moves to solve corner permutation
//   - orient_pdb.bin (2,187 bytes): min moves to solve corner orientation

use rubik_solver::ida2x2;
use std::collections::VecDeque;
use std::io::Write;

const NUM_PERM: usize = 40320;
const NUM_ORIENT: usize = 2187;
const NUM_MOVES: usize = 18;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <perm_output> <orient_output>", args[0]);
        std::process::exit(1);
    }

    let tables = ida2x2::move_tables();
    let goals = ida2x2::goal_states();

    // Generate permutation PDB
    eprintln!("Generating permutation PDB ({} entries)...", NUM_PERM);
    let mut perm_pdb = vec![255u8; NUM_PERM];
    let mut queue = VecDeque::new();
    for &coord in goals.perm_coords() {
        perm_pdb[coord as usize] = 0;
        queue.push_back(coord);
    }

    while let Some(coord) = queue.pop_front() {
        let dist = perm_pdb[coord as usize] + 1;
        let base = coord as usize * NUM_MOVES;
        for m in 0..NUM_MOVES {
            let next = tables.perm_move[base + m];
            if perm_pdb[next as usize] == 255 {
                perm_pdb[next as usize] = dist;
                queue.push_back(next);
            }
        }
    }

    let perm_reachable = perm_pdb.iter().filter(|&&d| d != 255).count();
    let perm_max = perm_pdb.iter().max().unwrap_or(&0);
    eprintln!(
        "  reachable: {}/{}, max depth: {}",
        perm_reachable, NUM_PERM, perm_max
    );

    // Generate orientation PDB
    eprintln!("Generating orientation PDB ({} entries)...", NUM_ORIENT);
    let mut orient_pdb = vec![255u8; NUM_ORIENT];
    let mut queue = VecDeque::new();
    for &coord in goals.orient_coords() {
        orient_pdb[coord as usize] = 0;
        queue.push_back(coord);
    }

    while let Some(coord) = queue.pop_front() {
        let dist = orient_pdb[coord as usize] + 1;
        let base = coord as usize * NUM_MOVES;
        for m in 0..NUM_MOVES {
            let next = tables.orient_move[base + m];
            if orient_pdb[next as usize] == 255 {
                orient_pdb[next as usize] = dist;
                queue.push_back(next);
            }
        }
    }

    let orient_reachable = orient_pdb.iter().filter(|&&d| d != 255).count();
    let orient_max = orient_pdb.iter().max().unwrap_or(&0);
    eprintln!(
        "  reachable: {}/{}, max depth: {}",
        orient_reachable, NUM_ORIENT, orient_max
    );

    // Write output files
    eprintln!("Writing {}...", args[1]);
    let mut f = std::fs::File::create(&args[1]).expect("failed to create perm PDB file");
    f.write_all(&perm_pdb).expect("failed to write perm PDB");

    eprintln!("Writing {}...", args[2]);
    let mut f = std::fs::File::create(&args[2]).expect("failed to create orient PDB file");
    f.write_all(&orient_pdb)
        .expect("failed to write orient PDB");

    eprintln!("Done.");
}
