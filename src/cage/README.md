# Cage - Virtual Memory Map (vmmap.rs) Tests

## Overview

This document describes the unit test suite for `vmmap.rs`, which implements virtual memory mapping functionality. The tests verify correct behavior of memory protection changes, entry management, and address space allocation.

## Running the Tests

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

## Test Categories

### 1. Memory Protection Change Tests (`change_prot`)

These tests verify that the `change_prot` function correctly modifies memory protection flags while preserving other entry attributes.

#### Key Tests:
- **`test_change_prot_entire_region`** - Verifies that changing protection on an entire region doesn't fragment it
- **`test_change_prot_middle_of_region`** - Confirms proper splitting into 3 parts when modifying middle pages
- **`test_change_prot_beginning_of_region`** - Tests splitting into 2 parts when modifying start pages
- **`test_change_prot_end_of_region`** - Tests splitting into 2 parts when modifying end pages
- **`test_change_prot_spanning_multiple_regions`** - Verifies correct handling across multiple non-contiguous regions
- **`test_change_prot_to_same_value`** - Ensures no fragmentation when protection doesn't actually change
- **`test_change_prot_exact_boundaries`** - Tests single-page modifications and precise boundary handling
- **`test_change_prot_multiple_times`** - Verifies correct state after successive protection changes
- **`test_change_prot_to_none`** - Tests setting protection to `PROT_NONE`
- **`test_change_prot_preserves_backing_type`** - Confirms backing type (Anonymous, SharedMemory, etc.) is preserved
- **`test_change_prot_preserves_maxprot`** - Ensures `maxprot` field remains unchanged

**Key Insight**: `change_prot` only modifies the `prot` field while preserving all other entry attributes including `maxprot`, `backing`, `flags`, etc.

### 2. Entry Overwrite Behavior Tests (`add_entry_with_overwrite`)

These tests clarify what "overwrite" actually means in the context of adding memory entries.

#### Key Tests:
- **`test_add_entry_with_overwrite_replaces_existing_full_overlap`** - Demonstrates that overlapping entries are completely replaced, not merged
- **`test_add_entry_with_overwrite_partial_overlap`** - Shows how partial overlaps cause existing entries to be split
- **`test_add_entry_with_overwrite_removes_completely_covered_entries`** - Verifies that entries fully covered by new entry are removed
- **`test_add_entry_with_overwrite_exact_boundaries`** - Tests behavior at exact boundaries (adjacent but non-overlapping)

**Key Insight**: "Overwrite" means existing overlapping entries are replaced or split, not merged. The new entry's attributes completely replace the old entry's attributes in the overlapping region.

### 3. Address Space Allocation Tests (`find_map_space_with_hint`)

These tests clarify parameter expectations and search behavior for finding available address space.

#### Key Tests:
- **`test_find_map_space_with_hint_uses_page_number`** - **CRITICAL**: Confirms hint parameter is a PAGE NUMBER, not a byte address
- **`test_find_map_space_with_hint_searches_from_hint_page`** - Verifies search begins at the hint page
- **`test_find_map_space_with_hint_zero_hint`** - Shows that hint=0 searches from the beginning
- **`test_find_map_space_with_hint_large_page_number`** - Confirms page-based behavior with large page numbers
- **`test_find_map_space_with_hint_alignment_in_pages`** - Verifies alignment (`pages_per_map`) is also in pages

**Key Insight**: All parameters to `find_map_space_with_hint` are in PAGE UNITS, not byte addresses. This includes:
- `hint`: starting page number for search
- `npages`: number of pages needed
- `pages_per_map`: alignment requirement in pages
- Return value: interval of page numbers

### 4. Additional Function Behavior Tests

#### Key Tests:
- **`test_add_entry_strict_no_overlap`** - Demonstrates that `add_entry` (without overwrite) uses strict insertion and rejects overlaps
- **`test_find_space_returns_none_when_full`** - Verifies proper `None` return when no space is available

## Important Clarifications from Tests

1. **Page Numbers vs Addresses**: All vmmap operations use page numbers internally, not byte addresses
2. **Entry Splitting**: Operations that modify part of an entry create new entries for unchanged portions
3. **Attribute Preservation**: Protection changes preserve maxprot, backing type, and other metadata
4. **Overwrite Semantics**: "Overwrite" means replace, not merge - new entry attributes completely override old ones
5. **Strict vs Overwrite**: `add_entry` rejects overlaps; `add_entry_with_overwrite` handles them by splitting/replacing


