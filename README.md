# Rust Mock Challenge: Testing `FnOnce` with `mockall`

This repository contains a minimal reproduction of a tricky scenario involving mocking a function that accepts a `FnOnce` closure. Due to Rust’s ownership model, `FnOnce` closures cannot be trivially cloned or invoked through a shared reference. The goal is to test and validate that the closure is actually passed and can later be invoked or inspected.

## Table of Contents

- [Rust Mock Challenge: Testing `FnOnce` with `mockall`](#rust-mock-challenge-testing-fnonce-with-mockall)
  - [Table of Contents](#table-of-contents)
  - [Context](#context)
  - [Problem](#problem)
  - [Constraints](#constraints)
  - [Solution Overview](#solution-overview)
  - [Detailed Steps](#detailed-steps)
    - [Define Your Trait and Structs](#define-your-trait-and-structs)
  - [Why Not Change `FnOnce` to `Fn`?](#why-not-change-fnonce-to-fn)
  - [FAQ](#faq)
  - [References](#references)

---

## Context

We have an asynchronous method:

```rust
async fn bar<F>(&self, update_fn: F)
where
    F: FnOnce(Zed) -> Zed + Send + 'static,
{
    update_fn(Zed {});
}
```

...which we want to mock using mockall. The method takes a `FnOnce` closure. Unlike `Fn` or FnMut, a `FnOnce` closure:

1. Consumes the environment it captures.
2. Can only be called once.

When mockall generates a method signature like this:

```rust
trait Foo {
    async fn bar<F>(&self, update_fn: F)
    where
        F: FnOnce(Zed) -> Zed + Send + 'static;
}
```
...and we write an expectation with something like:
```rust
.expect_bar()
.withf(|update_fn: &Box<dyn FnOnce(Zed) -> Zed + Send + 'static>| {
    // ...
})
```
...it passes `update_fn` as a reference (`&Box<dyn FnOnce(...)>`). This creates a conflict:

 - We cannot move or invoke a `FnOnce` closure through a shared reference alone (that would violate Rust’s ownership rules).
 - We also cannot trivially Clone the contents of a `Box<dyn FnOnce(...)>` because `FnOnce` is inherently non-Clone.

## Problem

We need to:

1. Capture the `FnOnce` closure inside the `withf` predicate to prove it was passed.
2. Possibly invoke (or at least store for later) the closure to validate its effects.
3. Avoid changing the trait signature from `FnOnce` to `Fn` (which would be simpler but is not allowed by the challenge).

When we try naive solutions, we get compile-time errors:

E0507: cannot move out of `*update_fn` which is behind a shared reference.
E0308: mismatched types when trying to clone or store a `FnOnce` closure.

## Constraints

1. Maintain the `FnOnce` type to ensure it can be consumed once.
2. Must respect Rust’s ownership model (no illegal moves or clones).
3. Remain asynchronous-friendly (the bar function is async).
4. Use mockall for mocking.

## Solution Overview

Key Insight: You cannot clone the contents of `Box<dyn FnOnce(...)>` nor directly move it from a shared reference. Instead:

Capture the closure reference in your mock’s `withf` predicate.
Store it in a thread-safe structure (e.g., `Arc<Mutex<Option<Box<dyn FnOnce(...)>>>>`).
Transfer Ownership after the mock call finishes, so you can invoke it exactly once.
This means:

 - In `withf`, you will not attempt to invoke the closure. Instead, you simply store it as-is (without dereferencing it) so you can move it later.
 - After the mock call occurs, retrieve the stored closure from the Arc<Mutex> and invoke it once in your test logic.

## Detailed Steps

### Define Your Trait and Structs

1. Define Your Trait and Structs:
```rust
#[automock]
trait Foo {
    async fn bar<F>(&self, update_fn: F)
    where
        F: FnOnce(Zed) -> Zed + Send + 'static;
}

struct FooImpl;
impl Foo for FooImpl {
    async fn bar<F>(&self, update_fn: F)
    where
        F: FnOnce(Zed) -> Zed + Send + 'static,
    {
        update_fn(Zed {});
    }
}

struct Zed;
struct BazImpl;

impl BazImpl {
    async fn baz<F: Foo>(self, f: F) {
        f.bar(|zed| zed).await;
    }
}
```
2. Set Up the Thread-Safe Storage
```rust
let captured_update_fn: Arc<Mutex<Option<Box<dyn FnOnce(Zed) -> Zed + Send>>>> =
    Arc::new(Mutex::new(None));
let captured_update_fn_clone = Arc::clone(&captured_update_fn);
```
3. Mock and Expect the `FnOnce`
```rust
let mut mock_foo = MockFoo::new();
mock_foo
    .expect_bar()
    .times(1)
    .withf(move |update_fn: &Box<dyn FnOnce(Zed) -> Zed + Send>| {
        // Step 1: Access the Arc<Mutex> to store the closure pointer
        let mut captured = captured_update_fn_clone.lock().unwrap();

        // Step 2: Store the reference to the box. 
        // We do NOT deref or invoke it here.
        // Instead, we put it into the Arc<Mutex> for later usage.
        *captured = Some(update_fn.to_owned()); 

        // Return true so mockall knows the predicate matched
        true
    })
    .return_const(());

```
*Note*: `update_fn.to_owned()` does not clone the closure contents (which is impossible for `FnOnce`), it only copies the Box pointer. This is allowed if the trait object is Sized enough at runtime. If you still get a mismatch error, see the next subsection in FAQ.

4. Invoke the Mock
```rust
let baz = BazImpl {};
baz.baz(mock_foo).await;
```
5. Retrieve and Invoke the Captured Closure
```rust
let captured_closure = captured_update_fn.lock().unwrap().take();
assert!(captured_closure.is_some(), "No closure was captured!");

// Actually call the FnOnce now
if let Some(update_fn) = captured_closure {
    let zed = Zed;
    let result = update_fn(zed);
    // ... do any validations if needed
}
```

## Why Not Change `FnOnce` to `Fn`?

Altering the trait to `Fn` or FnMut would make this simpler because those closures can be called multiple times or do not require full ownership. However, if your real-world scenario requires `FnOnce` (due to one-time consumption or resource management), switching to `Fn`/`FnMut` breaks the contract you’re testing. The challenge explicitly forbids using a different closure trait, so we have to respect `FnOnce`.

## FAQ

1. Why do I see E0507 (move out of a shared reference)?
This error occurs when you try to directly invoke or move the `FnOnce` closure from `&Box<dyn FnOnce>`. Rust doesn’t allow moving content out of a reference (especially if `FnOnce` needs full ownership).
2. I get mismatched types saying it expects `Box<dyn FnOnce>` but found `&Box<dyn FnOnce>`?
This means you’re assigning a reference to a slot that expects ownership. The snippet above uses update_fn.to_owned() to store a pointer-level clone. If the compile error persists, double-check that the signature inside `withf` is `move |update_fn: &Box<dyn FnOnce(...)>| { ... }`.
3. Can I directly invoke the closure inside `withf`?
Usually not: `withf` provides a predicate that runs during matching, not the actual “business logic” place. Also, you’d lose the ability to verify the closure in your test if you call it before the function finishes.
4. What if `update_fn.to_owned()` still fails because `Box<dyn FnOnce>` does not implement Clone?
In practice, some setups do allow copying a pointer to the trait object. If your environment or Rust version complains, you may need a different strategy, such as capturing the Box further up the call chain or storing the closure in your own code before passing it to mockall. The gist is that you cannot truly “clone” a `FnOnce`; you can only store its pointer for deferred use.

## References

(Rust Reference on `FnOnce`)[https://doc.rust-lang.org/reference/types/closure.html]
(The mockall crate on Docs.rs)[https://docs.rs/mockall/latest/mockall/]
Common Rust Ownership Errors (E0507, E0308)

Thank you for checking out this challenge! By following the solution steps above, you should be able to mock and test an async function that takes a `FnOnce` closure without violating Rust’s ownership rules.