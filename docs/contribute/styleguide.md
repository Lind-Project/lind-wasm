# Rust Style Guide

## 1. Introduction

This style guide establishes conventions for writing Rust code that is clear, maintainable, and consistent. It is inspired by the principles outlined in the Tcl/Tk Engineering Manual and adapts them to the unique features of Rust.

---

## 2. Code Structure and Organization

### 2.1 Project Structure
- **Modules:** Organize code into modules (`mod`) to group related functionality.
- **File Structure:** Keep each module in its own file where possible. Use a directory structure for nested modules.
- **Naming Conventions:**
  - Filenames: Use `snake_case` (e.g., `data_processing.rs`).
  - Module names: Use `snake_case` (e.g., `data_processing`).

### 2.2 Code Layout
- Limit lines to **80–100 characters** for readability.
- Use **4 spaces** per indentation level.
- Separate logical blocks of code with one blank line.
- Group related constants, type definitions, and functions together.

### 2.3 Comments
- **File Headers:** Begin each file with a brief comment summarizing its purpose.
  List the following:
  - `# Arguments` – Lists function parameters and their descriptions.  
  - `# Returns` – Describes the return value and its type.  
  - `# Panics` – Explains when and why the function may panic.  
  - `# Examples` – Provides usage examples in Rust code blocks.  
  - `# Safety` – Used for `unsafe` functions, outlining preconditions.  

Example:
```rust
/// Computes the area of a rectangle.
///
/// # Arguments
///
/// * `width` - The width of the rectangle.
/// * `height` - The height of the rectangle.
///
/// # Returns
///
/// The computed area as `f64`.
///
/// # Panics
///
/// This function does not explicitly handle negative values.
///
/// # Examples
///
/// ```
/// let area = my_crate::calculate_area(2.0, 3.0);
/// assert_eq!(area, 6.0);
/// ```
fn calculate_area(width: f64, height: f64) -> f64 {
    width * height
}

```

- **Module and Function Documentation:**
  - Use `///` for public items.
  - Use `//!` for module-level documentation.
  
  Example:
```rust
//! # Geometry Module
//!
//! This module provides functions for calculating areas of geometric shapes.
//!
//! # Examples
//!
//! ```
//! use crate::geometry::calculate_area;
//!
//! let area = calculate_area(5.0, 4.0);
//! assert_eq!(area, 20.0);
//! ```
```

- **Inline Comments:** Use `//` sparingly for explanations within code blocks.

---

## 3. Naming Conventions

### 3.1 General Guidelines
- Use **descriptive names** for variables, functions, and types.
- Avoid abbreviations unless they are well-known.
- Be consistent in naming patterns across similar components.

### 3.2 Specific Guidelines
- **Constants:** Use `UPPER_SNAKE_CASE`.
- **Variables:** Use `snake_case`.
- **Functions:** Use `snake_case`.
- **Structs and Enums:** Use `PascalCase`.
- **Traits:** Use `PascalCase` with meaningful names (e.g., `Display`, `Serializable`).
- **Lifetimes:** Use short, single-letter names (e.g., `'a`, `'b`) for common cases.

---

## 4. Coding Practices

### 4.1 Functions
- Functions should have a single responsibility.
- Use **early returns** to reduce nesting.
- Use meaningful parameter names to describe their purpose.

```rust
fn calculate_area(width: f32, height: f32) -> f32 {
    width * height
}
```

### 4.2 Error Handling
- Use `Result` and `Option` for error handling.
- Prefer `?` syntax for propagating errors.
- Include detailed context in errors using crates like `thiserror` or `anyhow`.

### 4.3 Ownership and Borrowing
- Favor **borrowing** over cloning or moving unless ownership is required.
- Explicitly document the ownership model in complex cases.

### 4.4 Iterators and Collections
- Use iterators for collection processing instead of manual loops.
- Use expressive iterator combinators like `map`, `filter`, and `fold`.

```rust
let squares: Vec<i32> = numbers.iter().map(|n| n * n).collect();
```

---

## 5. Style and Formatting

### 5.1 Use of `rustfmt`
- Use `rustfmt` to automatically format code.
- Configure `rustfmt` in `rustfmt.toml` for project-specific preferences.

### 5.2 Imports
- Group `use` statements:
  - Standard library imports first.
  - External crate imports second.
  - Internal module imports last.

```rust
use std::collections::HashMap;
use serde::Serialize;
use crate::utils::parse_data;
```

### 5.3 Attributes
- Place attributes (`#[derive]`, `#[test]`, etc.) on the line immediately above the item.
- Use multi-line formatting for attributes with multiple arguments.

```rust
#[derive(Debug, Clone, PartialEq)]
struct Point {
    x: f32,
    y: f32,
}
```

---

## 6. Testing

### 6.1 Unit Tests
- Write unit tests for all public functions.
- Place tests in a `mod tests` block within the same file.
- Whenever possible, include doctests as mentioned in the [rust style guide](https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html)

```rust
/// Computes the area of a rectangle.
///
/// # Examples
///
/// ```
/// let area = my_crate::calculate_area(2.0, 3.0);
/// assert_eq!(area, 6.0);
/// ```  
fn calculate_area(width: f64, height: f64) -> f64 {
    width * height
}

```

### 6.2 Integration Tests
- Place integration tests in the `tests/` directory.
- Use descriptive filenames and group related tests together.

### 6.3 Test Coverage
- Aim for high test coverage but prioritize testing critical paths.

---

## 7. Unsafe Code
- As mentioned in commenting style above, ensure unsafe functions are documented with a `# Safety` block
- Use `unsafe` sparingly and only when absolutely necessary.
- Encapsulate unsafe code in functions with clear and well-documented safety contracts.

```rust
/// Dereferences a raw pointer.
///
/// # Safety
///
/// - The caller must ensure that `ptr` is non-null and properly aligned.
/// - Accessing the pointer must not cause a data race.
///
/// # Examples
///
/// ```
/// let x = 42;
/// let ptr = &x as *const i32;
/// unsafe {
///     assert_eq!(deref_raw_pointer(ptr), 42);
/// }
/// ```
unsafe fn deref_raw_pointer(ptr: *const i32) -> i32 {
    *ptr
}

```

---

## 8. Documentation

### 8.1 Public API Documentation
- Use `///` comments to document all public types, traits, and functions.
- Include examples in the documentation when possible.

```rust
/// Calculates the area of a rectangle.
///
/// # Arguments:
///
/// * `width` - The width of the rectangle.
/// * `height` - The height of the rectangle.
///
/// # Examples:
///
/// ```
/// let area = calculate_area(2.0, 3.0);
/// assert_eq!(area, 6.0);
/// ```
fn calculate_area(width: f32, height: f32) -> f32 {
    width * height
}
```

### 8.2 Internal Documentation
- Use inline comments (`//`) to explain complex or non-obvious code.
- Avoid redundant comments that restate obvious code behavior.

---

## 9. Dependencies

- Use dependencies sparingly and prefer well-maintained and widely-used crates.
- Regularly audit dependencies for vulnerabilities and updates.
- Pin versions in the `Cargo.toml` to ensure reproducibility.

---

## 10. Performance and Optimization

- Write clear, maintainable code first; optimize only after profiling.
- Use Rust's powerful profiling tools (e.g., `cargo-flamegraph`).
- Avoid premature optimization that sacrifices code readability.

---

## 11. Community Standards

- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/).
- Participate in code reviews and encourage constructive feedback.
- Contribute back to the Rust ecosystem by reporting bugs and submitting patches.

---

## 12. Conclusion

Adhering to this style guide will ensure that Rust codebases remain consistent, maintainable, and aligned with the language's best practices. Regularly revisit and refine these guidelines to adapt to new developments in the Rust ecosystem.

