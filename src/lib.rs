//! # SlotManager
//!
//! A high-performance, decoupled slot manager designed for Data-Oriented Design (DOD).
//! It provides stable 64-bit handles to data stored in packed, contiguous arrays.
//!
//! ## Quick Example
//!
//! ```rust
//! use slotmap_microlib::{SlotManager, Handle};
//!
//! let mut manager = SlotManager::new(10);
//! let mut data = Vec::new();
//!
//! // the manager provides the index for adding data
//! let h1 = manager.add(|idx| data.push("Entity A"));
//!
//! // resolve handle to current index
//! if let Some(idx) = manager.get(h1) {
//!     assert_eq!(data[idx], "Entity A");
//! }
//!
//! // synchronize your array via swap-and-pop when removing
//! manager.remove(h1, |last_idx, to_rem_idx| {
//!     data.swap(to_rem_idx, last_idx);
//!     data.pop();
//! });
//! ```
//!
//! ## What is a Slot Map?
//!
//! A Slot Map is a data structure that provides the benefits of both a `Vec` and a `HashMap`.
//! Like a `Vec`, it stores data contiguously in memory for maximum cache efficiency.
//! Like a `HashMap`, it provides stable handles (keys) that remain valid even when the
//! underlying data is moved or deleted.
//!
//! The `SlotManager` implements the remapping logic (4-array parallel architecture)
//! without taking ownership of your data. It acts as a management layer that tells you
//! where to store your data and how to update your indices during removals.
//!
//! ## Comparison with Other Maps
//!
//! The `SlotManager` provides $O(1)$ access via stable handles, combining the performance of
//! a `Vec` with the stability of a `HashMap`. Like a `Vec`, it stores data contiguously for maximum
//! iteration speed and avoids the overhead of a hash function. However, while a `Vec` is unstable
//! because removals shift elements and invalidate indices, the `SlotManager` maintains stable handles
//! throughout the lifecycle of the data, providing the same reliability as a `HashMap` without the
//! cost of non-contiguous memory access.
//!
//! ## When to use this library
//!
//! 1. **High-Performance Loops:** When you need to iterate over thousands of objects (physics, rendering) using linear memory prefetching.
//! 2. **Structure of Arrays (SoA):** When you want to keep parallel arrays (e.g., `positions`, `velocities`) synchronized with a single manager.
//! 3. **Stable References:** When external systems (like a scripting engine or UI) need to hold pointers to entities that are frequently added and removed.
//!
//!
//! ## Wrapper Pattern
//!
//! The `SlotManager` is meant to be used as part of a larger data structure, to internally manage the slots for its objects.
//!
//! ```rust
//! # use slotmap_microlib::{SlotManager, Handle};
//! pub struct ParticleSystem {
//!     pos_x: Vec<f32>,
//!     pos_y: Vec<f32>,
//!     manager: SlotManager,
//! }
//!
//! impl ParticleSystem {
//!     pub fn spawn(&mut self, x: f32, y: f32) -> Handle {
//!         self.manager.add(|idx| {
//!             if idx >= self.pos_x.len() {
//!                 self.pos_x.push(x);
//!                 self.pos_y.push(y);
//!             } else {
//!                 self.pos_x[idx] = x;
//!                 self.pos_y[idx] = y;
//!             }
//!         })
//!     }
//!
//!     pub fn kill(&mut self, handle: Handle) {
//!         self.manager.remove(handle, |last, to_rem| {
//!             self.pos_x.swap(to_rem, last);
//!             self.pos_y.swap(to_rem, last);
//!             self.pos_x.pop();
//!             self.pos_y.pop();
//!         });
//!     }
//! }
//! ```

/// A bit-packed 64-bit handle containing a 32-bit generation and 32-bit slot ID.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Handle(pub u64);

impl Handle {
    /// Constructs a new Handle from a slot index and a generation.
    #[inline]
    pub fn new(slot: u32, generation: u32) -> Self {
        let packed = ((generation as u64) << 32) | (slot as u64 & 0xFFFFFFFF);
        Self(packed)
    }

    /// Returns the generation (high 32 bits) of the handle.
    #[inline]
    pub fn generation(&self) -> u32 {
        (self.0 >> 32) as u32
    }

    /// Returns the slot ID (low 32 bits) of the handle.
    #[inline]
    pub fn slot(&self) -> u32 {
        (self.0 & 0xFFFFFFFF) as u32
    }
}

/// A decoupled manager for handles and indices.
///
/// It maintains the internal mappings required to provide stable handles
/// to elements in a dense array.
#[derive(Debug, Clone)]
pub struct SlotManager {
    dense_to_sparse: Vec<usize>,
    sparse_to_index: Vec<Option<usize>>,
    generations: Vec<u32>,
    free_stack: Vec<u32>,
    size: usize,
}

impl Default for SlotManager {
    /// Creates an empty manager with zero initial capacity.
    fn default() -> Self {
        Self::new(0)
    }
}

impl SlotManager {
    /// Creates a new manager with pre-allocated capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            dense_to_sparse: vec![0; capacity],
            sparse_to_index: vec![None; capacity],
            generations: vec![0; capacity],
            free_stack: (0..capacity as u32).rev().collect(),
            size: 0,
        }
    }

    /// Allocates a new slot and returns its stable Handle.
    ///
    /// `on_add` is called with the dense index where the new data should be placed.
    pub fn add<F>(&mut self, on_add: F) -> Handle
    where
        F: FnOnce(usize),
    {
        if self.free_stack.is_empty() {
            self.grow();
        }

        let slot = self.free_stack.pop().unwrap() as usize;
        self.generations[slot] += 1;
        let g = self.generations[slot];

        self.sparse_to_index[slot] = Some(self.size);
        self.dense_to_sparse[self.size] = slot;

        on_add(self.size);
        self.size += 1;

        Handle::new(slot as u32, g)
    }

    /// Resolves a Handle to its current dense index.
    ///
    /// Returns `None` if the handle is stale (data removed) or invalid.
    #[inline]
    pub fn get(&self, handle: Handle) -> Option<usize> {
        let slot = handle.slot() as usize;
        if slot >= self.generations.len() || self.generations[slot] != handle.generation() {
            return None;
        }
        self.sparse_to_index[slot]
    }

    /// Removes an element and invalidates the handle.
    ///
    /// `on_remove` is called with `(last_index, index_to_remove)` to facilitate
    /// a swap-and-pop on external contiguous storage.
    pub fn remove<F>(&mut self, handle: Handle, on_remove: F) -> bool
    where
        F: FnOnce(usize, usize),
    {
        let slot = handle.slot() as usize;
        if slot >= self.generations.len() || self.generations[slot] != handle.generation() {
            return false;
        }

        if let Some(i_to_remove) = self.sparse_to_index[slot] {
            let last_index = self.size - 1;

            on_remove(last_index, i_to_remove);

            let last_slot = self.dense_to_sparse[last_index];
            self.sparse_to_index[last_slot] = Some(i_to_remove);
            self.dense_to_sparse[i_to_remove] = last_slot;

            self.generations[slot] += 1;
            self.sparse_to_index[slot] = None;
            self.free_stack.push(slot as u32);
            self.size -= 1;

            return true;
        }
        false
    }

    /// Returns true if the handle is valid and active.
    #[inline]
    pub fn exists(&self, handle: Handle) -> bool {
        let slot = handle.slot() as usize;
        slot < self.generations.len() && self.generations[slot] == handle.generation()
    }

    /// Returns an iterator over all active handles in dense memory order.
    pub fn iter(&self) -> impl Iterator<Item = Handle> + '_ {
        self.dense_to_sparse[0..self.size]
            .iter()
            .map(|&slot| Handle::new(slot as u32, self.generations[slot]))
    }

    /// Returns the number of active elements.
    #[inline]
    pub fn len(&self) -> usize {
        self.size
    }

    /// Returns true if no elements are active.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    fn grow(&mut self) {
        let old_cap = self.generations.len();
        let new_cap = if old_cap == 0 { 1 } else { old_cap * 2 };

        self.dense_to_sparse.resize(new_cap, 0);
        self.sparse_to_index.resize(new_cap, None);
        self.generations.resize(new_cap, 0);

        for i in (old_cap..new_cap).rev() {
            self.free_stack.push(i as u32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_workflow() {
        let mut manager = SlotManager::new(10);
        let mut data = Vec::new();

        let h1 = manager.add(|_| data.push("A"));
        let h2 = manager.add(|_| data.push("B"));

        assert_eq!(manager.len(), 2);
        assert_eq!(data[manager.get(h1).unwrap()], "A");

        manager.remove(h1, |last, to_rem| {
            data.swap(to_rem, last);
            data.pop();
        });

        assert!(manager.get(h1).is_none());
        assert_eq!(data[manager.get(h2).unwrap()], "B");
    }

    #[test]
    fn aba_protection() {
        let mut manager = SlotManager::new(1);
        let h1 = manager.add(|_| ());
        manager.remove(h1, |_, _| ());
        let h2 = manager.add(|_| ());

        assert_eq!(h1.slot(), h2.slot());
        assert_ne!(h1.generation(), h2.generation());
        assert!(!manager.exists(h1));
        assert!(manager.exists(h2));
    }
}
