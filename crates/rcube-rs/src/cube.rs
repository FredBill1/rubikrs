// cube.rs — translated from RCube's Cube.h + Cube.cpp
//
// Based on RCube (https://github.com/ShellPuppy/RCube/tree/c0e6df125db141eaf0044bf5a39cb54c942cfab2)
// Original C++ code copyright (C) ShellPuppy and contributors, GPL v3
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// Face mapping (matches rubik-core):
//   F(0) = Green, R(1) = Red,   B(2) = Blue
//   L(3) = Orange, U(4) = White, D(5) = Yellow

use std::sync::atomic::{AtomicBool, Ordering};

use crate::constants;
use crate::face::Face;

// ---------------------------------------------------------------------------
// MoveRecord — a single (face, depth, q) move, matching Cube::Move() callsite
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct MoveRecord {
    pub face: u8,
    pub depth: u32,
    /// 1=Clockwise, 2=HalfTurn, -1(or 3)=CounterClockwise
    pub q: i32,
}

// ---------------------------------------------------------------------------
// Cube struct
// ---------------------------------------------------------------------------

pub struct Cube {
    pub row_size: u32,
    pub r1: u32,
    pub mid: u32,
    pub is_even: bool,
    pub faces: [Face; 6],
    pub stage: i32,
    pub qstate: i32,
    pub iteration: u32,
    pub edge_state: [bool; 12],
    pub move_count: u64,
    pub recorded_moves: Vec<MoveRecord>,
}

impl Cube {
    // =========================================================================
    // Constructor — translated from Cube::Cube(int) + Cube::Initalize + Reset
    // =========================================================================

    pub fn new(row_size: u32) -> Self {
        let r1 = row_size.saturating_sub(1);
        let mid = row_size >> 1;
        let is_even = (row_size & 1) == 0;
        let mem_size = row_size.next_power_of_two();

        let faces: [Face; 6] = std::array::from_fn(|i| Face::new(i as u8, row_size, mem_size));

        Self {
            row_size,
            r1,
            mid,
            is_even,
            faces,
            stage: 0,
            qstate: 0,
            iteration: 0,
            edge_state: [false; 12],
            move_count: 0,
            recorded_moves: Vec::new(),
        }
    }

    // =========================================================================
    // RotateX — rotates a slice in the Y-Z plane (Left/Right faces)
    // =========================================================================

    #[inline]
    pub fn rotate_x(&mut self, index: u32, step: i32) {
        if index == 0 {
            self.faces[3].rotate_face_cw(-step);
        }
        if index == self.r1 {
            self.faces[1].rotate_face_cw(step);
        }

        // Destructure so we can mutably access f0, f4, f2, f5 simultaneously
        let [f0, _f1, f2, _f3, f4, f5] = &mut self.faces;

        let mut p0 = f0.get_pos(0, index);
        let mut p4 = f4.get_pos(0, index);
        let mut p2 = f2.get_pos(self.r1, self.r1 - index);
        let mut p5 = f5.get_pos(0, index);

        let d0 = f0.get_delta(3);
        let d4 = f4.get_delta(3);
        let d2 = f2.get_delta(1);
        let d5 = f5.get_delta(3);

        let i0 = ((0i32.wrapping_sub(step & 3)) & 3) as usize;
        let i1 = ((1i32.wrapping_sub(step & 3)) & 3) as usize;
        let i2 = ((2i32.wrapping_sub(step & 3)) & 3) as usize;
        let i3 = ((3i32.wrapping_sub(step & 3)) & 3) as usize;

        for _ in 0..self.row_size {
            let b = [
                f0.data[p0 as usize],
                f4.data[p4 as usize],
                f2.data[p2 as usize],
                f5.data[p5 as usize],
            ];

            f0.data[p0 as usize] = b[i0];
            f4.data[p4 as usize] = b[i1];
            f2.data[p2 as usize] = b[i2];
            f5.data[p5 as usize] = b[i3];

            p0 += d0;
            p4 += d4;
            p2 += d2;
            p5 += d5;
        }
    }

    // =========================================================================
    // RotateY — rotates a slice in the X-Y plane (Up/Down faces)
    // =========================================================================

    #[inline]
    pub fn rotate_y(&mut self, index: u32, step: i32) {
        if index == 0 {
            self.faces[5].rotate_face_cw(step);
        }
        if index == self.r1 {
            self.faces[4].rotate_face_cw(-step);
        }

        let [f0, f1, f2, f3, _f4, _f5] = &mut self.faces;

        let mut p0 = f0.get_pos(index, 0);
        let mut p1 = f1.get_pos(index, 0);
        let mut p2 = f2.get_pos(index, 0);
        let mut p3 = f3.get_pos(index, 0);

        let d0 = f0.get_delta(0);
        let d1 = f1.get_delta(0);
        let d2 = f2.get_delta(0);
        let d3 = f3.get_delta(0);

        let i0 = ((0i32.wrapping_sub(step & 3)) & 3) as usize;
        let i1 = ((1i32.wrapping_sub(step & 3)) & 3) as usize;
        let i2 = ((2i32.wrapping_sub(step & 3)) & 3) as usize;
        let i3 = ((3i32.wrapping_sub(step & 3)) & 3) as usize;

        for _ in 0..self.row_size {
            let b = [
                f0.data[p0 as usize],
                f1.data[p1 as usize],
                f2.data[p2 as usize],
                f3.data[p3 as usize],
            ];

            f0.data[p0 as usize] = b[i0];
            f1.data[p1 as usize] = b[i1];
            f2.data[p2 as usize] = b[i2];
            f3.data[p3 as usize] = b[i3];

            p0 += d0;
            p1 += d1;
            p2 += d2;
            p3 += d3;
        }
    }

    // =========================================================================
    // RotateZ — rotates a slice in the X-Z plane (Front/Back faces)
    // =========================================================================

    #[inline]
    pub fn rotate_z(&mut self, index: u32, step: i32) {
        if index == 0 {
            self.faces[0].rotate_face_cw(step);
        }
        if index == self.r1 {
            self.faces[2].rotate_face_cw(-step);
        }

        let [_f0, f1, _f2, f3, f4, f5] = &mut self.faces;

        let mut p1 = f1.get_pos(0, index);
        let mut p5 = f5.get_pos(self.r1 - index, 0);
        let mut p3 = f3.get_pos(self.r1, self.r1 - index);
        let mut p4 = f4.get_pos(index, self.r1);

        let d1 = f1.get_delta(3);
        let d5 = f5.get_delta(0);
        let d3 = f3.get_delta(1);
        let d4 = f4.get_delta(2);

        let i0 = ((0i32.wrapping_sub(step & 3)) & 3) as usize;
        let i1 = ((1i32.wrapping_sub(step & 3)) & 3) as usize;
        let i2 = ((2i32.wrapping_sub(step & 3)) & 3) as usize;
        let i3 = ((3i32.wrapping_sub(step & 3)) & 3) as usize;

        for _ in 0..self.row_size {
            let b = [
                f1.data[p1 as usize],
                f5.data[p5 as usize],
                f3.data[p3 as usize],
                f4.data[p4 as usize],
            ];

            f1.data[p1 as usize] = b[i0];
            f5.data[p5 as usize] = b[i1];
            f3.data[p3 as usize] = b[i2];
            f4.data[p4 as usize] = b[i3];

            p1 += d1;
            p5 += d5;
            p3 += d3;
            p4 += d4;
        }
    }

    // =========================================================================
    // Public move — routes face/depth/q to the correct rotation axis
    // =========================================================================

    pub fn move_face(&mut self, face: u8, depth: u32, q: i32) {
        self.move_count += 1;
        self.recorded_moves.push(MoveRecord { face, depth, q });

        match face {
            0 => self.rotate_z(depth, q),                 // F
            1 => self.rotate_x(self.r1 - depth, q),        // R
            2 => self.rotate_z(self.r1 - depth, -q),       // B
            3 => self.rotate_x(depth, -q),                 // L
            4 => self.rotate_y(self.r1 - depth, -q),       // U
            5 => self.rotate_y(depth, q),                  // D
            _ => {}
        }
    }

    // =========================================================================
    // Solve — main entry point, coordinates centers → corners → edges
    // =========================================================================

    pub fn solve(&mut self, cancelled: &AtomicBool) {
        // Cube Size 1: Trivial case
        if self.row_size == 1 {
            self.move_count += 1;
            for i in 0u8..6u8 {
                self.faces[i as usize].set_rc(0, 0, i);
            }
            return;
        }

        // Size 2: Only have to solve corners
        if self.row_size == 2 {
            self.solve_corners(cancelled);
            return;
        }

        // Odd size cubes need to have their center pieces aligned first
        self.align_true_centers();

        if cancelled.load(Ordering::Relaxed) {
            return;
        }

        // Stage 0 through 14 (solve centers)
        if self.stage <= 14 {
            self.solve_centers(cancelled);
        }

        if cancelled.load(Ordering::Relaxed) {
            return;
        }

        // Stage 15 (solve corners)
        if self.stage == 15 {
            self.solve_corners(cancelled);
        }

        if cancelled.load(Ordering::Relaxed) {
            return;
        }

        // Stage 16 (solve edges)
        if self.stage == 16 {
            if self.is_even {
                self.solve_edges_even(cancelled);
            } else {
                self.solve_edges_odd(cancelled);
                if cancelled.load(Ordering::Relaxed) {
                    return;
                }
                if !self.is_cube_solved() {
                    self.solve_edges_odd(cancelled);
                }
            }
        }
    }

    // =========================================================================
    // Centers — solve centers (15 stages)
    // =========================================================================

    fn solve_centers(&mut self, cancelled: &AtomicBool) {
        // No need to solve centers for cubes less than size 4
        if self.row_size < 4 {
            self.stage = 15;
            return;
        }

        // Stage 0: Push R color pieces from F to R
        if self.stage == 0 {
            self.push_center_pieces(
                constants::F,
                constants::R,
                constants::R,
                cancelled,
            );
            self.stage += 1;
        }

        // Stage 1: Push R color pieces from U to R
        if self.stage == 1 {
            self.push_center_pieces(
                constants::U,
                constants::R,
                constants::R,
                cancelled,
            );
            self.stage += 1;
        }

        // Stage 2: Push R color pieces from B to R
        if self.stage == 2 {
            self.push_center_pieces(
                constants::B,
                constants::R,
                constants::R,
                cancelled,
            );
            self.stage += 1;
        }

        // Stage 3: Push R color pieces from L to R
        if self.stage == 3 {
            self.push_center_pieces(
                constants::L,
                constants::R,
                constants::R,
                cancelled,
            );
            self.stage += 1;
        }

        // Stage 4: Push R color pieces from D to R
        if self.stage == 4 {
            self.push_center_pieces(
                constants::D,
                constants::R,
                constants::R,
                cancelled,
            );
            self.stage += 1;
        }

        // Stage 5: Push L color pieces from U to L
        if self.stage == 5 {
            self.push_center_pieces(
                constants::U,
                constants::L,
                constants::L,
                cancelled,
            );
            self.stage += 1;
        }

        // Stage 6: Push L color pieces from D to L
        if self.stage == 6 {
            self.push_center_pieces(
                constants::D,
                constants::L,
                constants::L,
                cancelled,
            );
            self.stage += 1;
        }

        // Stage 7: Push L color pieces from B to L
        if self.stage == 7 {
            self.push_center_pieces(
                constants::B,
                constants::L,
                constants::L,
                cancelled,
            );
            self.stage += 1;
        }

        // Stage 8: Push L color pieces from F to L
        if self.stage == 8 {
            self.push_center_pieces(
                constants::F,
                constants::L,
                constants::L,
                cancelled,
            );
            self.stage += 1;
        }

        // Stage 9: Push F color pieces from B to F
        if self.stage == 9 {
            self.push_center_pieces(
                constants::B,
                constants::F,
                constants::F,
                cancelled,
            );
            self.stage += 1;
        }

        // Stage 10: Push F color pieces from U to F
        if self.stage == 10 {
            self.push_center_pieces(
                constants::U,
                constants::F,
                constants::F,
                cancelled,
            );
            self.stage += 1;
        }

        // Stage 11: Push F color pieces from D to F
        if self.stage == 11 {
            self.push_center_pieces(
                constants::D,
                constants::F,
                constants::F,
                cancelled,
            );
            self.stage += 1;
        }

        // Stage 12: Push D color pieces from U to D
        if self.stage == 12 {
            self.push_center_pieces(
                constants::U,
                constants::D,
                constants::D,
                cancelled,
            );
            self.stage += 1;
        }

        // Stage 13: Push D color pieces from B to D
        if self.stage == 13 {
            self.push_center_pieces(
                constants::B,
                constants::D,
                constants::D,
                cancelled,
            );
            self.stage += 1;
        }

        // Stage 14: Push U color pieces from B to U
        if self.stage == 14 {
            self.push_center_pieces(
                constants::U,
                constants::B,
                constants::B,
                cancelled,
            );
            self.stage += 1;
        }
    }

    // =========================================================================
    // AlignTrueCenters — align the center piece of an odd sized cube
    // =========================================================================

    fn align_true_centers(&mut self) {
        if self.is_even {
            return; // skip for even size cubes
        }

        // Find Front Center piece
        let mut q: u8 = 0;
        for i in 0u8..6u8 {
            if self.faces[i as usize].get_rc(self.mid, self.mid) == constants::F {
                q = i;
            }
        }

        // Move Front Center piece to the front
        if q == constants::U {
            self.move_face(constants::L, self.mid, 1);
        }
        if q == constants::D {
            self.move_face(constants::L, self.mid, -1);
        }
        if q == constants::L {
            self.move_face(constants::U, self.mid, -1);
        }
        if q == constants::R {
            self.move_face(constants::U, self.mid, 1);
        }
        if q == constants::B {
            self.move_face(constants::U, self.mid, 2);
        }

        // Find Up Center piece
        for i in 0u8..6u8 {
            if self.faces[i as usize].get_rc(self.mid, self.mid) == constants::U {
                q = i;
            }
        }

        // Move up to the top
        if q == constants::D {
            self.move_face(constants::F, self.mid, 2);
        }
        if q == constants::L {
            self.move_face(constants::F, self.mid, 1);
        }
        if q == constants::R {
            self.move_face(constants::F, self.mid, -1);
        }
    }

    // =========================================================================
    // IsOpposite — returns true if src and dst faces are opposites
    // =========================================================================

    fn is_opposite(&self, src: u8, dst: u8) -> bool {
        if (src == 0 && dst == 2) || (src == 2 && dst == 0) {
            return true;
        }
        if (src == 1 && dst == 3) || (src == 3 && dst == 1) {
            return true;
        }
        if (src == 4 && dst == 5) || (src == 5 && dst == 4) {
            return true;
        }
        false
    }

    // =========================================================================
    // FindCommutatorMap — find the correct commutator map index
    // =========================================================================

    fn find_commutator_map(&self, src: u8, dst: u8) -> usize {
        for i in 0..30 {
            if constants::CMAP[i][0] == src && constants::CMAP[i][1] == dst {
                return i;
            }
        }
        0
    }

    // =========================================================================
    // PushCenterPieces — push center pieces of a color from one face to another
    // =========================================================================

    fn push_center_pieces(
        &mut self,
        src: u8,
        dst: u8,
        color: u8,
        cancelled: &AtomicBool,
    ) {
        let map = self.find_commutator_map(src, dst);

        let srcl = constants::CMAP[map][2]; // face 'left' of src (in direction of dst)
        let sq = -(constants::CMAP[map][4] as i32); // quadrant on source face
        let dq = -(constants::CMAP[map][5] as i32); // destination rotation relative to source
        let d: i32 = if self.is_opposite(src, dst) { 2 } else { 1 };

        let mut mstack: Vec<u32> = vec![0u32; self.mid as usize];

        let mut start = self.mid;

        // If starting from a saved state, set the start point
        if self.iteration > 0 {
            start = self.iteration;
        }

        let src_idx = src as usize;
        let dst_idx = dst as usize;

        for quadrant in self.qstate..4 {
            self.qstate = quadrant;

            for r in start..self.r1 {
                self.iteration = r;

                if cancelled.load(Ordering::Relaxed) {
                    return;
                }

                loop {
                    let mut pieces: u32 = 0;
                    let mut stkptr: usize = 0;

                    for c in 1..self.mid {
                        if self.faces[src_idx].get_rcq(r, c, sq) == color {
                            pieces += 1;
                            if self.faces[dst_idx].get_rcq(r, c, dq) != color {
                                mstack[stkptr] = c;
                                stkptr += 1;
                            }
                        }
                    }

                    // The row is clear — move on
                    if pieces == 0 {
                        break;
                    }

                    // The row is not clear but has no valid moves
                    if stkptr == 0 {
                        self.move_face(dst, 0, 1);
                        continue;
                    }

                    // Apply the commutator as actual Move calls (unoptimized path)
                    // This ensures moves are correctly recorded via move_face.
                    // Equivalent to the commented-out C++ code in PushCenterPieces.
                    for i in 0..stkptr {
                        self.move_face(srcl, mstack[i], -d);
                    }
                    self.move_face(dst, 0, 1);
                    self.move_face(srcl, r, -d);
                    self.move_face(dst, 0, -1);
                    for i in 0..stkptr {
                        self.move_face(srcl, mstack[i], d);
                    }
                    self.move_face(dst, 0, 1);
                    self.move_face(srcl, r, d);
                }
            }

            // Reset the start point
            start = self.mid;

            // Rotate the src face to prepare for the next quadrant
            self.move_face(src, 0, 1);
        }

        self.qstate = 0;
        self.iteration = 0;
    }

    // =========================================================================
    // Scramble — MSVC-compatible rand() to match C++ RCube binary
    // =========================================================================

    pub fn scramble(&mut self, seed: u32) {
        // MSVC-compatible rand()
        struct Rng { state: u32 }
        impl Rng {
            fn srand(&mut self, s: u32) { self.state = s; }
            fn rand(&mut self) -> u32 {
                self.state = self.state.wrapping_mul(214013).wrapping_add(2531011);
                (self.state >> 16) & 0x7FFF
            }
        }

        let mut rng = Rng { state: seed };
        let row_size = self.row_size;

        for _r in 0..3u32 {
            rng.srand(seed);

            for _ in 0..(3 * row_size) {
                let rnd = rng.rand() % 3;
                if rnd == 0 { self.rotate_x(rng.rand() % row_size, (rng.rand() as i32 & 3) + 1); }
                if rnd == 1 { self.rotate_y(rng.rand() % row_size, (rng.rand() as i32 & 3) + 1); }
                if rnd == 2 { self.rotate_z(rng.rand() % row_size, (rng.rand() as i32 & 3) + 1); }
            }

            for i in 0..row_size {
                self.rotate_x(i, (rng.rand() as i32 & 3) + 1);
                self.rotate_y(i, (rng.rand() as i32 & 3) + 1);
                self.rotate_z(i, (rng.rand() as i32 & 3) + 1);
            }

            for _ in 0..(3 * row_size) {
                let rnd = rng.rand() % 3;
                if rnd == 0 { self.rotate_x(rng.rand() % row_size, (rng.rand() as i32 & 3) + 1); }
                if rnd == 1 { self.rotate_y(rng.rand() % row_size, (rng.rand() as i32 & 3) + 1); }
                if rnd == 2 { self.rotate_z(rng.rand() % row_size, (rng.rand() as i32 & 3) + 1); }
            }

            let seed = (seed + 1) % 0x0FFFFFF;
            rng.srand(seed);
        }
    }

    // =========================================================================
    // Corners
    // =========================================================================

    fn solve_corners(&mut self, cancelled: &AtomicBool) {
        // Solve the U face corners
        for i in 0..4 {
            if cancelled.load(Ordering::Relaxed) {
                return;
            }

            let pos = self.find_corner(i);

            match pos {
                0 => {
                    self.move_face(constants::L, 0, 1);
                    self.move_face(constants::D, 0, 1);
                    self.move_face(constants::L, 0, -1);
                }
                1 => {
                    self.move_face(constants::L, 0, -1);
                    self.move_face(constants::D, 0, 2);
                    self.move_face(constants::L, 0, 1);
                }
                2 => {
                    self.move_face(constants::R, 0, 1);
                    self.move_face(constants::D, 0, 1);
                    self.move_face(constants::R, 0, -1);
                    self.move_face(constants::D, 0, 2);
                }
                3 => {
                    self.move_face(constants::R, 0, -1);
                    self.move_face(constants::D, 0, -1);
                    self.move_face(constants::R, 0, 1);
                    self.move_face(constants::D, 0, 1);
                }
                4 => {
                    self.move_face(constants::D, 0, 2);
                }
                5 => {
                    self.move_face(constants::D, 0, 1);
                }
                6 => {
                    // corner is already in position
                }
                7 => {
                    self.move_face(constants::D, 0, -1);
                }
                _ => {}
            }

            let mut pos = self.find_corner(i);

            while !(pos == 3
                && self.faces[constants::U as usize].get_rc(0, self.r1) == constants::U)
            {
                self.move_face(constants::R, 0, -1);
                self.move_face(constants::D, 0, -1);
                self.move_face(constants::R, 0, 1);
                self.move_face(constants::D, 0, 1);
                pos = self.find_corner(i);
            }

            if i < 3 {
                self.move_face(constants::U, 0, -1);
            }
        }

        // Temporarily move the U face corners to the D face
        self.move_face(constants::L, 0, 2);
        self.move_face(constants::R, 0, 2);

        // --------------------------------------------------
        // Solve the D face corners
        // --------------------------------------------------

        // Put one corner in a known position
        let pos = self.find_corner(4);
        if pos == 0 {
            self.move_face(constants::U, 0, -1);
        }
        if pos == 1 {
            self.move_face(constants::U, 0, 2);
        }
        if pos == 2 {
            self.move_face(constants::U, 0, 1);
        }

        // The remaining corners can end up in 6 different configurations
        let mut c = [0i32; 3];
        {
            let fp = |cr: usize| self.find_corner(cr) as usize;
            c[fp(5)] = 5;
            c[fp(6)] = 6;
            c[fp(7)] = 7;
        }

        // Solve each configuration
        if c[0] == 5 && c[1] == 6 && c[2] == 7 {
            self.move_face(constants::U, 0, 1);
        }

        if c[0] == 5 && c[1] == 7 && c[2] == 6 {
            self.move_face(constants::U, 0, 1);
            self.flip_corners();
            self.flip_corners();
            self.move_face(constants::U, 0, -1);
        }

        if c[0] == 6 && c[1] == 5 && c[2] == 7 {
            self.move_face(constants::U, 0, 2);
            self.flip_corners();
            self.flip_corners();
            self.move_face(constants::U, 0, 2);
        }

        if c[0] == 6 && c[1] == 7 && c[2] == 5 {
            self.flip_corners();
            self.flip_corners();
            self.move_face(constants::U, 0, 1);
        }

        if c[0] == 7 && c[1] == 5 && c[2] == 6 {
            self.flip_corners();
            self.move_face(constants::U, 0, 1);
        }

        if c[0] == 7 && c[1] == 6 && c[2] == 5 {
            self.flip_corners();
            self.move_face(constants::U, 0, -1);
            self.flip_corners();
            self.move_face(constants::U, 0, -1);
        }

        // Force all of the D face colors in the same direction
        for _ in 0..4 {
            while self.faces[constants::U as usize].get_rc(0, self.r1) != constants::D {
                self.move_face(constants::R, 0, -1);
                self.move_face(constants::D, 0, -1);
                self.move_face(constants::R, 0, 1);
                self.move_face(constants::D, 0, 1);
            }
            self.move_face(constants::U, 0, 1);
        }

        // Push the D corners to the D face and bring the U face corners up
        self.move_face(constants::L, 0, 2);
        self.move_face(constants::R, 0, 2);

        self.stage = 16;
    }

    // =========================================================================
    // FlipCorners — rotate 3 corners on the U face
    // =========================================================================

    fn flip_corners(&mut self) {
        self.move_face(constants::U, 0, 1);
        self.move_face(constants::R, 0, 1);
        self.move_face(constants::U, 0, -1);
        self.move_face(constants::L, 0, -1);
        self.move_face(constants::U, 0, 1);
        self.move_face(constants::R, 0, -1);
        self.move_face(constants::U, 0, -1);
        self.move_face(constants::L, 0, 1);
    }

    // =========================================================================
    // GetCorner — reads the 3 face colors for a corner
    // =========================================================================

    fn get_corner(&self, cr: usize) -> (u8, u8, u8) {
        match cr {
            0 => (
                self.faces[4].get_rc(0, 0),          // U(0,0)
                self.faces[0].get_rc(self.r1, 0),     // F(R1,0)
                self.faces[3].get_rc(self.r1, self.r1), // L(R1,R1)
            ),
            1 => (
                self.faces[4].get_rc(self.r1, 0),     // U(R1,0)
                self.faces[3].get_rc(self.r1, 0),     // L(R1,0)
                self.faces[2].get_rc(self.r1, self.r1), // B(R1,R1)
            ),
            2 => (
                self.faces[4].get_rc(self.r1, self.r1), // U(R1,R1)
                self.faces[1].get_rc(self.r1, self.r1), // R(R1,R1)
                self.faces[2].get_rc(self.r1, 0),     // B(R1,0)
            ),
            3 => (
                self.faces[4].get_rc(0, self.r1),     // U(0,R1)
                self.faces[0].get_rc(self.r1, self.r1), // F(R1,R1)
                self.faces[1].get_rc(self.r1, 0),     // R(R1,0)
            ),
            4 => (
                self.faces[5].get_rc(0, 0),          // D(0,0)
                self.faces[3].get_rc(0, 0),           // L(0,0)
                self.faces[2].get_rc(0, self.r1),     // B(0,R1)
            ),
            5 => (
                self.faces[5].get_rc(self.r1, 0),     // D(R1,0)
                self.faces[0].get_rc(0, 0),           // F(0,0)
                self.faces[3].get_rc(0, self.r1),     // L(0,R1)
            ),
            6 => (
                self.faces[5].get_rc(self.r1, self.r1), // D(R1,R1)
                self.faces[0].get_rc(0, self.r1),     // F(0,R1)
                self.faces[1].get_rc(0, 0),           // R(0,0)
            ),
            7 => (
                self.faces[5].get_rc(0, self.r1),     // D(0,R1)
                self.faces[1].get_rc(0, self.r1),     // R(0,R1)
                self.faces[2].get_rc(0, 0),           // B(0,0)
            ),
            _ => (0, 0, 0),
        }
    }

    // =========================================================================
    // IsCorner — returns true if the corner in position cr has given colors
    // =========================================================================

    fn is_corner(&self, cr: usize, c0: u8, c1: u8, c2: u8) -> bool {
        let (b0, b1, b2) = self.get_corner(cr);
        (c0 == b0 || c0 == b1 || c0 == b2)
            && (c1 == b0 || c1 == b1 || c1 == b2)
            && (c2 == b0 || c2 == b1 || c2 == b2)
    }

    // =========================================================================
    // FindCorner — finds the position of corner cr
    // =========================================================================

    fn find_corner(&self, cr: usize) -> i32 {
        for i in 0..8 {
            if self.is_corner(
                i,
                constants::CORNERS[cr][0],
                constants::CORNERS[cr][1],
                constants::CORNERS[cr][2],
            ) {
                return i as i32;
            }
        }
        -1
    }

    // =========================================================================
    // Edges — Odd size
    // =========================================================================

    fn solve_edges_odd(&mut self, cancelled: &AtomicBool) {
        let mut mstack: Vec<u32> = vec![0u32; self.row_size as usize];

        // Reset the edge solve states
        self.edge_state = [false; 12];

        // Pre-check for edges that have flipped center pieces (avoid parity problems)
        for de in 0usize..12 {
            if cancelled.load(Ordering::Relaxed) {
                return;
            }

            // Move current edge so that it's on the right side front face
            self.set_destination_edge(de as i32, true);

            // Fix issue with the right center edge piece being backwards
            let (r0, r1) = self.get_right_edge_colors(self.mid);

            for i in 0usize..12 {
                let c0 = constants::EDGE_COLOR_MAP[2 * i];
                let c1 = constants::EDGE_COLOR_MAP[2 * i + 1];

                if r0 == c1 && r1 == c0 {
                    self.move_face(constants::D, self.mid, 1);
                    self.flip_right_edge();
                    self.move_face(constants::D, self.mid, -1);
                    self.unflip_right_edge();
                }
            }

            self.set_destination_edge(de as i32, false);
        }

        // For each of the 12 edges
        for de in 0usize..12 {
            if cancelled.load(Ordering::Relaxed) {
                return;
            }

            // Move current edge so that it's on the right side front face
            self.set_destination_edge(de as i32, true);

            // Get the two colors for this edge
            let c0 = constants::EDGE_COLOR_MAP[2 * de];
            let c1 = constants::EDGE_COLOR_MAP[2 * de + 1];

            // Loop through all other edges
            for se in 0usize..12 {
                // Skip this edge if it's already been solved
                if self.edge_state[se] {
                    continue;
                }

                // Move edge so it's on the left side of the front face
                self.set_source_edge(se as i32, true);

                // Fix issue with the right center edge piece being backwards
                let (r0, r1) = self.get_right_edge_colors(self.mid);
                if r0 == c1 && r1 == c0 && (de as i32) < 11 {
                    self.move_face(constants::D, self.mid, 1);
                    self.flip_right_edge();
                    self.move_face(constants::D, self.mid, -1);
                    self.unflip_right_edge();
                }

                let mut found;
                loop {
                    found = false;

                    // ---- Step 1a ----
                    // Find pieces on the left that can be moved to the right
                    let mut mptr: usize = 0;
                    for r in 1..self.r1 {
                        let (l0, l1) = self.get_left_edge_colors(r);
                        let (r0, r1) = self.get_right_edge_colors(r);

                        // Piece exists on the left?
                        if (l0 == c0 && l1 == c1) || (l0 == c1 && l1 == c0) {
                            // No piece exists on the right?
                            if !((r0 == c0 && r1 == c1) || (r0 == c1 && r1 == c0)) {
                                if r != self.mid {
                                    mstack[mptr] = r;
                                    mptr += 1;
                                }
                            }
                        }
                    }

                    // ---- Step 1b ----
                    // Move pieces from the left to the right
                    if mptr > 0 {
                        found = true;

                        for i in 0..mptr {
                            if mstack[i] < self.mid {
                                self.move_face(constants::D, mstack[i], 1);
                                self.move_face(constants::D, self.r1 - mstack[i], 1);
                            }
                        }
                        self.flip_right_edge();
                        for i in 0..mptr {
                            if mstack[i] < self.mid {
                                self.move_face(constants::D, mstack[i], -1);
                            }
                        }
                        self.unflip_right_edge();
                        for i in 0..mptr {
                            if mstack[i] < self.mid {
                                self.move_face(constants::D, self.r1 - mstack[i], -1);
                            }
                        }

                        for i in 0..mptr {
                            if mstack[i] >= self.mid {
                                self.move_face(constants::D, mstack[i], 1);
                                self.move_face(constants::D, self.r1 - mstack[i], 1);
                            }
                        }
                        self.flip_right_edge();
                        for i in 0..mptr {
                            if mstack[i] >= self.mid {
                                self.move_face(constants::D, mstack[i], -1);
                            }
                        }
                        self.unflip_right_edge();
                        for i in 0..mptr {
                            if mstack[i] >= self.mid {
                                self.move_face(constants::D, self.r1 - mstack[i], -1);
                            }
                        }
                    }

                    // ---- Step 2a ----
                    // Find pieces on the left that can be moved to the right
                    mptr = 0;
                    for r in 1..self.r1 {
                        let (l0, l1) = self.get_left_edge_colors(r);
                        let (r0, r1) = self.get_right_edge_colors(r);

                        // Piece exists on the left?
                        if (l0 == c0 && l1 == c1) || (l0 == c1 && l1 == c0) {
                            // Also a piece exists on the right?
                            if ((r0 == c0 && r1 == c1) || (r0 == c1 && r1 == c0))
                                && r != self.mid
                            {
                                mstack[mptr] = r;
                                mptr += 1;
                            }
                        }
                    }

                    // ---- Step 2b ----
                    // Move pieces from the left to the right
                    if mptr > 0 {
                        found = true;
                        self.flip_right_edge();
                        for i in 0..mptr {
                            self.move_face(constants::D, mstack[i], 1);
                        }
                        self.unflip_right_edge();
                        for i in 0..mptr {
                            self.move_face(constants::D, mstack[i], -1);
                        }
                    }

                    // ---- Step 3 ----
                    // Move center edge pieces from left to right
                    let (l0, l1) = self.get_left_edge_colors(self.mid);
                    if l0 == c0 && l1 == c1 {
                        self.flip_left_edge();
                        self.move_center_edge(false);
                        self.unflip_left_edge();
                    }

                    if l0 == c1 && l1 == c0 {
                        self.move_center_edge(false);
                    }

                    if !found {
                        break;
                    }
                }

                // Move edge back to its original location
                self.set_source_edge(se as i32, false);
            }

            // Fix remaining parity issues for this edge
            for r in 1..self.mid {
                let (r0, r1) = self.get_right_edge_colors(r);
                if r0 == c1 && r1 == c0 {
                    self.fix_parity(r);
                }
            }

            // Move edge back to the correct face and orientation
            self.set_destination_edge(de as i32, false);

            // This edge is now solved
            self.edge_state[de] = true;
        }
    }

    // =========================================================================
    // Edges — Even size
    // =========================================================================

    fn solve_edges_even(&mut self, cancelled: &AtomicBool) {
        let mut mstack: Vec<u32> = vec![0u32; self.row_size as usize];

        // Reset the edge solve states
        self.edge_state = [false; 12];

        // For each of the 12 edges
        for de in 0usize..12 {
            if cancelled.load(Ordering::Relaxed) {
                return;
            }

            // Move current edge so that it's on the right side front face
            self.set_destination_edge(de as i32, true);

            // Get the two colors for this edge
            let c0 = constants::EDGE_COLOR_MAP[2 * de];
            let c1 = constants::EDGE_COLOR_MAP[2 * de + 1];

            // Loop through all other edges
            for se in 0usize..12 {
                // Skip this edge if it's already been solved
                if self.edge_state[se] {
                    continue;
                }

                // Move edge so it's on the left side of the front face
                self.set_source_edge(se as i32, true);

                let mut found;
                loop {
                    found = false;

                    // ---- Step 1a ----
                    // Find pieces on the left that can be moved to the right
                    let mut mptr: usize = 0;
                    for r in 1..self.r1 {
                        let (l0, l1) = self.get_left_edge_colors(r);
                        let (r0, r1) = self.get_right_edge_colors(r);

                        // Piece exists on the left?
                        if (l0 == c0 && l1 == c1) || (l0 == c1 && l1 == c0) {
                            // No piece exists on the right?
                            if !((r0 == c0 && r1 == c1) || (r0 == c1 && r1 == c0)) {
                                mstack[mptr] = r;
                                mptr += 1;
                            }
                        }
                    }

                    // ---- Step 1b ----
                    // Move pieces from the left to the right
                    if mptr > 0 {
                        found = true;

                        for i in 0..mptr {
                            if mstack[i] < self.mid {
                                self.move_face(constants::D, mstack[i], 1);
                                self.move_face(constants::D, self.r1 - mstack[i], 1);
                            }
                        }
                        self.flip_right_edge();
                        for i in 0..mptr {
                            if mstack[i] < self.mid {
                                self.move_face(constants::D, mstack[i], -1);
                            }
                        }
                        self.unflip_right_edge();
                        for i in 0..mptr {
                            if mstack[i] < self.mid {
                                self.move_face(constants::D, self.r1 - mstack[i], -1);
                            }
                        }

                        for i in 0..mptr {
                            if mstack[i] >= self.mid {
                                self.move_face(constants::D, mstack[i], 1);
                                self.move_face(constants::D, self.r1 - mstack[i], 1);
                            }
                        }
                        self.flip_right_edge();
                        for i in 0..mptr {
                            if mstack[i] >= self.mid {
                                self.move_face(constants::D, mstack[i], -1);
                            }
                        }
                        self.unflip_right_edge();
                        for i in 0..mptr {
                            if mstack[i] >= self.mid {
                                self.move_face(constants::D, self.r1 - mstack[i], -1);
                            }
                        }
                    }

                    // ---- Step 2a ----
                    // Find pieces on the left that can be moved to the right
                    mptr = 0;
                    for r in 1..self.r1 {
                        let (l0, l1) = self.get_left_edge_colors(r);
                        let (r0, r1) = self.get_right_edge_colors(r);

                        // Piece exists on the left?
                        if (l0 == c0 && l1 == c1) || (l0 == c1 && l1 == c0) {
                            // Also a piece exists on the right?
                            if (r0 == c0 && r1 == c1) || (r0 == c1 && r1 == c0) {
                                mstack[mptr] = r;
                                mptr += 1;
                            }
                        }
                    }

                    // ---- Step 2b ----
                    // Move pieces from the left to the right
                    if mptr > 0 && (mptr as u32) < self.row_size {
                        found = true;
                        self.flip_right_edge();
                        for i in 0..mptr {
                            self.move_face(constants::D, mstack[i], 1);
                        }
                        self.unflip_right_edge();
                        for i in 0..mptr {
                            self.move_face(constants::D, mstack[i], -1);
                        }
                    }

                    if !found {
                        break;
                    }
                }

                // Move edge back to its original location
                self.set_source_edge(se as i32, false);
            }

            // Fix remaining parity issues for this edge
            for r in 1..self.mid {
                let (r0, r1) = self.get_right_edge_colors(r);
                if r0 == c1 && r1 == c0 {
                    self.fix_parity(r);
                }
            }

            // Move edge back to the correct face and orientation
            self.set_destination_edge(de as i32, false);

            // This edge is now solved
            self.edge_state[de] = true;
        }
    }

    // =========================================================================
    // FlipRightEdge — flips the F-R edge
    // =========================================================================

    fn flip_right_edge(&mut self) {
        self.move_face(constants::R, 0, 1);
        self.update_edge_rotation(constants::R, 1);
        self.move_face(constants::U, 0, 1);
        self.update_edge_rotation(constants::U, 1);
        self.move_face(constants::R, 0, -1);
        self.update_edge_rotation(constants::R, -1);
        self.move_face(constants::F, 0, 1);
        self.update_edge_rotation(constants::F, 1);
        self.move_face(constants::R, 0, -1);
        self.update_edge_rotation(constants::R, -1);
        self.move_face(constants::F, 0, -1);
        self.update_edge_rotation(constants::F, -1);
        self.move_face(constants::R, 0, 1);
        self.update_edge_rotation(constants::R, 1);
    }

    // =========================================================================
    // UnFlipRightEdge — un-flips the F-R edge
    // =========================================================================

    fn unflip_right_edge(&mut self) {
        self.move_face(constants::R, 0, -1);
        self.update_edge_rotation(constants::R, -1);
        self.move_face(constants::F, 0, 1);
        self.update_edge_rotation(constants::F, 1);
        self.move_face(constants::R, 0, 1);
        self.update_edge_rotation(constants::R, 1);
        self.move_face(constants::F, 0, -1);
        self.update_edge_rotation(constants::F, -1);
        self.move_face(constants::R, 0, 1);
        self.update_edge_rotation(constants::R, 1);
        self.move_face(constants::U, 0, -1);
        self.update_edge_rotation(constants::U, -1);
        self.move_face(constants::R, 0, -1);
        self.update_edge_rotation(constants::R, -1);
    }

    // =========================================================================
    // FlipLeftEdge — flips the F-L edge
    // =========================================================================

    fn flip_left_edge(&mut self) {
        self.move_face(constants::L, 0, -1);
        self.update_edge_rotation(constants::L, -1);
        self.move_face(constants::U, 0, -1);
        self.update_edge_rotation(constants::U, -1);
        self.move_face(constants::L, 0, 1);
        self.update_edge_rotation(constants::L, 1);
        self.move_face(constants::F, 0, -1);
        self.update_edge_rotation(constants::F, -1);
        self.move_face(constants::L, 0, 1);
        self.update_edge_rotation(constants::L, 1);
        self.move_face(constants::F, 0, 1);
        self.update_edge_rotation(constants::F, 1);
        self.move_face(constants::L, 0, -1);
        self.update_edge_rotation(constants::L, -1);
    }

    // =========================================================================
    // UnFlipLeftEdge — un-flips the F-L edge
    // =========================================================================

    fn unflip_left_edge(&mut self) {
        self.move_face(constants::L, 0, 1);
        self.update_edge_rotation(constants::L, 1);
        self.move_face(constants::F, 0, -1);
        self.update_edge_rotation(constants::F, -1);
        self.move_face(constants::L, 0, -1);
        self.update_edge_rotation(constants::L, -1);
        self.move_face(constants::F, 0, 1);
        self.update_edge_rotation(constants::F, 1);
        self.move_face(constants::L, 0, -1);
        self.update_edge_rotation(constants::L, -1);
        self.move_face(constants::U, 0, 1);
        self.update_edge_rotation(constants::U, 1);
        self.move_face(constants::L, 0, 1);
        self.update_edge_rotation(constants::L, 1);
    }

    // =========================================================================
    // MoveCenterEdge — move front face center edge from left to right side
    // =========================================================================

    fn move_center_edge(&mut self, flipped: bool) {
        // Find unsolved center edge on the U face
        let mut q: i32 = -1;

        if !self.edge_state[6] {
            q = 1;
        }
        if !self.edge_state[2] {
            q = 2;
        }
        if !self.edge_state[5] {
            q = 3;
        }
        if !self.edge_state[10] {
            q = 0;
        }

        if q >= 0 {
            if q > 0 {
                self.move_face(constants::U, 0, -q);
            }

            self.move_face(constants::L, self.mid, 2);
            self.move_face(constants::F, 0, 1);
            self.move_face(constants::L, self.mid, -1);
            self.move_face(constants::F, 0, 2);
            self.move_face(constants::L, self.mid, 1);
            self.move_face(constants::F, 0, 1);
            self.move_face(constants::L, self.mid, 2);

            if q > 0 {
                self.move_face(constants::U, 0, q);
            }

            return;
        }

        // Find unsolved center edge on the D face
        q = -1;

        if !self.edge_state[7] {
            q = 1;
        }
        if !self.edge_state[0] {
            q = 2;
        }
        if !self.edge_state[4] {
            q = 3;
        }
        if !self.edge_state[8] {
            q = 0;
        }

        if q >= 0 {
            if q > 0 {
                self.move_face(constants::D, 0, q);
            }

            self.move_face(constants::L, self.mid, 2);
            self.move_face(constants::F, 0, -1);
            self.move_face(constants::L, self.mid, 1);
            self.move_face(constants::F, 0, 2);
            self.move_face(constants::L, self.mid, -1);
            self.move_face(constants::F, 0, -1);
            self.move_face(constants::L, self.mid, 2);

            if q > 0 {
                self.move_face(constants::D, 0, -q);
            }

            return;
        }

        if !flipped {
            self.move_face(constants::B, 0, 1);
            self.update_edge_rotation(constants::B, 1);
            self.move_center_edge(true);
            self.move_face(constants::B, 0, -1);
            self.update_edge_rotation(constants::B, -1);
        }
    }

    // =========================================================================
    // GetLeftEdgeColors — edge piece on the left side of the F face
    // =========================================================================

    fn get_left_edge_colors(&self, row: u32) -> (u8, u8) {
        let l0 = self.faces[constants::L as usize].get_rc(row, self.r1);
        let l1 = self.faces[constants::F as usize].get_rc(row, 0);
        (l0, l1)
    }

    // =========================================================================
    // GetRightEdgeColors — edge piece on the right side of the F face
    // =========================================================================

    fn get_right_edge_colors(&self, row: u32) -> (u8, u8) {
        let r0 = self.faces[constants::F as usize].get_rc(row, self.r1);
        let r1 = self.faces[constants::R as usize].get_rc(row, 0);
        (r0, r1)
    }

    // =========================================================================
    // FixParity — fixes edge parity on a row (front right edge only)
    // =========================================================================

    fn fix_parity(&mut self, row: u32) {
        self.move_face(constants::D, row, -1);
        self.move_face(constants::R, 0, 2);
        self.move_face(constants::U, row, 1);
        self.move_face(constants::F, 0, 2);
        self.move_face(constants::U, row, -1);
        self.move_face(constants::F, 0, 2);
        self.move_face(constants::D, row, 2);
        self.move_face(constants::R, 0, 2);
        self.move_face(constants::D, row, 1);
        self.move_face(constants::R, 0, 2);
        self.move_face(constants::D, row, -1);
        self.move_face(constants::R, 0, 2);
        self.move_face(constants::F, 0, 2);
        self.move_face(constants::D, row, 2);
        self.move_face(constants::F, 0, 2);
    }

    // =========================================================================
    // SetDestinationEdge — prepare an edge to be solved, or put it back
    // =========================================================================

    fn set_destination_edge(&mut self, edge: i32, set: bool) {
        if set {
            match edge {
                0 => {
                    self.move_face(constants::D, 0, -1);
                    self.update_edge_rotation(constants::D, -1);
                    self.move_face(constants::R, 0, 1);
                    self.update_edge_rotation(constants::R, 1);
                }
                1 => {
                    self.move_face(constants::B, 0, 2);
                    self.update_edge_rotation(constants::B, 2);
                    self.move_face(constants::R, 0, 2);
                    self.update_edge_rotation(constants::R, 2);
                }
                2 => {
                    self.move_face(constants::B, 0, -1);
                    self.update_edge_rotation(constants::B, -1);
                    self.move_face(constants::R, 0, 2);
                    self.update_edge_rotation(constants::R, 2);
                }
                3 => {
                    self.move_face(constants::R, 0, 2);
                    self.update_edge_rotation(constants::R, 2);
                }
                4 => {
                    self.move_face(constants::R, 0, 1);
                    self.update_edge_rotation(constants::R, 1);
                }
                5 => {
                    self.move_face(constants::R, 0, -1);
                    self.update_edge_rotation(constants::R, -1);
                }
                6 => {
                    self.move_face(constants::U, 0, 2);
                    self.update_edge_rotation(constants::U, 2);
                    self.move_face(constants::R, 0, -1);
                    self.update_edge_rotation(constants::R, -1);
                }
                7 => {
                    self.move_face(constants::D, 0, 2);
                    self.update_edge_rotation(constants::D, 2);
                    self.move_face(constants::R, 0, 1);
                    self.update_edge_rotation(constants::R, 1);
                }
                8 => {
                    self.move_face(constants::F, 0, -1);
                    self.update_edge_rotation(constants::F, -1);
                }
                9 => {
                    // F-R edge is already in destination position
                }
                10 => {
                    self.move_face(constants::F, 0, 1);
                    self.update_edge_rotation(constants::F, 1);
                }
                11 => {
                    self.move_face(constants::F, 0, 2);
                    self.update_edge_rotation(constants::F, 2);
                }
                _ => {}
            }
            return;
        }

        // !set — undo the moves
        match edge {
            0 => {
                self.move_face(constants::R, 0, -1);
                self.update_edge_rotation(constants::R, -1);
                self.move_face(constants::D, 0, 1);
                self.update_edge_rotation(constants::D, 1);
            }
            1 => {
                self.move_face(constants::R, 0, 2);
                self.update_edge_rotation(constants::R, 2);
                self.move_face(constants::B, 0, 2);
                self.update_edge_rotation(constants::B, 2);
            }
            2 => {
                self.move_face(constants::R, 0, 2);
                self.update_edge_rotation(constants::R, 2);
                self.move_face(constants::B, 0, 1);
                self.update_edge_rotation(constants::B, 1);
            }
            3 => {
                self.move_face(constants::R, 0, 2);
                self.update_edge_rotation(constants::R, 2);
            }
            4 => {
                self.move_face(constants::R, 0, -1);
                self.update_edge_rotation(constants::R, -1);
            }
            5 => {
                self.move_face(constants::R, 0, 1);
                self.update_edge_rotation(constants::R, 1);
            }
            6 => {
                self.move_face(constants::R, 0, 1);
                self.update_edge_rotation(constants::R, 1);
                self.move_face(constants::U, 0, 2);
                self.update_edge_rotation(constants::U, 2);
            }
            7 => {
                self.move_face(constants::R, 0, -1);
                self.update_edge_rotation(constants::R, -1);
                self.move_face(constants::D, 0, 2);
                self.update_edge_rotation(constants::D, 2);
            }
            8 => {
                self.move_face(constants::F, 0, 1);
                self.update_edge_rotation(constants::F, 1);
            }
            9 => {
                // F-R edge is already in destination position
            }
            10 => {
                self.move_face(constants::F, 0, -1);
                self.update_edge_rotation(constants::F, -1);
            }
            11 => {
                self.move_face(constants::F, 0, 2);
                self.update_edge_rotation(constants::F, 2);
            }
            _ => {}
        }
    }

    // =========================================================================
    // SetSourceEdge — prepare an edge as source or put it back
    // =========================================================================

    fn set_source_edge(&mut self, edge: i32, set: bool) {
        if set {
            match edge {
                0 => {
                    self.move_face(constants::D, 0, 1);
                    self.update_edge_rotation(constants::D, 1);
                    self.move_face(constants::L, 0, -1);
                    self.update_edge_rotation(constants::L, -1);
                }
                1 => {
                    self.move_face(constants::L, 0, 2);
                    self.update_edge_rotation(constants::L, 2);
                }
                2 => {
                    self.move_face(constants::U, 0, -1);
                    self.update_edge_rotation(constants::U, -1);
                    self.move_face(constants::L, 0, 1);
                    self.update_edge_rotation(constants::L, 1);
                }
                3 => {
                    self.move_face(constants::B, 0, 2);
                    self.update_edge_rotation(constants::B, 2);
                    self.move_face(constants::L, 0, 2);
                    self.update_edge_rotation(constants::L, 2);
                }
                4 => {
                    self.move_face(constants::D, 0, 2);
                    self.update_edge_rotation(constants::D, 2);
                    self.move_face(constants::L, 0, -1);
                    self.update_edge_rotation(constants::L, -1);
                }
                5 => {
                    self.move_face(constants::U, 0, 2);
                    self.update_edge_rotation(constants::U, 2);
                    self.move_face(constants::L, 0, 1);
                    self.update_edge_rotation(constants::L, 1);
                }
                6 => {
                    self.move_face(constants::L, 0, 1);
                    self.update_edge_rotation(constants::L, 1);
                }
                7 => {
                    self.move_face(constants::L, 0, -1);
                    self.update_edge_rotation(constants::L, -1);
                }
                8 => {
                    self.move_face(constants::D, 0, -1);
                    self.update_edge_rotation(constants::D, -1);
                    self.move_face(constants::L, 0, -1);
                    self.update_edge_rotation(constants::L, -1);
                }
                9 => {
                    // F-R edge is already in source position
                }
                10 => {
                    self.move_face(constants::U, 0, 1);
                    self.update_edge_rotation(constants::U, 1);
                    self.move_face(constants::L, 0, 1);
                    self.update_edge_rotation(constants::L, 1);
                }
                11 => {
                    // F-L edge is already in source position
                }
                _ => {}
            }
            return;
        }

        // !set — undo
        match edge {
            0 => {
                self.move_face(constants::L, 0, 1);
                self.update_edge_rotation(constants::L, 1);
                self.move_face(constants::D, 0, -1);
                self.update_edge_rotation(constants::D, -1);
            }
            1 => {
                self.move_face(constants::L, 0, 2);
                self.update_edge_rotation(constants::L, 2);
            }
            2 => {
                self.move_face(constants::L, 0, -1);
                self.update_edge_rotation(constants::L, -1);
                self.move_face(constants::U, 0, 1);
                self.update_edge_rotation(constants::U, 1);
            }
            3 => {
                self.move_face(constants::L, 0, 2);
                self.update_edge_rotation(constants::L, 2);
                self.move_face(constants::B, 0, 2);
                self.update_edge_rotation(constants::B, 2);
            }
            4 => {
                self.move_face(constants::L, 0, 1);
                self.update_edge_rotation(constants::L, 1);
                self.move_face(constants::D, 0, 2);
                self.update_edge_rotation(constants::D, 2);
            }
            5 => {
                self.move_face(constants::L, 0, -1);
                self.update_edge_rotation(constants::L, -1);
                self.move_face(constants::U, 0, 2);
                self.update_edge_rotation(constants::U, 2);
            }
            6 => {
                self.move_face(constants::L, 0, -1);
                self.update_edge_rotation(constants::L, -1);
            }
            7 => {
                self.move_face(constants::L, 0, 1);
                self.update_edge_rotation(constants::L, 1);
            }
            8 => {
                self.move_face(constants::L, 0, 1);
                self.update_edge_rotation(constants::L, 1);
                self.move_face(constants::D, 0, 1);
                self.update_edge_rotation(constants::D, 1);
            }
            9 => {
                // F-R edge
            }
            10 => {
                self.move_face(constants::L, 0, -1);
                self.update_edge_rotation(constants::L, -1);
                self.move_face(constants::U, 0, -1);
                self.update_edge_rotation(constants::U, -1);
            }
            11 => {
                // F-L edge
            }
            _ => {}
        }
    }

    // =========================================================================
    // UpdateEdgeRotation — track edge locations as they move around
    // =========================================================================

    fn update_edge_rotation(&mut self, faceid: u8, steps: i32) {
        let e = &constants::EDGE_ROT_MAP[faceid as usize];

        if steps > 0 {
            for _ in 0..steps {
                let tmp = self.edge_state[e[3] as usize];
                self.edge_state[e[3] as usize] = self.edge_state[e[2] as usize];
                self.edge_state[e[2] as usize] = self.edge_state[e[1] as usize];
                self.edge_state[e[1] as usize] = self.edge_state[e[0] as usize];
                self.edge_state[e[0] as usize] = tmp;
            }
        }

        if steps < 0 {
            let abs_steps = steps.unsigned_abs();
            for _ in 0..abs_steps {
                let tmp = self.edge_state[e[0] as usize];
                self.edge_state[e[0] as usize] = self.edge_state[e[1] as usize];
                self.edge_state[e[1] as usize] = self.edge_state[e[2] as usize];
                self.edge_state[e[2] as usize] = self.edge_state[e[3] as usize];
                self.edge_state[e[3] as usize] = tmp;
            }
        }
    }

    // =========================================================================
    // IsCubeSolved — returns true if all faces are in solved state
    // =========================================================================

    pub fn is_cube_solved(&self) -> bool {
        for i in 0..6 {
            if !self.faces[i].is_face_solved() {
                return false;
            }
        }
        true
    }
}
