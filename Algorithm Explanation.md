# High-Performance Slot Map Architecture

A Slot Map is a data structure designed for Data-Oriented Design (DOD). It provides high-performance, contiguous memory access for processing data while maintaining "stable handles" that allow external systems to reference specific data points even as they are moved or deleted in memory.

This specific implementation uses the 4-Array Parallel Structure to ensure maximum cache efficiency.

## The Core Components

To implement this architecture, you maintain four parallel arrays and one management stack.

### A. The Dense Storage (The "What")

These arrays are kept perfectly packed (no holes). This is what you iterate over during rendering or calculation.

1. **Data Array (e.g., Points):** Stores the actual values (e.g., x, y coordinates).

2. **DenseToSparse Array:** A "Back-link" array. Each entry stores the SlotID of the handle that owns the data at this index.

### B. The Sparse Identity (The "Who")

These arrays have holes (they are sparse). Their indices never change, providing a stable "address."

3. **SparseToIndex Array:** Maps a `SlotID` to the current index in the Dense Storage. Inactive slots should store a sentinel value (e.g., -1).

4. **Generations Array:** A counter for each slot that increments every time the slot is reused.

### C. The Manager

**FreeStack:** An array (used as a stack) containing the indices of all currently unused slots in the Sparse arrays.

**ActiveCount:** An integer tracking how many elements currently exist in the Dense arrays.

## The Handle System

A Handle is a 64-bit identifier (Long) that you give to the user. It is bit-packed to include two pieces of information:

- Lower 32 bits: The `SlotID` (Index into the Sparse arrays).

- Upper 32 bits: The `Generation` (The version of that slot).

### Packing (Kotlin/Java):

The following logic assumes unsigned 32-bit integers are packed into a 64-bit long. Using bitwise operations, the handle is constructed as follows:

$$
Handle = (generation.toLong() \ll 32) \lor (slotId.toLong() \land 0xFFFFFFFFL)
$$

### Unpacking

To retrieve the components, the bitwise operations are reversed:

$$
slotId = (handle \land 0xFFFFFFFFL).toInt()
$$

$$
generation = (handle \gg 32).toInt()
$$

## Implementation Workflow

### Create Operation ($O(1)$)

1. Pop a slotId from the FreeStack.
2. Update Identity: 
    - Increment `Generations[slotId]`.
    - Set `SparseToIndex[slotId] = ActiveCount`.
3. Store Data:
    - Set `Data[ActiveCount] = newValue`.
    - Set `DenseToSparse[ActiveCount] = slotId`.
4. Increment `ActiveCount` and return the bit-packed Handle.

### Lookup Operation ($O(1)$)

1. Extract `slotId` and `generation` from the handle.
2. Check if `Generations[slotId] == handle.generation`. If not, the handle is "dead."
3. Resolve: 
    - `val denseIndex = SparseToIndex[slotId]`
    - Return `Data[denseIndex]`.

### Delete/Remove Operation ($O(1)$)

1. Get `indexToRemove` via `SparseToIndex[slotId]`.
2. Identify the last element: `val lastIndex = ActiveCount - 1`.
3. If `indexToRemove < lastIndex`: 
    - Move the last element's data: `Data[indexToRemove] = Data[lastIndex]`.
    - Find who owned that last element: `val ownerSlotId = DenseToSparse[lastIndex]`.
    - Update that owner's map: `SparseToIndex[ownerSlotId] = indexToRemove`.
    - Update the back-link: `DenseToSparse[indexToRemove] = ownerSlotId`.

4. Invalidate:
    - Increment `Generations[slotId]` (so old handles fail validation).

    - Set `SparseToIndex[slotId] = -1` (Set sentinel value).

    - Push `slotId` back onto the `FreeStack`.

5. Decrement `ActiveCount`.

## Why Use This Structure?

### Cache Efficiency

Because the data array is perfectly contiguous, the CPU can use Linear Prefetching. When you loop through your points to draw them, the CPU loads large blocks of coordinates into the L1 cache ahead of time.

### Reference Stability

In an embroidery app, a "Link" (the line between two points) stores a Handle. If you delete points or re-sort the dense array to optimize the needle path, the "Link" never breaks. It always resolves the correct coordinates through the Sparse array.

### Zero-Search Deletion

By using the `DenseToSparse` back-link, you never have to "search" for which handle belongs to the swapped data. Every move is a direct memory write.
