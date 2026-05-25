// Face struct — translated from RCube's Face.h / Face.cpp
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
// Each face stores a 2D grid of sticker colors (bytes 0-5).
// Uses virtual rotation (orientation field) to avoid physically
// rotating face data — rotation is purely a coordinate remapping.

/// A single face of the cube.
pub struct Face {
    /// Row size minus one (R1 = RowSize - 1)
    pub r1: u32,
    /// Memory row size (power of 2, >= RowSize), used for bit-shift addressing
    pub mem_row_size: u32,
    /// Bit shift = log2(mem_row_size), used for fast row indexing
    pub bitshift: u32,
    /// Total data array size in bytes = mem_row_size * mem_row_size
    pub data_size: u64,
    /// Sticker color data (byte 0-5), sized data_size
    pub data: Vec<u8>,
    /// Virtual orientation of this face [0,1,2,3] — CW rotations by 90°
    pub orientation: i32,
    /// Face identifier (0-5): F=0, R=1, B=2, L=3, U=4, D=5
    pub id: u8,
    /// Actual cube row size (same as Cube::row_size)
    pub row_size: u32,
}

impl Face {
    /// Create a new face with the given id, row size, and memory row size.
    /// Initalizes all stickers to the face's own color.
    pub fn new(id: u8, row_size: u32, mem_row_size: u32) -> Self {
        let r1 = row_size.saturating_sub(1);
        let bitshift = mem_row_size.ilog2();
        let data_size = (mem_row_size as u64) * (mem_row_size as u64);

        let mut face = Self {
            r1,
            mem_row_size,
            bitshift,
            data_size,
            data: vec![0u8; data_size as usize],
            orientation: 0,
            id,
            row_size,
        };

        // Paint entire face with its own color
        face.paint(id);
        face
    }

    /// Virtually rotates this face by q * 90° (does NOT affect other cube faces).
    #[inline]
    pub fn rotate_face_cw(&mut self, q: i32) {
        self.orientation = (self.orientation + q) & 3;
    }

    /// Gets the value of this face at coordinates (r=row, c=col), accounting for virtual rotation.
    #[inline]
    pub fn get_rc(&self, r: u32, c: u32) -> u8 {
        match self.orientation {
            0 => self.data[((r as usize) << self.bitshift) + c as usize],
            1 => self.data[((c as usize) << self.bitshift) + (self.r1 - r) as usize],
            2 => self.data[(((self.r1 - r) as usize) << self.bitshift) + (self.r1 - c) as usize],
            3 => self.data[(((self.r1 - c) as usize) << self.bitshift) + r as usize],
            _ => 0,
        }
    }

    /// Gets the value of this face at (r, c) with an additional rotation q (q=1 means
    /// 90° CW), accounting for both the stored orientation and the extra rotation.
    #[inline]
    pub fn get_rcq(&self, r: u32, c: u32, q: i32) -> u8 {
        let q = (self.orientation - q) & 3;

        match q {
            0 => self.data[((r as usize) << self.bitshift) + c as usize],
            1 => self.data[((c as usize) << self.bitshift) + (self.r1 - r) as usize],
            2 => self.data[(((self.r1 - r) as usize) << self.bitshift) + (self.r1 - c) as usize],
            3 => self.data[(((self.r1 - c) as usize) << self.bitshift) + r as usize],
            _ => 0,
        }
    }

    /// Sets the value of this face at (r=row, c=col).
    #[inline]
    pub fn set_rc(&mut self, r: u32, c: u32, v: u8) {
        match self.orientation {
            0 => {
                self.data[((r as usize) << self.bitshift) + c as usize] = v;
            }
            1 => {
                self.data[((c as usize) << self.bitshift) + (self.r1 - r) as usize] = v;
            }
            2 => {
                self.data[(((self.r1 - r) as usize) << self.bitshift) + (self.r1 - c) as usize] = v;
            }
            3 => {
                self.data[(((self.r1 - c) as usize) << self.bitshift) + r as usize] = v;
            }
            _ => {}
        }
    }

    /// Sets the value of this face at (r, c) with an additional rotation q.
    #[inline]
    pub fn set_rcq(&mut self, r: u32, c: u32, q: i32, v: u8) {
        let q = (self.orientation - q) & 3;

        match q {
            0 => {
                self.data[((r as usize) << self.bitshift) + c as usize] = v;
            }
            1 => {
                self.data[((c as usize) << self.bitshift) + (self.r1 - r) as usize] = v;
            }
            2 => {
                self.data[(((self.r1 - r) as usize) << self.bitshift) + (self.r1 - c) as usize] = v;
            }
            3 => {
                self.data[(((self.r1 - c) as usize) << self.bitshift) + r as usize] = v;
            }
            _ => {}
        }
    }

    /// Returns the offset needed to traverse the data array in a given direction.
    /// d=0: Right, d=1: Down, d=2: Left, d=3: Up
    #[inline]
    pub fn get_delta(&self, d: u32) -> i32 {
        match d {
            0 => self.get_pos(0, 1) - self.get_pos(0, 0), // Right
            1 => self.get_pos(0, 0) - self.get_pos(1, 0), // Down
            2 => self.get_pos(0, 0) - self.get_pos(0, 1), // Left
            3 => self.get_pos(1, 0) - self.get_pos(0, 0), // Up
            _ => 0,
        }
    }

    /// Returns the array index of data that corresponds to row (r) and column (c).
    #[inline]
    pub fn get_pos(&self, r: u32, c: u32) -> i32 {
        match self.orientation {
            0 => ((r as usize) << self.bitshift) as i32 + c as i32,
            1 => ((c as usize) << self.bitshift) as i32 + (self.r1 - r) as i32,
            2 => (((self.r1 - r) as usize) << self.bitshift) as i32 + (self.r1 - c) as i32,
            3 => (((self.r1 - c) as usize) << self.bitshift) as i32 + r as i32,
            _ => 0,
        }
    }

    /// Paint the entire face with a single color.
    pub fn paint(&mut self, color: u8) {
        self.data.fill(color);
    }

    /// Count stickers of a specific color on this face.
    pub fn count(&self, color: u8) -> u32 {
        let mut result = 0u32;
        for r in 0..self.row_size {
            for c in 0..self.row_size {
                if self.get_rc(r, c) == color {
                    result += 1;
                }
            }
        }
        result
    }

    /// Writes counts of each color on this face into the given array.
    pub fn get_counts(&self, cnt: &mut [u32; 6]) {
        for i in 0..6 {
            cnt[i] = 0;
        }
        for r in 0..self.row_size {
            for c in 0..self.row_size {
                cnt[self.get_rc(r, c) as usize] += 1;
            }
        }
    }

    /// Returns true if all stickers on this face match the face id (solved state).
    pub fn is_face_solved(&self) -> bool {
        for r in 0..self.row_size {
            for c in 0..self.row_size {
                if self.get_rc(r, c) != self.id {
                    return false;
                }
            }
        }
        true
    }
}
