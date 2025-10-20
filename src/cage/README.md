# Cage Library Documentation

## Running vmmap.rs Unit Tests

To run all vmmap tests (first navigate to the cage lib):
```bash
cargo test --lib memory::vmmap
```

To run a specific test:
```bash
cargo test --lib test_change_prot_entire_region
```

To run tests with output:
```bash
cargo test --lib memory::vmmap -- --nocapture
```

## Vmmap.rs Unit Test Suite Summary

### 1. Memory Protection Change Tests (`change_prot`)

These tests verify that the `change_prot` function correctly modifies memory protection flags while preserving other entry attributes.


**Key Insight**: `change_prot` only modifies the `prot` field while preserving all other entry attributes including `maxprot`, `backing`, `flags`, etc.

### 2. Entry Overwrite Behavior Tests (`add_entry_with_overwrite`)

These tests clarify what "overwrite" actually means in the context of adding memory entries.


**Key Insight**: "Overwrite" means existing overlapping entries are replaced or split, not merged. The new entry's attributes completely replace the old entry's attributes in the overlapping region.

### 3. Address Space Allocation Tests (`find_map_space_with_hint`)

These tests clarify parameter expectations and search behavior for finding available address space.

**Key Insight**: All parameters to `find_map_space_with_hint` are in PAGE UNITS, not byte addresses. This includes:
- `hint`: starting page number for search
- `npages`: number of pages needed
- `pages_per_map`: alignment requirement in pages
- Return value: interval of page numbers

## Important Clarifications from Unit Tests
1. **Page-Based Operations**: All vmmap operations use page numbers internally, not byte addresses. This aligns with the underlying system memory model and ensures efficient address space management.

2. **Entry Splitting**: When operations modify only part of an existing memory entry, the system automatically creates new entries for unchanged portions while preserving their original attributes. This ensures fine-grained control over memory regions.

3. **Attribute Preservation**: Protection changes (via `change_prot`) preserve all entry metadata including `maxprot`, backing type, flags, and other attributes. Only the requested protection field is modified.

4. **Overwrite Semantics**: The `add_entry_with_overwrite` function replaces overlapping entries rather than merging them. New entry attributes completely override old attributes in the overlapping region, with automatic splitting for partial overlaps.

5. **Strict vs Overwrite Modes**: 
   - `add_entry`: Rejects any overlapping entries (strict mode)
   - `add_entry_with_overwrite`: Handles overlaps by splitting and replacing existing entries as needed


