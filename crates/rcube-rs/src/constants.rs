// Constants and lookup tables for RCube solver
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

/// Face identifiers matching rubik-core's color-based Face enum
#[allow(dead_code)]
pub const F: u8 = 0; // Green
#[allow(dead_code)]
pub const R: u8 = 1; // Red
#[allow(dead_code)]
pub const B: u8 = 2; // Blue
#[allow(dead_code)]
pub const L: u8 = 3; // Orange
#[allow(dead_code)]
pub const U: u8 = 4; // White
#[allow(dead_code)]
pub const D: u8 = 5; // Yellow

/// Pairs of edge colors (12 edges, 2 colors each = 24 entries)
/// Edge order: D-B, B-L, B-U, B-R, D-R, U-R, U-L, D-L, F-D, F-R, F-U, F-L
pub const EDGE_COLOR_MAP: [u8; 24] = [
    5, 2, 2, 3, 2, 4, 2, 1, 5, 1, 4, 1, 4, 3, 5, 3, 0, 5, 0, 1, 0, 4, 0, 3,
];

/// Parameters for center commutators (30 entries, 6 values each)
/// Format: [src, dst, srcl, ?, quadrant_src, quadrant_dst]
/// The quadrant_src/quadrant_dst values inform which quadrant of the face to work on.
pub const CMAP: [[u8; 6]; 30] = [
    [0, 1, 4, 5, 3, 3],
    [0, 2, 3, 1, 0, 2],
    [0, 3, 5, 4, 1, 1],
    [0, 4, 3, 1, 0, 0],
    [0, 5, 1, 3, 2, 2],
    [1, 0, 5, 4, 1, 1],
    [1, 2, 4, 5, 3, 3],
    [1, 3, 5, 4, 1, 1],
    [1, 4, 0, 2, 0, 1],
    [1, 5, 2, 0, 2, 1],
    [2, 0, 1, 3, 0, 2],
    [2, 1, 5, 4, 1, 1],
    [2, 3, 4, 5, 3, 3],
    [2, 4, 1, 3, 0, 2],
    [2, 5, 3, 1, 2, 0],
    [3, 0, 4, 5, 3, 3],
    [3, 1, 4, 5, 3, 3],
    [3, 2, 5, 4, 1, 1],
    [3, 4, 2, 0, 0, 3],
    [3, 5, 0, 2, 2, 3],
    [4, 0, 1, 3, 2, 2],
    [4, 1, 2, 0, 3, 2],
    [4, 2, 3, 1, 0, 2],
    [4, 3, 0, 2, 1, 2],
    [4, 5, 0, 2, 1, 3],
    [5, 0, 3, 1, 0, 0],
    [5, 1, 0, 2, 3, 0],
    [5, 2, 1, 3, 2, 0],
    [5, 3, 2, 0, 1, 0],
    [5, 4, 2, 0, 1, 3],
];

/// Corner color definitions, 8 corners, 3 colors each
/// Corner order positions: 0=UFL, 1=ULB, 2=UBR, 3=URF, 4=DLB, 5=DFL, 6=DFR, 7=DRB
pub const CORNERS: [[u8; 3]; 8] = [
    [4, 0, 3], // U-F-L
    [4, 3, 2], // U-L-B
    [4, 1, 2], // U-B-R
    [4, 0, 1], // U-R-F
    [5, 3, 2], // D-L-B
    [5, 0, 3], // D-F-L
    [5, 0, 1], // D-F-R
    [5, 1, 2], // D-R-B
];

/// Edge rotation map — maps face rotations to which EdgeState indices rotate.
/// Each face has 4 edge positions, [a, b, c, d] in order.
/// When a face rotates CW, edge state at position d→a, a→b, b→c, c→d.
/// Edge indices: 0=D-B, 1=B-L, 2=B-U, 3=B-R, 4=D-R, 5=U-R, 6=U-L, 7=D-L, 8=F-D, 9=F-R, 10=F-U, 11=F-L
pub const EDGE_ROT_MAP: [[u8; 4]; 6] = [
    [8, 11, 10, 9], // F
    [3, 4, 9, 5],   // R
    [0, 3, 2, 1],   // B
    [1, 6, 11, 7],  // L
    [2, 5, 10, 6],  // U
    [0, 7, 8, 4],   // D
];
