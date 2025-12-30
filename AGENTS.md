# AGENTS.md - Rust Development Guidelines

## Overview
This document contains essential best practices and guidelines for AI agents and developers working on Rust projects. Following these principles ensures safe, maintainable, and idiomatic Rust code.

---

## Table of Contents
1. [Clippy Configuration](#clippy-configuration)
2. [Error Handling](#error-handling)
3. [Defensive Programming](#defensive-programming)
4. [Code Quality Standards](#code-quality-standards)
5. [Unsafe Rust](#unsafe-rust)
6. [Testing Guidelines](#testing-guidelines)
7. [Documentation Requirements](#documentation-requirements)
8. [Performance Considerations](#performance-considerations)
9. [Documentation and Research Tools](#documentation-and-research-tools)
10. [UI Libraries](#ui-libraries)
11. [Dependency Management](#dependency-management)
12. [Multi-Crate Workspaces](#multi-crate-workspaces)s

---

## Clippy Configuration

### Enable All Clippy Warnings
Always run Clippy with the strictest settings to catch potential issues early.

**In your `Cargo.toml`:**
```toml
[lints.clippy]
# Deny all warnings - treat them as errors
all = "deny"
pedantic = "deny"
nursery = "deny"
cargo = "deny"
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"

# Unimplemented items can be left as warnings
todo = "warn"
unimplemented = "warn"
```

**Or via command line:**
```bash
cargo clippy -- -D warnings -D clippy::all -D clippy::pedantic -W clippy::nursery
```

### Key Clippy Lints to Enable

```toml
[lints.clippy]
# Correctness (deny)
cast_lossless = "deny"
cast_possible_truncation = "deny"
cast_possible_wrap = "deny"
cast_precision_loss = "deny"
cast_sign_loss = "deny"

# Performance (warn/deny)
inefficient_to_string = "deny"
large_enum_variant = "warn"
large_stack_arrays = "warn"
needless_pass_by_value = "warn"

# Style (warn)
missing_errors_doc = "warn"
missing_panics_doc = "warn"
module_name_repetitions = "warn"
```

---

## Error Handling

### Use `thiserror` for Library Errors
**Never use string-based errors.** Always define explicit error types using `thiserror`.

**Add to `Cargo.toml`:**
```toml
[dependencies]
thiserror = "2.0"
```

**Example Error Definition:**
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MyLibraryError {
    #[error("Failed to parse configuration: {0}")]
    ConfigParseFailed(String),
    
    #[error("Database connection failed")]
    DatabaseConnectionFailed(#[from] sqlx::Error),
    
    #[error("Invalid input: {field} must be {constraint}")]
    ValidationError {
        field: String,
        constraint: String,
    },
    
    #[error("Resource not found: {resource_type} with id {id}")]
    NotFound {
        resource_type: String,
        id: String,
    },
    
    #[error("I/O error")]
    Io(#[from] std::io::Error),
    
    #[error("Serialization error")]
    Serialization(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, MyLibraryError>;
```

### Use `anyhow` for Application Errors
For application code (not libraries), `anyhow` provides convenient error handling with context.

**Add to `Cargo.toml`:**
```toml
[dependencies]
anyhow = "1.0"
```

**Example Usage:**
```rust
use anyhow::{Context, Result};

fn process_file(path: &str) -> Result<String> {
    let content = std::fs::read_to_string(path)
        .context(format!("Failed to read file: {}", path))?;
    
    let parsed = parse_content(&content)
        .context("Failed to parse file content")?;
    
    Ok(parsed)
}
```

### Error Handling Best Practices

1. **Never use `unwrap()`** - Always handle errors explicitly
2. **Never use `expect()`** in production code - Use proper error propagation
3. **Use `?` operator** for error propagation
4. **Add context** to errors using `.context()` or `.with_context()`
5. **Document errors** in function documentation
6. **Create specific error variants** for different failure modes

**Bad:**
```rust
fn bad_example(data: &str) -> String {
    let parsed: u32 = data.parse().unwrap(); // NEVER DO THIS
    format!("Value: {}", parsed)
}
```

**Good:**
```rust
fn good_example(data: &str) -> Result<String, MyLibraryError> {
    let parsed: u32 = data.parse()
        .map_err(|e| MyLibraryError::ValidationError {
            field: "data".to_string(),
            constraint: format!("must be a valid u32: {}", e),
        })?;
    
    Ok(format!("Value: {}", parsed))
}
```

---

## Defensive Programming

### Core Principles

1. **Validate all inputs** at API boundaries
2. **Use the type system** to enforce invariants
3. **Avoid panics** in library code
4. **Check preconditions** explicitly
5. **Handle all error cases** exhaustively

### Input Validation

```rust
pub fn process_user_input(input: &str) -> Result<ProcessedData> {
    // Validate input is not empty
    if input.is_empty() {
        return Err(MyLibraryError::ValidationError {
            field: "input".to_string(),
            constraint: "must not be empty".to_string(),
        });
    }
    
    // Validate length constraints
    if input.len() > MAX_INPUT_LENGTH {
        return Err(MyLibraryError::ValidationError {
            field: "input".to_string(),
            constraint: format!("must not exceed {} characters", MAX_INPUT_LENGTH),
        });
    }
    
    // Continue processing...
    Ok(ProcessedData::new(input))
}
```

### Use Builder Pattern for Complex Types

```rust
#[derive(Debug)]
pub struct Config {
    host: String,
    port: u16,
    timeout: Duration,
}

pub struct ConfigBuilder {
    host: Option<String>,
    port: Option<u16>,
    timeout: Option<Duration>,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self {
            host: None,
            port: None,
            timeout: None,
        }
    }
    
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = Some(host.into());
        self
    }
    
    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }
    
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
    
    pub fn build(self) -> Result<Config> {
        Ok(Config {
            host: self.host.ok_or_else(|| MyLibraryError::ValidationError {
                field: "host".to_string(),
                constraint: "must be specified".to_string(),
            })?,
            port: self.port.ok_or_else(|| MyLibraryError::ValidationError {
                field: "port".to_string(),
                constraint: "must be specified".to_string(),
            })?,
            timeout: self.timeout.unwrap_or(Duration::from_secs(30)),
        })
    }
}
```

### Newtype Pattern for Type Safety

```rust
// Instead of using raw types that can be confused
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductId(u64);

impl UserId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
    
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

// Now you can't accidentally mix up user IDs and product IDs
fn get_user(id: UserId) -> Result<User> {
    // Type system prevents passing ProductId here
    todo!()
}
```

### Safe Index Access

```rust
// Bad: Can panic
let item = vec[index];

// Good: Handle out of bounds
let item = vec.get(index)
    .ok_or(MyLibraryError::NotFound {
        resource_type: "item".to_string(),
        id: index.to_string(),
    })?;
```

### Safe Arithmetic

```rust
// Bad: Can overflow in debug, wraps in release
let result = a + b;

// Good: Handle overflow explicitly
let result = a.checked_add(b)
    .ok_or(MyLibraryError::ValidationError {
        field: "sum".to_string(),
        constraint: "result would overflow".to_string(),
    })?;

// Or use saturating/wrapping explicitly when appropriate
let result = a.saturating_add(b); // When overflow should cap
let result = a.wrapping_add(b);   // When overflow should wrap (make explicit)
```

---

## Code Quality Standards

### Rust Edition and Features

Use the latest stable Rust edition:

```toml
[package]
edition = "2021"
rust-version = "1.75"  # Specify MSRV
```

### Code Organization

```rust
// Module structure should be clear and logical
// src/lib.rs or src/main.rs
mod config;
mod error;
mod models;
mod api;
mod utils;

pub use error::{Error, Result};
pub use config::Config;
```

### Prefer Enums Over Booleans

```rust
// Bad
fn process(data: &str, is_verbose: bool, is_strict: bool) { }

// Good
#[derive(Debug, Clone, Copy)]
pub enum Verbosity {
    Quiet,
    Normal,
    Verbose,
}

#[derive(Debug, Clone, Copy)]
pub enum ValidationMode {
    Lenient,
    Strict,
}

fn process(data: &str, verbosity: Verbosity, validation: ValidationMode) { }
```

### Avoid Stringly-Typed Code

```rust
// Bad
fn get_user_by_type(user_type: &str) -> Result<User> {
    match user_type {
        "admin" => { /* ... */ },
        "regular" => { /* ... */ },
        _ => Err(Error::InvalidUserType),
    }
}

// Good
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserType {
    Admin,
    Regular,
    Guest,
}

fn get_user_by_type(user_type: UserType) -> Result<User> {
    match user_type {
        UserType::Admin => { /* ... */ },
        UserType::Regular => { /* ... */ },
        UserType::Guest => { /* ... */ },
    }
}
```

### Use `#[must_use]` Attribute

```rust
#[must_use = "Result must be used or explicitly ignored"]
pub fn critical_operation() -> Result<Data> {
    // ...
}

#[must_use]
pub fn create_builder() -> Builder {
    Builder::new()
}
```

### Prefer `.into()` for Type Conversions

Use `.into()` instead of specific conversion methods when the type can be inferred. This makes refactoring easier and the code more flexible.

**Why `.into()` is better:**
- Makes refactoring easier - change the target type without changing conversion calls
- More generic - works with any type that implements `Into<T>`
- Cleaner at call sites
- The compiler infers the target type from context

**Examples:**

```rust
// Bad: Specific conversion method
fn process_name(name: String) {
    // ...
}

let name = "Alice".to_string();
process_name(name);

// Good: Use into() for flexibility
fn process_name(name: String) {
    // ...
}

let name = "Alice".into();
process_name(name);

// Even better: Accept impl Into<String>
fn process_name(name: impl Into<String>) {
    let name = name.into();
    // ...
}

process_name("Alice");  // No conversion needed at call site!
```

**More examples:**

```rust
// Bad: Explicit conversions
let path = PathBuf::from("/home/user");
let string = user_input.to_string();
let vec = slice.to_vec();

// Good: Use into() when type is inferred
let path: PathBuf = "/home/user".into();
let string: String = user_input.into();
let vec: Vec<_> = slice.into();

// Best: Let function signature drive inference
fn open_file(path: impl Into<PathBuf>) -> Result<File> {
    let path = path.into();
    File::open(path)
}

open_file("/home/user");  // Clean!
```

**Function signatures with `Into<T>`:**

```rust
// Bad: Rigid signature
pub fn create_user(name: String, email: String) -> User {
    User { name, email }
}

// Caller must convert explicitly
create_user("Alice".to_string(), "alice@example.com".to_string());

// Good: Flexible signature
pub fn create_user(
    name: impl Into<String>,
    email: impl Into<String>,
) -> User {
    User {
        name: name.into(),
        email: email.into(),
    }
}

// Caller can pass &str directly
create_user("Alice", "alice@example.com");
```

**When NOT to use `.into()`:**

```rust
// Don't use when conversion intent should be explicit
let num_str = format!("{}", number);  // Clear: formatting
let parsed: u32 = str.parse()?;       // Clear: parsing

// Don't use when type can't be inferred
let value = something.into();  // Error: can't infer type!
let value: TargetType = something.into();  // OK

// Don't use for fallible conversions - use try_into()
let small: u8 = large_number.try_into()?;  // Can fail
```

**Summary:**
- Default to `.into()` for infallible type conversions
- Use `impl Into<T>` in function signatures for flexibility
- Only use specific methods when clarity demands it
- Always use `.try_into()` for fallible conversions

---

## Unsafe Rust

### Core Principle: Avoid `unsafe` at All Costs

**Unsafe Rust should be treated as a last resort.** The vast majority of Rust code should never need `unsafe` blocks.

### Why Avoid `unsafe`

1. **Defeats Rust's safety guarantees** - You lose memory safety, thread safety, and type safety
2. **Hard to audit** - Requires deep understanding of invariants and undefined behavior
3. **Maintenance burden** - Future changes can introduce subtle bugs
4. **Propagates risk** - Unsafety in one place can affect the entire codebase
5. **Better alternatives exist** - Safe wrappers and crates are usually available

### The `unsafe` Hierarchy (Prefer Earlier Options)

```
1. Pure safe Rust ✅ BEST
   ↓
2. Safe wrapper crates ✅ GOOD
   ↓
3. Well-audited unsafe in dependencies ⚠️ ACCEPTABLE
   ↓
4. Your own unsafe code ❌ AVOID
   ↓
5. Extensive unsafe code ❌❌ NEVER
```

### Use Safe Wrapper Crates Instead

Before writing `unsafe` code, **always search for a safe wrapper crate first**.

#### Common Safe Wrappers

**Windows API:**
```toml
# Bad: Using windows-sys (unsafe bindings)
[dependencies]
windows-sys = "0.52"

# Good: Using winsafe (safe wrapper)
[dependencies]
winsafe = "0.0.19"
```

```rust
// Bad: Unsafe Windows API calls
use windows_sys::Win32::System::Threading::*;

unsafe {
    let handle = CreateThread(
        std::ptr::null(),
        0,
        Some(thread_proc),
        std::ptr::null(),
        0,
        std::ptr::null_mut(),
    );
}

// Good: Safe wrapper
use winsafe::{self as w, prelude::*};

let handle = w::HTHREAD::CreateThread(
    None,
    0,
    thread_proc,
    None,
)?;  // Returns Result, no unsafe needed!
```

**POSIX/Unix APIs:**
```toml
# Instead of libc (unsafe)
[dependencies]
nix = "0.27"  # Safe POSIX wrapper
rustix = "0.38"  # Safe Unix system calls
```

**FFI Bindings:**
```toml
# Look for -sys crates with safe wrappers
libsqlite3-sys = "0.27"  # Unsafe bindings
rusqlite = "0.30"        # Safe wrapper ✅

openssl-sys = "0.9"      # Unsafe bindings  
openssl = "0.10"         # Safe wrapper ✅
```

**Memory Manipulation:**
```toml
# Instead of raw pointer manipulation
[dependencies]
bytemuck = "1.14"  # Safe transmutation
zerocopy = "0.7"   # Safe zero-copy parsing
```

#### Finding Safe Wrappers

1. **Search crates.io** for "{library}-rs" or "safe-{library}"
2. **Check "Reverse Dependencies"** of unsafe -sys crates
3. **Look for high-level crates** that wrap low-level bindings
4. **Ask the community** on Reddit, Discord, or Zulip

### When You Think You Need `unsafe`

**Ask these questions first:**

1. ❓ **Is there a safe wrapper crate?**
   - Check crates.io, lib.rs, and GitHub
   - Ask in Rust community forums

2. ❓ **Can I restructure to avoid it?**
   - Rethink the API design
   - Use safe abstractions like `Vec`, `Box`, `Rc`

3. ❓ **Can I use standard library types?**
   - `Vec` instead of raw arrays
   - `Box` instead of raw pointers
   - `Cell`/`RefCell` instead of interior mutability hacks

4. ❓ **Is this a premature optimization?**
   - Profile first
   - Safe code is usually fast enough

### If You Absolutely Must Use `unsafe`

If no safe alternative exists and you must use `unsafe`:

#### 1. Minimize the Surface Area

```rust
// Bad: Large unsafe block
pub fn process_data(data: &[u8]) -> Vec<u8> {
    unsafe {
        // 100 lines of unsafe code
        // ...
    }
}

// Good: Tiny unsafe block wrapped in safe function
pub fn process_data(data: &[u8]) -> Vec<u8> {
    // 95 lines of safe code
    // ...
    
    // Only the truly unsafe operation
    let value = unsafe {
        *ptr  // 1 line
    };
    
    // More safe code
    // ...
}
```

#### 2. Document EVERY Invariant

```rust
/// Reads a value from a raw pointer.
///
/// # Safety
///
/// The caller must ensure:
/// - `ptr` is non-null
/// - `ptr` is properly aligned for type `T`
/// - `ptr` points to a valid, initialized instance of `T`
/// - `ptr` is dereferenceable (points to allocated memory)
/// - The memory `ptr` points to is not accessed by any other code
///   during this function call
/// - The returned reference's lifetime doesn't outlive the pointee
unsafe fn read_from_ptr<T>(ptr: *const T) -> &T {
    &*ptr
}
```

#### 3. Wrap in Safe Abstractions

```rust
// Bad: Exposing unsafe interface
pub unsafe fn raw_operation(ptr: *mut u8) { }

// Good: Safe public API, unsafe internals
pub fn safe_operation(data: &mut [u8]) -> Result<()> {
    // Validate inputs
    if data.is_empty() {
        return Err(Error::EmptyData);
    }
    
    // Safe wrapper around unsafe operation
    unsafe {
        // SAFETY: We validated data is non-empty and properly aligned
        raw_operation_internal(data.as_mut_ptr())
    }
    
    Ok(())
}

unsafe fn raw_operation_internal(ptr: *mut u8) {
    // Actual unsafe code, not exposed
}
```

#### 4. Test Extensively

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_unsafe_code_edge_cases() {
        // Test empty input
        // Test single element
        // Test boundary conditions
        // Test concurrent access (if applicable)
    }
    
    // Use Miri for undefined behavior detection
    // cargo +nightly miri test
}
```

#### 5. Run Miri

```bash
# Install Miri
rustup +nightly component add miri

# Run tests under Miri to detect undefined behavior
cargo +nightly miri test
```

### Red Flags That Should Make You Reconsider

- ❌ You're not 100% sure the unsafe code is correct
- ❌ You can't articulate all safety invariants
- ❌ The unsafe code interacts with external state
- ❌ You're using unsafe for "performance" without profiling
- ❌ The unsafe block is more than 5 lines
- ❌ You're using transmute without understanding it fully
- ❌ You're doing pointer arithmetic

### Safe Alternatives Checklist

Before writing `unsafe`, verify you've checked:

- [ ] Searched crates.io for safe wrappers
- [ ] Checked if standard library types can solve it
- [ ] Asked in Rust community (Reddit, Discord, forums)
- [ ] Profiled to confirm performance is actually a problem
- [ ] Reviewed similar crates to see their approach
- [ ] Considered restructuring the API to avoid the need

### Example: Choosing Safe Over Unsafe

```rust
// ❌ BAD: Manual unsafe memory management
pub struct Buffer {
    ptr: *mut u8,
    len: usize,
    cap: usize,
}

impl Buffer {
    pub unsafe fn new(capacity: usize) -> Self {
        let layout = Layout::array::<u8>(capacity).unwrap();
        let ptr = alloc(layout);
        // ... unsafe pointer manipulation
    }
}

// ✅ GOOD: Use Vec (safe)
pub struct Buffer {
    data: Vec<u8>,
}

impl Buffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
        }
    }
}

// ✅ ALSO GOOD: Use bytes crate (safe, optimized)
use bytes::BytesMut;

pub struct Buffer {
    data: BytesMut,
}
```

### Summary

- **Default position:** Never use `unsafe`
- **First resort:** Find a safe wrapper crate
- **Second resort:** Restructure to avoid needing it
- **Last resort:** Minimal, well-documented, tested `unsafe`
- **Remember:** Most performance problems are algorithmic, not safety-related

The Rust ecosystem has mature, safe wrappers for most use cases. Use them.

---

## Testing Guidelines

### Test Organization

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_valid_input() {
        let result = process_input("valid").unwrap();
        assert_eq!(result.value(), 42);
    }
    
    #[test]
    fn test_invalid_input() {
        let result = process_input("");
        assert!(result.is_err());
        match result {
            Err(MyLibraryError::ValidationError { field, .. }) => {
                assert_eq!(field, "input");
            },
            _ => panic!("Expected ValidationError"),
        }
    }
    
    #[test]
    #[should_panic(expected = "not implemented")]
    fn test_unimplemented_feature() {
        unimplemented_feature();
    }
}
```

### Property-Based Testing

```toml
[dev-dependencies]
proptest = "1.0"
```

```rust
#[cfg(test)]
mod proptests {
    use proptest::prelude::*;
    
    proptest! {
        #[test]
        fn test_parse_roundtrip(s in "[a-z]{1,100}") {
            let parsed = parse(&s)?;
            let serialized = serialize(&parsed)?;
            prop_assert_eq!(s, serialized);
        }
    }
}
```

### Integration Tests

Place in `tests/` directory:

```rust
// tests/integration_test.rs
use my_library::*;

#[test]
fn test_end_to_end_workflow() {
    let config = Config::builder()
        .host("localhost")
        .port(8080)
        .build()
        .expect("Valid config");
    
    let client = Client::new(config);
    let result = client.process().expect("Process should succeed");
    
    assert!(result.is_valid());
}
```

---

## Documentation Requirements

### Public API Documentation

Every public item must have documentation:

```rust
/// Processes the input data and returns a processed result.
///
/// # Arguments
///
/// * `input` - The input string to process
/// * `options` - Processing options
///
/// # Returns
///
/// Returns a `Result` containing the processed data or an error.
///
/// # Errors
///
/// This function will return an error if:
/// - The input is empty
/// - The input exceeds maximum length
/// - Parsing fails
///
/// # Examples
///
/// ```
/// use my_library::{process, Options};
///
/// let result = process("hello", Options::default())?;
/// assert_eq!(result.value(), "HELLO");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn process(input: &str, options: Options) -> Result<ProcessedData> {
    // Implementation
}
```

### Module Documentation

```rust
//! Configuration management for the application.
//!
//! This module provides types and functions for loading, validating,
//! and managing application configuration.
//!
//! # Examples
//!
//! ```
//! use my_library::config::Config;
//!
//! let config = Config::from_file("config.toml")?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
```

---

## Performance Considerations

### Prefer Borrowing Over Cloning

```rust
// Bad: Unnecessary clone
fn process(data: String) -> String {
    data.to_uppercase()
}

// Good: Borrow when possible
fn process(data: &str) -> String {
    data.to_uppercase()
}
```

### Use `Cow` for Conditional Cloning

```rust
use std::borrow::Cow;

fn process(input: &str) -> Cow<str> {
    if input.contains("special") {
        Cow::Owned(input.replace("special", "SPECIAL"))
    } else {
        Cow::Borrowed(input)
    }
}
```

### Allocations and Capacity

```rust
// Bad: Multiple reallocations
let mut vec = Vec::new();
for i in 0..1000 {
    vec.push(i);
}

// Good: Pre-allocate when size is known
let mut vec = Vec::with_capacity(1000);
for i in 0..1000 {
    vec.push(i);
}
```

### Avoid Unnecessary Copies

```rust
// Use references in iterations
for item in &collection {  // Not: for item in collection
    process(item);
}

// Use drain() when consuming is needed
for item in collection.drain(..) {
    consume(item);
}
```

---

## Documentation and Research Tools

### Using MCP Servers for Crate Documentation

When working with Rust crates, leverage Model Context Protocol (MCP) servers to access up-to-date documentation:

#### GitHub MCP
Use the GitHub MCP server to access the latest source code, examples, and documentation directly from repositories:

```
# Access latest documentation from main/master branch
# Especially critical for rapidly evolving crates
GitHub MCP: Read files from github.com/{org}/{repo}/tree/main
```

**When to use GitHub MCP:**
- Accessing the **latest** API changes not yet in published docs
- Reading example code from the repository
- Checking recent commits and changes
- Accessing documentation from specific branches (especially `main`)
- **REQUIRED for Freya** - see UI Libraries section below

#### Context7 MCP
Use Context7 MCP for general crate documentation queries:

```
# Query published crate documentation
Context7 MCP: Search docs.rs and crate documentation
```

**When to use Context7:**
- Looking up stable, published API documentation
- Searching across multiple crates
- Finding general usage patterns
- Accessing docs.rs content

**Important:** For rapidly evolving crates, GitHub MCP should be preferred over Context7 as it provides access to the most recent changes.

### Research Workflow for New Crates

When integrating a new crate:

1. **Check docs.rs** - Review the published documentation
2. **Use Context7 MCP** - Query for general usage patterns
3. **Use GitHub MCP** - Read the latest examples from the repository
4. **Check CHANGELOG.md** - Understand recent breaking changes
5. **Review issues/discussions** - Understand common pitfalls

---

## UI Libraries

### Choosing a Rust GUI Library

When selecting a UI library for a Rust project, **always consult** the comprehensive survey at:

**https://www.boringcactus.com/2025/04/13/2025-survey-of-rust-gui-libraries.html**

This survey provides up-to-date comparisons of available Rust GUI libraries, their maturity levels, use cases, and trade-offs.

### Common UI Library Options

Based on the survey, here are typical categories:

- **Immediate Mode:** egui, iced
- **Native:** gtk-rs, relm4
- **Web-based:** Tauri, Dioxus
- **Custom Renderers:** Freya, Slint
- **Game Engine UI:** bevy_ui

### Working with Freya

**CRITICAL: Freya is rapidly evolving and the main branch differs significantly from stable releases.**

#### Documentation Source Rules for Freya

1. **ALWAYS use GitHub MCP** to access Freya documentation from the `main` branch
2. **NEVER rely on Context7 or old examples** - they are likely outdated
3. **Read directly from the repository** at `github.com/marc2332/freya`

**Why this matters:**
- Freya's API changes frequently between versions
- The `main` branch contains the latest API design
- Old examples from stable versions may use deprecated patterns
- docs.rs may lag behind current development

#### Accessing Freya Documentation

```
# Correct approach - Use GitHub MCP
GitHub MCP: Read from github.com/marc2332/freya/tree/main/examples
GitHub MCP: Read from github.com/marc2332/freya/tree/main/crates/components
GitHub MCP: Read from github.com/marc2332/freya/tree/main/README.md

# AVOID - These may be outdated
Context7: Query Freya docs  # May reference old API
docs.rs/freya             # May lag behind main branch
```

#### Freya Development Checklist

When working with Freya:

- [ ] Consulted latest examples from `main` branch via GitHub MCP
- [ ] Checked for API changes in recent commits
- [ ] Verified component API matches current `main` branch
- [ ] Reviewed `CHANGELOG.md` for breaking changes
- [ ] Tested against the version specified in `Cargo.toml`

#### Example Freya Workflow

```rust
// Step 1: Use GitHub MCP to read latest example
// GitHub: Read github.com/marc2332/freya/tree/main/examples/counter.rs

// Step 2: Check component documentation
// GitHub: Read github.com/marc2332/freya/tree/main/crates/components/src/button.rs

// Step 3: Implement using latest API patterns
use freya::prelude::*;

fn app() -> Element {
    let mut count = use_signal(|| 0);
    
    rsx! {
        rect {
            height: "100%",
            width: "100%",
            Button {
                onpress: move |_| count += 1,
                label { "Count: {count}" }
            }
        }
    }
}
```

### UI Library Best Practices

Regardless of the chosen UI library:

1. **Pin exact versions** in `Cargo.toml` for stability
2. **Read official examples** from the repository
3. **Check compatibility** with your Rust version
4. **Test on target platforms** early
5. **Monitor breaking changes** in updates
6. **Use GitHub MCP** for latest documentation when available

---

## Dependency Management

### Specify Exact Versions for Applications

```toml
# In applications (binaries), pin exact versions
[dependencies]
serde = "=1.0.195"
tokio = "=1.35.1"
```

### Use Semantic Versioning for Libraries

```toml
# In libraries, use flexible versions
[dependencies]
serde = "1.0"
tokio = { version = "1.35", features = ["full"] }
```

### Enable Only Needed Features

```toml
[dependencies]
tokio = { version = "1.35", features = ["rt-multi-thread", "net", "macros"] }
serde = { version = "1.0", features = ["derive"] }
# Not: features = ["full"]
```

### Review Dependencies Regularly

```bash
# Check for outdated dependencies
cargo outdated

# Audit for security vulnerabilities
cargo audit

# Check for unused dependencies
cargo machete
```

---

## Summary Checklist

When writing or reviewing Rust code, ensure:

- [ ] All Clippy warnings enabled and addressed
- [ ] No `unwrap()`, `expect()`, or `panic!()` in library code
- [ ] All public APIs return `Result<T, E>` with explicit error types
- [ ] Errors defined using `thiserror` for libraries
- [ ] All inputs validated at API boundaries
- [ ] Type system used to enforce invariants
- [ ] Prefer `.into()` for type conversions when type can be inferred
- [ ] Use `impl Into<T>` in function signatures for flexibility
- [ ] No `unsafe` code (or minimal, well-documented if absolutely necessary)
- [ ] Searched for safe wrapper crates before using `unsafe`
- [ ] All public items documented with examples
- [ ] Tests cover success and error cases
- [ ] No unnecessary allocations or clones
- [ ] Dependencies minimized and features specified
- [ ] Code formatted with `rustfmt`
- [ ] No compiler warnings (`cargo build` clean)
- [ ] Used GitHub MCP for latest crate documentation when needed
- [ ] Used Context7 MCP for stable crate documentation queries
- [ ] For Freya: Consulted `main` branch via GitHub MCP (not old examples)
- [ ] For UI libraries: Consulted boringcactus survey for library selection
- [ ] For workspaces: Crates at root level (not in `crates/` folder)
- [ ] For workspaces: Shared dependencies defined in `[workspace.dependencies]`
- [ ] For workspaces: No circular dependencies between crates

---

## Multi-Crate Workspaces

### Overview

Rust workspaces allow you to manage multiple related crates in a single repository. This is essential for larger projects that need to be split into logical components.

### Workspace Structure: Root-Level Crates

**Prefer placing workspace crates at the root level** rather than nested in a `crates/` directory.

#### ✅ Recommended Structure

```
my-project/
├── Cargo.toml          # Workspace root
├── Cargo.lock          # Shared lock file
├── my-core/            # Core library crate
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs
├── my-cli/             # CLI binary crate
│   ├── Cargo.toml
│   └── src/
│       └── main.rs
├── my-server/          # Server binary crate
│   ├── Cargo.toml
│   └── src/
│       └── main.rs
├── my-utils/           # Utilities crate
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs
└── README.md
```

#### ❌ Avoid (Nested in crates/ directory)

```
my-project/
├── Cargo.toml
├── crates/             # Unnecessary nesting
│   ├── my-core/
│   ├── my-cli/
│   ├── my-server/
│   └── my-utils/
└── README.md
```

**Why root-level is better:**
- Shorter import paths in IDEs
- Easier navigation - less nesting
- Simpler to understand project structure
- Matches common Rust ecosystem conventions
- Less typing when navigating directories

### Workspace Root `Cargo.toml`

```toml
[workspace]
members = [
    "my-core",
    "my-cli",
    "my-server",
    "my-utils",
]

# Workspace-wide settings
resolver = "2"

# Shared dependencies across all workspace members
[workspace.dependencies]
tokio = { version = "1.35", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
thiserror = "2.0"
anyhow = "1.0"

# Shared lints for all crates
[workspace.lints.rust]
unsafe_code = "forbid"
missing_docs = "warn"

[workspace.lints.clippy]
all = "deny"
pedantic = "deny"
unwrap_used = "deny"
expect_used = "deny"

# Optional: default crate (e.g., main CLI)
[workspace.package]
edition = "2021"
rust-version = "1.75"
license = "MIT OR Apache-2.0"
repository = "https://github.com/user/my-project"

# Profiles apply to all workspace members
[profile.release]
opt-level = 3
lto = "thin"
strip = true

[profile.dev]
opt-level = 0
debug = true
```

### Member Crate `Cargo.toml`

```toml
# my-core/Cargo.toml
[package]
name = "my-core"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
# Use workspace dependencies
tokio.workspace = true
serde.workspace = true
thiserror.workspace = true

# Crate-specific dependencies
uuid = { version = "1.6", features = ["v4"] }

[dev-dependencies]
proptest = "1.4"

[lints]
workspace = true
```

```toml
# my-cli/Cargo.toml
[package]
name = "my-cli"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true

[[bin]]
name = "my-cli"
path = "src/main.rs"

[dependencies]
# Reference workspace crates
my-core = { path = "../my-core" }
my-utils = { path = "../my-utils" }

# Workspace dependencies
tokio.workspace = true
anyhow.workspace = true

# CLI-specific dependencies
clap = { version = "4.4", features = ["derive"] }

[lints]
workspace = true
```

### Workspace Best Practices

#### 1. Dependency Management

**Use workspace dependencies for shared crates:**

```toml
# Root Cargo.toml
[workspace.dependencies]
serde = { version = "1.0", features = ["derive"] }
tokio = { version = "1.35", default-features = false }

# Member crates can enable additional features
# my-server/Cargo.toml
[dependencies]
tokio = { workspace = true, features = ["rt-multi-thread", "net"] }
```

#### 2. Version Synchronization

**Keep related crates at the same version:**

```toml
# Use workspace.package for shared metadata
[workspace.package]
version = "0.2.0"  # Single source of truth
edition = "2021"
authors = ["Your Name <you@example.com>"]

# Member crates inherit
[package]
name = "my-core"
version.workspace = true
```

#### 3. Inter-Crate Dependencies

**Use path dependencies for workspace members:**

```toml
[dependencies]
my-core = { path = "../my-core", version = "0.2.0" }
```

**Benefits:**
- During development: uses local path
- When published: uses version from crates.io
- Ensures version compatibility

#### 4. Feature Organization

**Define features at workspace level when they affect multiple crates:**

```toml
# Root Cargo.toml
[workspace.dependencies]
tokio = { version = "1.35", default-features = false }

# my-core/Cargo.toml
[features]
default = ["tokio-runtime"]
tokio-runtime = ["tokio/rt-multi-thread"]
full = ["tokio/full"]

[dependencies]
tokio = { workspace = true, optional = true }

# my-cli/Cargo.toml
[dependencies]
my-core = { path = "../my-core", features = ["tokio-runtime"] }
```

#### 5. Testing Across Crates

```bash
# Run all tests in workspace
cargo test

# Run tests for specific crate
cargo test -p my-core

# Run tests with all features
cargo test --all-features

# Run tests in specific crate with specific features
cargo test -p my-server --features "full"
```

### Workspace Commands

```bash
# Build entire workspace
cargo build

# Build specific crate
cargo build -p my-cli

# Check all workspace members
cargo check --workspace

# Update all dependencies
cargo update

# Publish crates (do in order of dependencies)
cd my-core && cargo publish
cd my-utils && cargo publish
cd my-cli && cargo publish

# Clean entire workspace
cargo clean
```

### Organizing Workspace Crates

**Naming conventions:**

```
project-name/
├── project-core/      # Main library
├── project-cli/       # CLI interface
├── project-server/    # Server implementation
├── project-client/    # Client library
├── project-macros/    # Procedural macros
├── project-types/     # Shared types
└── project-test-utils/ # Test utilities
```

**Guidelines:**
- Prefix crates with project name for clarity
- Use descriptive suffixes (-core, -cli, -server, etc.)
- Keep crate names lowercase with hyphens
- Binary crates: `{project}-{purpose}`
- Library crates: `{project}-{component}`

### Splitting Code into Crates

**When to create a new crate:**

✅ **Good reasons:**
- Different compilation units (lib vs bin)
- Independent versioning needed
- Reusable component for other projects
- Reduces compilation time by splitting large crates
- Clear architectural boundary
- Different sets of dependencies

❌ **Bad reasons:**
- "Organization" without clear boundary
- Premature optimization
- Creating unnecessary indirection

**Example split:**

```
# Before: Single crate
my-project/
└── src/
    ├── main.rs
    ├── db/
    ├── api/
    └── models/

# After: Workspace with clear boundaries
my-project/
├── my-db/          # Database layer (reusable)
├── my-models/      # Data models (reusable)
├── my-api/         # API server (binary)
└── my-cli/         # CLI tool (binary)
```

### Common Workspace Patterns

#### Pattern 1: Library + Multiple Binaries

```
project/
├── Cargo.toml      # Workspace root
├── project-core/   # Main library
├── cli/            # CLI binary
├── server/         # Server binary
└── admin/          # Admin tool binary
```

#### Pattern 2: Layered Architecture

```
project/
├── Cargo.toml
├── domain/         # Core business logic
├── infrastructure/ # Database, external APIs
├── application/    # Use cases, services
└── presentation/   # API, CLI, UI
```

#### Pattern 3: Plugin Architecture

```
project/
├── Cargo.toml
├── core/           # Core library
├── plugin-api/     # Plugin interface
├── plugin-a/       # Plugin implementation
├── plugin-b/       # Plugin implementation
└── cli/            # CLI that loads plugins
```

### Workspace Gotchas and Solutions

#### Problem: Dependency Version Conflicts

```toml
# Bad: Different versions in different crates
# my-core/Cargo.toml
[dependencies]
serde = "1.0.195"

# my-cli/Cargo.toml
[dependencies]
serde = "1.0.180"  # Conflict!

# Good: Use workspace dependencies
# Root Cargo.toml
[workspace.dependencies]
serde = "1.0.195"

# All member crates
[dependencies]
serde.workspace = true
```

#### Problem: Circular Dependencies

```toml
# ❌ BAD: Circular dependency
# my-core/Cargo.toml
[dependencies]
my-utils = { path = "../my-utils" }

# my-utils/Cargo.toml
[dependencies]
my-core = { path = "../my-core" }  # Circular!

# ✅ GOOD: Create third crate for shared types
# my-types/Cargo.toml (no dependencies on other workspace crates)

# my-core/Cargo.toml
[dependencies]
my-types = { path = "../my-types" }

# my-utils/Cargo.toml
[dependencies]
my-types = { path = "../my-types" }
```

#### Problem: Slow Compile Times

**Solution: Split into smaller crates and use features:**

```toml
# Disable default features to reduce compile time
[dependencies]
tokio = { workspace = true, default-features = false, features = ["rt"] }

# Use incremental compilation
[profile.dev]
incremental = true
```

### Publishing Workspace Crates

```bash
# 1. Check what will be published
cargo package -p my-core --list

# 2. Dry run
cargo publish -p my-core --dry-run

# 3. Publish in dependency order
cargo publish -p my-types
cargo publish -p my-core
cargo publish -p my-utils
cargo publish -p my-cli

# Or use cargo-release for automation
cargo install cargo-release
cargo release --workspace
```

### Workspace Checklist

When setting up a workspace:

- [ ] Use root-level crate directories (not `crates/` folder)
- [ ] Configure `[workspace.dependencies]` for shared deps
- [ ] Set `[workspace.lints]` for consistent code quality
- [ ] Use `[workspace.package]` for shared metadata
- [ ] Define clear boundaries between crates
- [ ] Avoid circular dependencies
- [ ] Use path dependencies with versions
- [ ] Test entire workspace with `cargo test --workspace`
- [ ] Document inter-crate relationships
- [ ] Consider compilation time when splitting

---

## Additional Resources

- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [Rust Design Patterns](https://rust-unofficial.github.io/patterns/)
- [The Rustonomicon](https://doc.rust-lang.org/nomicon/) - For unsafe code
- [Effective Rust](https://www.lurklurk.org/effective-rust/)
- [Clippy Lints](https://rust-lang.github.io/rust-clippy/master/)

---

*Last Updated: December 2025*
