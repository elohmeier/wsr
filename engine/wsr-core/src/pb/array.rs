//! PowerBASIC arrays with arbitrary lower bounds (`DIM a(11 TO 1600)`, `DIM b(0 TO 15, 0 TO 1600)`).
//!
//! Out-of-range access in the original raises error 9 and yields a dummy element at 0x6c3308
//! (`PB_ArrayElem`, 0x6bc34a), zeroed at startup and shared by all such accesses. We mirror that per array:
//! out-of-range reads and writes hit a scratch slot (so a read after an out-of-range write sees that
//! value, as in the original), and every such access is counted in `oob` so tests can assert none happen.

use std::cell::Cell;
use std::ops::{Index, IndexMut};

#[derive(Clone, Debug)]
pub struct Arr1<T> {
    lo: i32,
    hi: i32,
    data: Vec<T>,
    dummy: T,
    oob: Cell<u32>,
}

impl<T: Clone + Default> Arr1<T> {
    pub fn new(lo: i32, hi: i32) -> Self {
        Arr1 { lo, hi, data: vec![T::default(); (hi - lo + 1).max(0) as usize], dummy: T::default(), oob: Cell::new(0) }
    }
    pub fn lbound(&self) -> i32 {
        self.lo
    }
    pub fn ubound(&self) -> i32 {
        self.hi
    }
    /// Number of out-of-range accesses so far (error 9 in the original).
    pub fn oob_count(&self) -> u32 {
        self.oob.get()
    }
    /// Elements in storage order.
    pub fn as_slice(&self) -> &[T] {
        &self.data
    }
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.data
    }
    /// `RESET a()`: all elements back to zero/empty.
    pub fn reset(&mut self) {
        for x in &mut self.data {
            *x = T::default();
        }
    }
    pub fn iter(&self) -> impl Iterator<Item = (i32, &T)> {
        let lo = self.lo;
        self.data.iter().enumerate().map(move |(k, v)| (lo + k as i32, v))
    }
    fn slot(&self, i: i32) -> Option<usize> {
        if i < self.lo || i > self.hi { None } else { Some((i - self.lo) as usize) }
    }
}

impl<T: Clone + Default> Index<i32> for Arr1<T> {
    type Output = T;
    fn index(&self, i: i32) -> &T {
        match self.slot(i) {
            Some(k) => &self.data[k],
            None => {
                self.oob.set(self.oob.get() + 1);
                &self.dummy
            }
        }
    }
}

impl<T: Clone + Default> IndexMut<i32> for Arr1<T> {
    fn index_mut(&mut self, i: i32) -> &mut T {
        match self.slot(i) {
            Some(k) => &mut self.data[k],
            None => {
                self.oob.set(self.oob.get() + 1);
                &mut self.dummy
            }
        }
    }
}

/// Two-dimensional array; `a[(i, j)]` with `i` the first (leftmost) subscript.
#[derive(Clone, Debug)]
pub struct Arr2<T> {
    lo1: i32,
    hi1: i32,
    lo2: i32,
    hi2: i32,
    data: Vec<T>,
    dummy: T,
    oob: Cell<u32>,
}

impl<T: Clone + Default> Arr2<T> {
    pub fn new(lo1: i32, hi1: i32, lo2: i32, hi2: i32) -> Self {
        let n = ((hi1 - lo1 + 1).max(0) * (hi2 - lo2 + 1).max(0)) as usize;
        Arr2 { lo1, hi1, lo2, hi2, data: vec![T::default(); n], dummy: T::default(), oob: Cell::new(0) }
    }
    pub fn bounds(&self) -> ((i32, i32), (i32, i32)) {
        ((self.lo1, self.hi1), (self.lo2, self.hi2))
    }
    pub fn oob_count(&self) -> u32 {
        self.oob.get()
    }
    pub fn reset(&mut self) {
        for x in &mut self.data {
            *x = T::default();
        }
    }
    /// Elements in PB storage order (first subscript fastest).
    pub fn as_slice(&self) -> &[T] {
        &self.data
    }
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.data
    }
    fn slot(&self, i: i32, j: i32) -> Option<usize> {
        if i < self.lo1 || i > self.hi1 || j < self.lo2 || j > self.hi2 {
            return None;
        }
        // PB stores dimension 1 fastest (column-major).
        let w1 = (self.hi1 - self.lo1 + 1) as usize;
        Some((j - self.lo2) as usize * w1 + (i - self.lo1) as usize)
    }
}

impl<T: Clone + Default> Index<(i32, i32)> for Arr2<T> {
    type Output = T;
    fn index(&self, (i, j): (i32, i32)) -> &T {
        match self.slot(i, j) {
            Some(k) => &self.data[k],
            None => {
                self.oob.set(self.oob.get() + 1);
                &self.dummy
            }
        }
    }
}

impl<T: Clone + Default> IndexMut<(i32, i32)> for Arr2<T> {
    fn index_mut(&mut self, (i, j): (i32, i32)) -> &mut T {
        match self.slot(i, j) {
            Some(k) => &mut self.data[k],
            None => {
                self.oob.set(self.oob.get() + 1);
                &mut self.dummy
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_and_oob() {
        let mut a: Arr1<i32> = Arr1::new(11, 85);
        a[11] = 5;
        a[85] = 7;
        assert_eq!(a[11] + a[85], 12);
        assert_eq!(a[10], 0);
        a[10] = 99;
        assert_eq!(a[9], 99); // same dummy slot, like 0x6c3308 in the original
        assert_eq!(a.oob_count(), 3);
        let mut b: Arr2<f64> = Arr2::new(0, 15, 0, 1600);
        b[(3, 1600)] = 1.5;
        assert_eq!(b[(3, 1600)], 1.5);
        assert_eq!(b[(16, 0)], 0.0);
    }
}
