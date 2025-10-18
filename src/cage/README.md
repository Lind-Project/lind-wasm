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

1. **Page Numbers vs Addresses**: All vmmap operations use page numbers internally, not byte addresses
2. **Entry Splitting**: Operations that modify part of an entry create new entries for unchanged portions
3. **Attribute Preservation**: Protection changes preserve maxprot, backing type, and other metadata
4. **Overwrite Semantics**: "Overwrite" means replace, not merge - new entry attributes completely override old ones
5. **Strict vs Overwrite**: `add_entry` rejects overlaps; `add_entry_with_overwrite` handles them by splitting/replacing


