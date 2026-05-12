# slotmap_microlib

[![Crates.io](https://img.shields.io/crates/v/slotmap_microlib.svg)](https://crates.io/crates/slotmap_microlib)


A high-performance, decoupled slot manager designed for Data-Oriented Design (DOD). It provides stable 64-bit handles to data stored in packed, contiguous arrays.

The slot manager is meant to work with a slot map. A slot map is a data structure that combines the memory efficiency of a `Vec` with the stable referencing of a `HashMap`. It maintains data in a perfectly packed "dense" array to leverage CPU cache prefetching while providing stable "handles" that do not break when elements are moved or deleted.

The `SlotManager` implements the 4-array parallel remapping logic without taking ownership of your data. This decoupled approach allows you to manage multiple parallel arrays (Structure of Arrays) or raw memory buffers with a single management layer.

## Quick Start

```rust
use slotmap_microlib::{SlotManager, Handle};

let mut manager = SlotManager::new(10);
let mut data = Vec::new();

// the manager provides the current dense index
let h1 = manager.add(|idx| data.push("Entity A"));

// resolve handle to index
if let Some(idx) = manager.get(h1) {
    println!("Found: {}", data[idx]);
}

// synchronize your array via swap-and-pop
manager.remove(h1, |last_idx, to_rem_idx| {
    data.swap(to_rem_idx, last_idx);
    data.pop();
});
```

## Why not a Vec<T> or a HashMap<K, V>?

The `SlotManager` provides $O(1)$ access via stable handles, combining the performance of a `Vec` with the stability of a `HashMap`. Like a `Vec`, it stores data contiguously for maximum iteration speed and avoids the overhead of a hash function. However, while a `Vec` is unstable because removals shift elements and invalidate indices, the `SlotManager` maintains stable handles throughout the lifecycle of the data, providing the same reliability as a `HashMap` without the cost of non-contiguous memory access.

## Use Cases

1.  **Structure of Arrays (SoA):** Manage parallel vectors (e.g., `pos_x`, `pos_y`, `health`) using one manager.
2.  **High-Performance Loops:** Iterating over thousands of objects for physics or rendering kernels.
3.  **Secondary Maps:** Using `handle.slot()` as a direct index into external arrays for transient data (selection state, etc.), bypassing `HashMap` overhead.
4.  **Minimalism:** A zero-dependency, single-file implementation for easy auditing or integration.

## Key API Reference

### `SlotManager`
- `new(capacity: usize)`: Initializes the manager with pre-allocated slots.
- `add(on_add: F)`: Allocates a slot and returns a `Handle`. Invokes closure with the new dense index.
- `get(handle: Handle)`: Resolves a handle to its current dense index. Returns `None` if stale.
- `remove(handle: Handle, on_remove: F)`: Invalidates handle and provides indices for a swap-and-pop.
- `iter()`: Returns an iterator over all active handles in dense memory order.

### `Handle`
- `slot()`: Returns the stable 32-bit slot ID (useful for secondary maps).
- `generation()`: Returns the 32-bit version counter.

## Usage

The `SlotManager` is meant to be used as part of a larger data structure, to internally manage the slots for objects.

```rust 
use slotmap_microlib::{SlotManager, Handle};

pub struct ParticleSystem {
    pos_x: Vec<f32>,
    pos_y: Vec<f32>,
    manager: SlotManager,
}

impl ParticleSystem {
    pub fn spawn(&mut self, x: f32, y: f32) -> Handle {
        self.manager.add(|idx| {
            if idx >= self.pos_x.len() {
                self.pos_x.push(x);
                self.pos_y.push(y);
            } else {
                self.pos_x[idx] = x;
                self.pos_y[idx] = y;
            }
        })
    }

    pub fn kill(&mut self, handle: Handle) {
        // the manager removes the handle and provides indices for a swap-and-pop
        // it assumes that the last element is moved to the removed position, and the vectors are popped
        self.manager.remove(handle, |last, to_rem| {
            self.pos_x.swap(to_rem, last);
            self.pos_y.swap(to_rem, last);
            self.pos_x.pop();
            self.pos_y.pop();
        });
    }
}

```

This example demonstrates a simple particle system using the `SlotManager`. The manager is intentionally designed to be simple and lightweight, with minimal overhead. This provides more freedom to the struct that uses it to manage its data according to its specific needs.

## Integration

### Single File

Copy `src/lib.rs` into your project. It has zero external dependencies and it functions as a standalone module.

### via Crates.io

Add to `Cargo.toml`:
```toml
slotmap_microlib = "0.1.0"
```

## License

MIT
