#![allow(clippy::needless_range_loop)]
#![allow(clippy::manual_range_contains)]

use bitvec::mem::BitMemory;
use bitvec::prelude::*;
use rayon::prelude::*;
use std::fmt::Display;
use std::fmt::Error;
use std::fmt::Formatter;

// Cell values are only 0 (EMPTY) and 1..9 an assigned value.
pub type CellValue = u8;
pub const EMPTY_CELL: CellValue = 0;
const NUM_CELLS: usize = 9 * 9;
// 4 bits are enough to represent 1..9 and EMPTY
const NUM_BITS: usize = NUM_CELLS * 4;

/// A 9x9 Grid for Sudoku compactly represented with 4 bits per cell
/// Because we are compact we support [Copy] to allow easy splitting.
#[derive(Debug, Copy, Clone)]
pub struct Grid {
    cells: BitArr!(for NUM_BITS, in Lsb0, CellValue),
}

impl Grid {
    pub fn new<T: BitMemory + Into<CellValue>>(values: &[T]) -> Grid {
        let mut cells = bitarr![Lsb0, CellValue; 0; NUM_BITS];
        for i in 0..NUM_CELLS {
            let value: CellValue = values[i].into();
            assert!(value >= 1 && value <= 9 || value == EMPTY_CELL);
            cells[Self::get_bit_range(i)].store(value);
        }
        Grid { cells }
    }

    #[inline]
    fn get_bit_range(index: usize) -> std::ops::Range<usize> {
        index * 4..index * 4 + 4
    }

    pub fn get(&self, x: usize, y: usize) -> CellValue {
        let bits = &self.cells[Self::get_bit_range(get_index(x, y))];
        bits.load()
    }

    pub fn set(&mut self, val: CellValue, x: usize, y: usize) {
        let bits = &mut self.cells[Self::get_bit_range(get_index(x, y))];
        bits.store(val);
    }
}

#[inline]
fn get_index(x: usize, y: usize) -> usize {
    assert!(x < 9 && y < 9);
    y * 9 + x
}

impl Display for Grid {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        for y in 0..9 {
            if y % 3 == 0 {
                writeln!(f, "+------+------+-----+")?
            }
            for x in 0..9 {
                if x % 3 == 0 {
                    write!(f, "|")?
                }
                let val = self.get(x, y);
                if val == EMPTY_CELL {
                    write!(f, ".")?;
                } else {
                    write!(f, "{}", val)?;
                }
                if x < 8 {
                    write!(f, " ")?;
                }
            }
            writeln!(f, "|")?;
        }
        writeln!(f, "+------+------+-----+")?;
        Ok(())
    }
}

/// Represents a set of the values 1..9.
#[derive(Clone, Copy, Debug)]
pub struct ValueSet(u16);

impl ValueSet {
    pub fn empty() -> ValueSet {
        ValueSet(0)
    }

    pub fn full() -> ValueSet {
        ValueSet(0b1_1111_1111)
    }

    /// You can call this with [EMPTY_CELL] which is true if the set is empty.
    pub fn contains(&self, value: CellValue) -> bool {
        if value == EMPTY_CELL {
            return false;
        }
        assert!(value >= 1 && value <= 9);
        (self.0 & (1 << (value - 1))) > 0
    }

    pub fn count(&self) -> u8 {
        self.0.count_ones() as u8
    }

    pub fn get_first(&self) -> Option<u8> {
        let trailing = self.0.trailing_zeros() as u8;
        // 0 trailing means 1'th bit is set implying we have 1
        // 8 trailing means 9'th bit is set implying we have 9
        if trailing < 9 {
            Some(trailing + 1)
        } else {
            None
        }
    }

    pub fn add(&mut self, value: CellValue) {
        if value == EMPTY_CELL {
            return;
        }
        assert!(value >= 1 && value <= 9);

        self.0 |= 1 << (value - 1);
    }

    pub fn remove(&mut self, value: CellValue) {
        if value == EMPTY_CELL {
            return;
        }
        assert!(value >= 1 && value <= 9);
        self.0 &= !(1 << (value - 1));
    }

    pub fn clear(&mut self) {
        self.0 = 0;
    }
}

impl IntoIterator for ValueSet {
    type Item = CellValue;
    type IntoIter = ValueSetIterator;
    fn into_iter(self) -> Self::IntoIter {
        ValueSetIterator::new(self)
    }
}

impl FromIterator<CellValue> for ValueSet {
    fn from_iter<I: IntoIterator<Item = CellValue>>(iter: I) -> Self {
        let mut value_set = ValueSet::empty();
        for c in iter {
            value_set.add(c);
        }
        value_set
    }
}

pub struct ValueSetIterator {
    set: ValueSet,
    next: usize,
}

impl ValueSetIterator {
    fn new(value_set: ValueSet) -> Self {
        ValueSetIterator {
            set: value_set,
            next: 0,
        }
    }
}

impl Iterator for ValueSetIterator {
    type Item = CellValue;

    fn next(&mut self) -> Option<Self::Item> {
        while self.next <= 9 {
            self.next += 1;
            if self.set.contains((self.next - 1) as u8) {
                return Some((self.next - 1) as u8);
            }
        }
        None
    }
}

/**
 * A intermediary structure used for solving the Sudoko. Contains a Grid and the up-to-date valid candidates for each cell.
 */
#[derive(Debug, Clone, Copy)]
struct SolveState {
    grid: Grid,
    candidates: [ValueSet; NUM_CELLS],
}

impl SolveState {
    pub fn new(grid: Grid) -> Self {
        // NOTE: Can we do the initialization in one step?
        let mut candidates = [ValueSet::empty(); NUM_CELLS];
        for i in 0..NUM_CELLS {
            candidates[i] = get_candidates(&grid, i % 9, i / 9);
        }
        SolveState { grid, candidates }
    }

    #[inline]
    fn cand_at_mut(&mut self, x: usize, y: usize) -> &mut ValueSet {
        &mut self.candidates[get_index(x, y)]
    }

    #[inline]
    fn cand_at(&self, x: usize, y: usize) -> &ValueSet {
        &self.candidates[get_index(x, y)]
    }

    fn deadlocked(&self) -> bool {
        for y in 0..9 {
            for x in 0..9 {
                if self.grid.get(x, y) == EMPTY_CELL && self.cand_at(x, y).count() == 0 {
                    return true;
                }
            }
        }
        false
    }

    fn assign(&self, val: CellValue, x: usize, y: usize) -> Option<Self> {
        let mut cpy = *self;
        cpy.grid.set(val, x, y);
        cpy.candidates[get_index(x, y)].clear();
        cpy.remove_val_from_peers(val, x, y);
        if self.deadlocked() {
            None
        } else {
            Some(cpy)
        }
    }

    fn remove_val_from_peers(&mut self, val: CellValue, x: usize, y: usize) {
        // Constrain Horizontal
        for cx in 0..9 {
            self.cand_at_mut(cx, y).remove(val);
        }
        // Constrain Vertical
        for cy in 0..9 {
            self.cand_at_mut(x, cy).remove(val);
        }
        // Constrain Quadrant
        {
            let sx = (x / 3) * 3;
            let sy = (y / 3) * 3;
            for cy in sy..sy + 3 {
                for cx in sx..sx + 3 {
                    self.cand_at_mut(cx, cy).remove(val);
                }
            }
        }
    }

    fn is_solved(&self) -> bool {
        for y in 0..9 {
            for x in 0..9 {
                if self.grid.get(x, y) == EMPTY_CELL {
                    return false;
                }
            }
        }
        true
    }

    fn candidate_fewest_choices(&self) -> Option<(ValueSet, usize, usize)> {
        let mut lowest_count = 99;
        let mut best_i: usize = usize::MAX;
        for i in 0..NUM_CELLS {
            let candidate = self.candidates[i];
            let count = candidate.count();
            if count > 0 && count < lowest_count {
                lowest_count = count;
                best_i = i;
            }
        }
        if best_i != usize::MAX {
            Some((self.candidates[best_i], best_i % 9, best_i / 9))
        } else {
            None
        }
    }
}

pub fn get_candidates(grid: &Grid, x: usize, y: usize) -> ValueSet {
    assert!(x < 9 && y < 9);
    let mut candidates = ValueSet::full();

    if grid.get(x, y) != EMPTY_CELL {
        return ValueSet::empty();
    }

    // Scan horizontal
    for cx in 0..9 {
        candidates.remove(grid.get(cx, y));
    }
    // Scan vertical
    for cy in 0..9 {
        candidates.remove(grid.get(x, cy));
    }
    // Scan quadrant
    {
        let sx = (x / 3) * 3;
        let sy = (y / 3) * 3;
        for cy in sy..sy + 3 {
            for cx in sx..sx + 3 {
                candidates.remove(grid.get(cx, cy));
            }
        }
    }

    candidates
}

fn solve_recursive_internal(solve_state: SolveState) -> Option<SolveState> {
    if solve_state.is_solved() {
        return Some(solve_state);
    }
    // Try to fix any slot
    if let Some((cands, x, y)) = solve_state.candidate_fewest_choices() {
        for cand in cands {
            // Works and no deadlock?
            if let Some(branch) = solve_state.assign(cand, x, y) {
                if let Some(result_state) = solve_recursive_internal(branch) {
                    return Some(result_state);
                }
            }
        }
    }
    None
}

fn solve_recursive_internal_par(solve_state: SolveState) -> Option<SolveState> {
    if solve_state.is_solved() {
        return Some(solve_state);
    }
    // Try to fix any slot
    if let Some((cands, x, y)) = solve_state.candidate_fewest_choices() {
        // For some reason this is quite a lot slower.
        // let sub_results = cands.into_iter().par_bridge().map(|c| {

        let cands_arr: Vec<CellValue> = cands.into_iter().collect();
        let sub_results = cands_arr.par_iter().map(|&c| {
            // Works and no deadlock?
            if let Some(branch) = solve_state.assign(c, x, y) {
                if let Some(result_state) = solve_recursive_internal_par(branch) {
                    return Some(result_state);
                }
            }
            None
        });
        return sub_results.find_first(|&st| st.is_some())?;
    }
    None
}

pub fn solve_recursive(grid: Grid) -> Option<Grid> {
    solve_recursive_internal(SolveState::new(grid)).map(|st| st.grid)
}

pub fn solve_recursive_par(grid: Grid) -> Option<Grid> {
    solve_recursive_internal_par(SolveState::new(grid)).map(|st| st.grid)
}

fn is_digit(c: char) -> bool {
    '0' <= c && c <= '9'
}

pub fn parse_grid(text: &str) -> Option<Grid> {
    let nums: Vec<u8> = text
        .chars()
        .filter(|&c| is_digit(c) || c == '.')
        .map(|c| if c == '.' { 0 } else { c.to_digit(10).unwrap() } as u8)
        .collect();
    if nums.len() == NUM_CELLS {
        return Some(Grid::new(&nums));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::hash::Hash;
    #[rustfmt::skip]
    pub const TEST_GRID: &str = "
    4 . . |. . . |8 . 5 
    . 3 . |. . . |. . . 
    . . . |7 . . |. . . 
    ------+------+------
    . 2 . |. . . |. 6 . 
    . . . |. 8 . |4 . . 
    . . . |. 1 . |. . . 
    ------+------+------
    . . . |6 . 3 |. 7 . 
    5 . . |2 . . |. . . 
    1 . 4 |. . . |. . . 
";

    fn contains_same_unordered<T, I1, I2>(a: I1, b: I2) -> bool
    where
        I1: IntoIterator<Item = T>,
        I2: IntoIterator<Item = T>,
        T: Eq + Hash,
    {
        let a: HashSet<T> = a.into_iter().collect();
        let b: HashSet<T> = b.into_iter().collect();
        a == b
    }

    #[test]
    fn candidates() {
        let grid = parse_grid(TEST_GRID).unwrap();
        assert!(contains_same_unordered(
            get_candidates(&grid, 3, 1),
            [1, 4, 5, 8, 9]
        ));
        assert!(contains_same_unordered(
            get_candidates(&grid, 0, 1),
            [2, 6, 7, 8, 9]
        ));
        assert!(contains_same_unordered(
            get_candidates(&grid, 5, 7),
            [1, 4, 7, 8, 9]
        ));
    }

    #[test]
    fn can_solve() {
        let grid = parse_grid(TEST_GRID).unwrap();
        assert!(solve_recursive(grid).is_some());
    }
}
