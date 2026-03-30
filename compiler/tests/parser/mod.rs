// ===============================================================================
// QUANTALANG PARSER - TESTS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Comprehensive tests for the QuantaLang parser.

use quantalang::lexer::{Lexer, SourceFile};
use quantalang::parser::{Parser, parse_source, ParseResult};
use quantalang::ast::*;

/// Helper to parse source code and return the AST.
fn parse(source: &str) -> ParseResult<Module> {
    parse_source("test.quanta", source)
}

/// Helper to check if parsing succeeds.
fn parses(source: &str) -> bool {
    parse(source).is_ok()
}

/// Helper to check parsing fails.
fn fails(source: &str) -> bool {
    parse(source).is_err()
}

// =============================================================================
// EXPRESSION TESTS
// =============================================================================

mod expressions {
    use super::*;

    #[test]
    fn test_literals() {
        assert!(parses("fn main() { 42; }"));
        assert!(parses("fn main() { 3.14; }"));
        assert!(parses("fn main() { \"hello\"; }"));
        assert!(parses("fn main() { 'c'; }"));
        assert!(parses("fn main() { true; }"));
        assert!(parses("fn main() { false; }"));
        assert!(parses("fn main() { b'x'; }"));
        assert!(parses("fn main() { b\"bytes\"; }"));
    }

    #[test]
    fn test_integer_bases() {
        assert!(parses("fn main() { 0x1a; }"));      // hex
        assert!(parses("fn main() { 0o17; }"));      // octal
        assert!(parses("fn main() { 0b1010; }"));    // binary
        assert!(parses("fn main() { 1_000_000; }")); // with underscores
    }

    #[test]
    fn test_suffixed_literals() {
        assert!(parses("fn main() { 42i32; }"));
        assert!(parses("fn main() { 42u64; }"));
        assert!(parses("fn main() { 3.14f64; }"));
    }

    #[test]
    fn test_identifiers() {
        assert!(parses("fn main() { x; }"));
        assert!(parses("fn main() { foo_bar; }"));
        assert!(parses("fn main() { _unused; }"));
        assert!(parses("fn main() { r#type; }"));  // raw identifier
    }

    #[test]
    fn test_paths() {
        assert!(parses("fn main() { std::io::Read; }"));
        assert!(parses("fn main() { ::std::io::Read; }"));
        assert!(parses("fn main() { crate::module::item; }"));
        assert!(parses("fn main() { super::item; }"));
        assert!(parses("fn main() { self::item; }"));
    }

    #[test]
    fn test_unary_operators() {
        assert!(parses("fn main() { -x; }"));
        assert!(parses("fn main() { !b; }"));
        assert!(parses("fn main() { *ptr; }"));
        assert!(parses("fn main() { &x; }"));
        assert!(parses("fn main() { &mut x; }"));
        assert!(parses("fn main() { --x; }"));  // double negation
        assert!(parses("fn main() { &&x; }"));  // double reference
    }

    #[test]
    fn test_binary_operators() {
        assert!(parses("fn main() { a + b; }"));
        assert!(parses("fn main() { a - b; }"));
        assert!(parses("fn main() { a * b; }"));
        assert!(parses("fn main() { a / b; }"));
        assert!(parses("fn main() { a % b; }"));
        assert!(parses("fn main() { a & b; }"));
        assert!(parses("fn main() { a | b; }"));
        assert!(parses("fn main() { a ^ b; }"));
        assert!(parses("fn main() { a << b; }"));
        assert!(parses("fn main() { a >> b; }"));
        assert!(parses("fn main() { a && b; }"));
        assert!(parses("fn main() { a || b; }"));
        assert!(parses("fn main() { a == b; }"));
        assert!(parses("fn main() { a != b; }"));
        assert!(parses("fn main() { a < b; }"));
        assert!(parses("fn main() { a <= b; }"));
        assert!(parses("fn main() { a > b; }"));
        assert!(parses("fn main() { a >= b; }"));
    }

    #[test]
    fn test_operator_precedence() {
        assert!(parses("fn main() { a + b * c; }"));    // * binds tighter
        assert!(parses("fn main() { a || b && c; }"));  // && binds tighter
        assert!(parses("fn main() { a == b && c < d; }"));
    }

    #[test]
    fn test_assignment() {
        assert!(parses("fn main() { x = 1; }"));
        assert!(parses("fn main() { x += 1; }"));
        assert!(parses("fn main() { x -= 1; }"));
        assert!(parses("fn main() { x *= 2; }"));
        assert!(parses("fn main() { x /= 2; }"));
        assert!(parses("fn main() { x %= 2; }"));
        assert!(parses("fn main() { x &= mask; }"));
        assert!(parses("fn main() { x |= mask; }"));
        assert!(parses("fn main() { x ^= mask; }"));
        assert!(parses("fn main() { x <<= 1; }"));
        assert!(parses("fn main() { x >>= 1; }"));
    }

    #[test]
    fn test_function_calls() {
        assert!(parses("fn main() { f(); }"));
        assert!(parses("fn main() { f(1); }"));
        assert!(parses("fn main() { f(1, 2, 3); }"));
        assert!(parses("fn main() { f(g(1)); }"));  // nested
    }

    #[test]
    fn test_method_calls() {
        assert!(parses("fn main() { x.foo(); }"));
        assert!(parses("fn main() { x.foo(1, 2); }"));
        assert!(parses("fn main() { x.foo().bar(); }"));  // chained
    }

    #[test]
    fn test_field_access() {
        assert!(parses("fn main() { x.field; }"));
        assert!(parses("fn main() { x.a.b.c; }"));
        assert!(parses("fn main() { tuple.0; }"));  // tuple field
        assert!(parses("fn main() { tuple.1; }"));
    }

    #[test]
    fn test_index_access() {
        assert!(parses("fn main() { arr[0]; }"));
        assert!(parses("fn main() { arr[i + 1]; }"));
        assert!(parses("fn main() { matrix[i][j]; }"));
    }

    #[test]
    fn test_type_cast() {
        assert!(parses("fn main() { x as i32; }"));
        assert!(parses("fn main() { x as *const u8; }"));
    }

    #[test]
    fn test_ranges() {
        assert!(parses("fn main() { ..; }"));
        assert!(parses("fn main() { 0..; }"));
        assert!(parses("fn main() { ..10; }"));
        assert!(parses("fn main() { 0..10; }"));
        assert!(parses("fn main() { 0..=10; }"));  // inclusive
    }

    #[test]
    fn test_try_operator() {
        assert!(parses("fn main() { x?; }"));
        assert!(parses("fn main() { foo()?.bar()?; }"));
    }

    #[test]
    fn test_await() {
        assert!(parses("fn main() { x.await; }"));
        assert!(parses("fn main() { foo().await; }"));
    }

    #[test]
    fn test_arrays() {
        assert!(parses("fn main() { []; }"));
        assert!(parses("fn main() { [1]; }"));
        assert!(parses("fn main() { [1, 2, 3]; }"));
        assert!(parses("fn main() { [0; 10]; }"));  // repeat
    }

    #[test]
    fn test_tuples() {
        assert!(parses("fn main() { (); }"));       // unit
        assert!(parses("fn main() { (1,); }"));     // single element
        assert!(parses("fn main() { (1, 2); }"));
        assert!(parses("fn main() { (1, 2, 3); }"));
    }

    #[test]
    fn test_struct_literals() {
        assert!(parses("fn main() { Point { x: 1, y: 2 }; }"));
        assert!(parses("fn main() { Point { x, y }; }"));  // shorthand
        assert!(parses("fn main() { Point { x: 1, ..base }; }"));  // ..rest
    }

    #[test]
    fn test_closures() {
        assert!(parses("fn main() { || 1; }"));
        assert!(parses("fn main() { |x| x + 1; }"));
        assert!(parses("fn main() { |x, y| x + y; }"));
        assert!(parses("fn main() { |x: i32| x; }"));
        assert!(parses("fn main() { |x| -> i32 { x }; }"));
        assert!(parses("fn main() { move |x| x; }"));
        assert!(parses("fn main() { async |x| x; }"));
        assert!(parses("fn main() { async move |x| x; }"));
    }

    #[test]
    fn test_if_expression() {
        assert!(parses("fn main() { if true { 1 } }"));
        assert!(parses("fn main() { if true { 1 } else { 2 } }"));
        assert!(parses("fn main() { if a { 1 } else if b { 2 } else { 3 } }"));
    }

    #[test]
    fn test_match_expression() {
        assert!(parses("fn main() { match x { _ => 1 } }"));
        assert!(parses("fn main() { match x { 0 => a, 1 => b, _ => c } }"));
        assert!(parses("fn main() { match x { Some(v) => v, None => 0 } }"));
        assert!(parses("fn main() { match x { n if n > 0 => n, _ => 0 } }"));  // guard
    }

    #[test]
    fn test_loops() {
        assert!(parses("fn main() { loop { } }"));
        assert!(parses("fn main() { while true { } }"));
        assert!(parses("fn main() { while let Some(x) = iter.next() { } }"));
        assert!(parses("fn main() { for x in iter { } }"));
    }

    #[test]
    fn test_jumps() {
        assert!(parses("fn main() { return; }"));
        assert!(parses("fn main() { return 42; }"));
        assert!(parses("fn main() { loop { break; } }"));
        assert!(parses("fn main() { loop { break 42; } }"));
        assert!(parses("fn main() { loop { continue; } }"));
    }

    #[test]
    fn test_blocks() {
        assert!(parses("fn main() { { } }"));
        assert!(parses("fn main() { { 1 } }"));
        assert!(parses("fn main() { { let x = 1; x } }"));
        assert!(parses("fn main() { unsafe { } }"));
        assert!(parses("fn main() { async { } }"));
        assert!(parses("fn main() { async move { } }"));
    }

    #[test]
    fn test_parentheses() {
        assert!(parses("fn main() { (1); }"));
        assert!(parses("fn main() { (a + b) * c; }"));
    }
}

// =============================================================================
// TYPE TESTS
// =============================================================================

mod types {
    use super::*;

    #[test]
    fn test_primitive_types() {
        assert!(parses("fn main() -> i32 { 0 }"));
        assert!(parses("fn main() -> u64 { 0 }"));
        assert!(parses("fn main() -> f64 { 0.0 }"));
        assert!(parses("fn main() -> bool { true }"));
        assert!(parses("fn main() -> char { 'a' }"));
        assert!(parses("fn main() -> str { todo!() }"));
    }

    #[test]
    fn test_reference_types() {
        assert!(parses("fn main(x: &i32) { }"));
        assert!(parses("fn main(x: &mut i32) { }"));
        assert!(parses("fn main(x: &'a i32) { }"));
        assert!(parses("fn main(x: &'a mut i32) { }"));
    }

    #[test]
    fn test_pointer_types() {
        assert!(parses("fn main(x: *const i32) { }"));
        assert!(parses("fn main(x: *mut i32) { }"));
    }

    #[test]
    fn test_array_slice_types() {
        assert!(parses("fn main(x: [i32; 10]) { }"));
        assert!(parses("fn main(x: [i32; N]) { }"));
        assert!(parses("fn main(x: &[i32]) { }"));
    }

    #[test]
    fn test_tuple_types() {
        assert!(parses("fn main() -> () { }"));
        assert!(parses("fn main() -> (i32,) { (0,) }"));
        assert!(parses("fn main() -> (i32, i32) { (0, 0) }"));
    }

    #[test]
    fn test_path_types() {
        assert!(parses("fn main() -> Vec<i32> { Vec::new() }"));
        assert!(parses("fn main() -> std::io::Result<()> { Ok(()) }"));
    }

    #[test]
    fn test_function_types() {
        assert!(parses("fn main(f: fn()) { }"));
        assert!(parses("fn main(f: fn(i32) -> i32) { }"));
        assert!(parses("fn main(f: fn(i32, i32) -> i32) { }"));
        assert!(parses("fn main(f: unsafe fn()) { }"));
        assert!(parses("fn main(f: extern \"C\" fn()) { }"));
    }

    #[test]
    fn test_impl_trait() {
        assert!(parses("fn main() -> impl Clone { 42 }"));
        assert!(parses("fn main() -> impl Clone + Send { 42 }"));
    }

    #[test]
    fn test_dyn_trait() {
        assert!(parses("fn main(x: &dyn Clone) { }"));
        assert!(parses("fn main(x: Box<dyn Clone + Send>) { }"));
    }

    #[test]
    fn test_never_type() {
        assert!(parses("fn main() -> ! { panic!() }"));
    }

    #[test]
    fn test_infer_type() {
        assert!(parses("fn main() { let x: _ = 1; }"));
    }

    #[test]
    fn test_self_type() {
        assert!(parses("impl Foo { fn new() -> Self { Self { } } }"));
    }
}

// =============================================================================
// PATTERN TESTS
// =============================================================================

mod patterns {
    use super::*;

    #[test]
    fn test_wildcard() {
        assert!(parses("fn main() { let _ = 1; }"));
    }

    #[test]
    fn test_identifier_patterns() {
        assert!(parses("fn main() { let x = 1; }"));
        assert!(parses("fn main() { let mut x = 1; }"));
        assert!(parses("fn main() { let ref x = 1; }"));
        assert!(parses("fn main() { let ref mut x = 1; }"));
    }

    #[test]
    fn test_binding_patterns() {
        assert!(parses("fn main() { match x { a @ 1..=10 => {} _ => {} } }"));
    }

    #[test]
    fn test_literal_patterns() {
        assert!(parses("fn main() { match x { 1 => {} _ => {} } }"));
        assert!(parses("fn main() { match x { \"hello\" => {} _ => {} } }"));
        assert!(parses("fn main() { match x { true => {} false => {} } }"));
    }

    #[test]
    fn test_tuple_patterns() {
        assert!(parses("fn main() { let (a, b) = (1, 2); }"));
        assert!(parses("fn main() { let (a, b, c) = (1, 2, 3); }"));
        assert!(parses("fn main() { let (a,) = (1,); }"));
    }

    #[test]
    fn test_struct_patterns() {
        assert!(parses("fn main() { let Point { x, y } = p; }"));
        assert!(parses("fn main() { let Point { x: a, y: b } = p; }"));
        assert!(parses("fn main() { let Point { x, .. } = p; }"));
    }

    #[test]
    fn test_tuple_struct_patterns() {
        assert!(parses("fn main() { let Some(x) = opt; }"));
        assert!(parses("fn main() { let Pair(a, b) = pair; }"));
    }

    #[test]
    fn test_slice_patterns() {
        assert!(parses("fn main() { let [a, b, c] = arr; }"));
        assert!(parses("fn main() { let [first, ..] = arr; }"));
        assert!(parses("fn main() { let [first, .., last] = arr; }"));
    }

    #[test]
    fn test_reference_patterns() {
        assert!(parses("fn main() { let &x = &1; }"));
        assert!(parses("fn main() { let &mut x = &mut 1; }"));
    }

    #[test]
    fn test_or_patterns() {
        assert!(parses("fn main() { match x { 1 | 2 | 3 => {} _ => {} } }"));
    }

    #[test]
    fn test_range_patterns() {
        assert!(parses("fn main() { match x { 0..10 => {} _ => {} } }"));
        assert!(parses("fn main() { match x { 0..=10 => {} _ => {} } }"));
    }
}

// =============================================================================
// STATEMENT TESTS
// =============================================================================

mod statements {
    use super::*;

    #[test]
    fn test_let_statements() {
        assert!(parses("fn main() { let x = 1; }"));
        assert!(parses("fn main() { let x: i32 = 1; }"));
        assert!(parses("fn main() { let x: i32; }"));  // uninitialized
        assert!(parses("fn main() { let (a, b) = (1, 2); }"));
    }

    #[test]
    fn test_let_else() {
        assert!(parses("fn main() { let Some(x) = opt else { return; }; }"));
    }

    #[test]
    fn test_expression_statements() {
        assert!(parses("fn main() { x; }"));
        assert!(parses("fn main() { foo(); }"));
        assert!(parses("fn main() { 1 + 2; }"));
    }

    #[test]
    fn test_block_expressions_no_semi() {
        assert!(parses("fn main() { if true { } }"));
        assert!(parses("fn main() { match x { _ => {} } }"));
        assert!(parses("fn main() { loop { break; } }"));
    }

    #[test]
    fn test_item_statements() {
        assert!(parses("fn main() { fn inner() {} }"));
        assert!(parses("fn main() { struct Local { x: i32 } }"));
    }

    #[test]
    fn test_empty_statement() {
        assert!(parses("fn main() { ; }"));
        assert!(parses("fn main() { ;; }"));
    }
}

// =============================================================================
// ITEM TESTS
// =============================================================================

mod items {
    use super::*;

    #[test]
    fn test_function() {
        assert!(parses("fn foo() { }"));
        assert!(parses("fn foo(x: i32) { }"));
        assert!(parses("fn foo(x: i32, y: i32) { }"));
        assert!(parses("fn foo() -> i32 { 0 }"));
        assert!(parses("fn foo<T>(x: T) { }"));
        assert!(parses("fn foo<T: Clone>(x: T) { }"));
        assert!(parses("fn foo<T>(x: T) where T: Clone { }"));
    }

    #[test]
    fn test_function_modifiers() {
        assert!(parses("pub fn foo() { }"));
        assert!(parses("pub(crate) fn foo() { }"));
        assert!(parses("async fn foo() { }"));
        assert!(parses("const fn foo() { 0 }"));
        assert!(parses("unsafe fn foo() { }"));
    }

    #[test]
    fn test_struct() {
        assert!(parses("struct Unit;"));
        assert!(parses("struct Tuple(i32);"));
        assert!(parses("struct Tuple(i32, i32);"));
        assert!(parses("struct Named { x: i32 }"));
        assert!(parses("struct Named { x: i32, y: i32 }"));
        assert!(parses("struct Generic<T> { value: T }"));
        assert!(parses("pub struct Pub { pub x: i32 }"));
    }

    #[test]
    fn test_enum() {
        assert!(parses("enum Empty { }"));
        assert!(parses("enum Unit { A, B, C }"));
        assert!(parses("enum Tuple { A(i32), B(i32, i32) }"));
        assert!(parses("enum Named { A { x: i32 }, B { y: i32 } }"));
        assert!(parses("enum Mixed { A, B(i32), C { x: i32 } }"));
        assert!(parses("enum Discriminant { A = 0, B = 1 }"));
        assert!(parses("enum Generic<T> { Some(T), None }"));
    }

    #[test]
    fn test_trait() {
        assert!(parses("trait Foo { }"));
        assert!(parses("trait Foo { fn bar(); }"));
        assert!(parses("trait Foo { fn bar() { } }"));  // default impl
        assert!(parses("trait Foo { type Item; }"));
        assert!(parses("trait Foo { type Item: Clone; }"));
        assert!(parses("trait Foo { type Item = i32; }"));  // default
        assert!(parses("trait Foo { const N: usize; }"));
        assert!(parses("trait Foo: Clone { }"));  // supertrait
        assert!(parses("trait Foo: Clone + Send { }"));
        assert!(parses("trait Foo<T> { }"));
        assert!(parses("unsafe trait Foo { }"));
    }

    #[test]
    fn test_impl() {
        assert!(parses("impl Foo { }"));
        assert!(parses("impl Foo { fn bar() { } }"));
        assert!(parses("impl Foo { fn bar(&self) { } }"));
        assert!(parses("impl<T> Foo<T> { }"));
        assert!(parses("impl Trait for Foo { }"));
        assert!(parses("impl<T> Trait for Foo<T> { }"));
        assert!(parses("impl Trait for Foo { type Item = i32; }"));
        assert!(parses("impl Trait for Foo { const N: usize = 0; }"));
        assert!(parses("unsafe impl Trait for Foo { }"));
    }

    #[test]
    fn test_type_alias() {
        assert!(parses("type Foo = i32;"));
        assert!(parses("type Foo<T> = Vec<T>;"));
        assert!(parses("type Foo<T: Clone> = Vec<T>;"));
    }

    #[test]
    fn test_const_static() {
        assert!(parses("const N: i32 = 0;"));
        assert!(parses("const N: i32 = 1 + 2;"));
        assert!(parses("static S: i32 = 0;"));
        assert!(parses("static mut S: i32 = 0;"));
        assert!(parses("pub const N: i32 = 0;"));
    }

    #[test]
    fn test_module() {
        assert!(parses("mod foo;"));
        assert!(parses("mod foo { }"));
        assert!(parses("mod foo { fn bar() { } }"));
        assert!(parses("mod foo { mod bar { } }"));  // nested
        assert!(parses("pub mod foo { }"));
    }

    #[test]
    fn test_use() {
        assert!(parses("use std::io;"));
        assert!(parses("use std::io::Read;"));
        assert!(parses("use std::io::*;"));
        assert!(parses("use std::io::{Read, Write};"));
        assert!(parses("use std::io::Read as IoRead;"));
        assert!(parses("use crate::module::item;"));
        assert!(parses("use super::item;"));
        assert!(parses("use self::item;"));
        assert!(parses("pub use std::io::Read;"));
    }

    #[test]
    fn test_extern() {
        assert!(parses("extern crate foo;"));
        assert!(parses("extern crate foo as bar;"));
        assert!(parses("extern { }"));
        assert!(parses("extern \"C\" { }"));
        assert!(parses("extern \"C\" { fn foo(); }"));
        assert!(parses("extern \"C\" { static S: i32; }"));
        assert!(parses("unsafe extern \"C\" { fn foo(); }"));
    }
}

// =============================================================================
// ATTRIBUTE TESTS
// =============================================================================

mod attributes {
    use super::*;

    #[test]
    fn test_outer_attributes() {
        assert!(parses("#[test] fn foo() { }"));
        assert!(parses("#[derive(Debug)] struct Foo { }"));
        assert!(parses("#[derive(Debug, Clone)] struct Foo { }"));
        assert!(parses("#[cfg(test)] fn foo() { }"));
        assert!(parses("#[allow(unused)] fn foo() { }"));
    }

    #[test]
    fn test_inner_attributes() {
        assert!(parses("#![no_std] fn main() { }"));
        assert!(parses("#![allow(unused)] fn main() { }"));
    }

    #[test]
    fn test_attribute_with_value() {
        assert!(parses("#[path = \"foo.rs\"] mod foo;"));
        assert!(parses("#[doc = \"Documentation\"] fn foo() { }"));
    }

    #[test]
    fn test_multiple_attributes() {
        assert!(parses("#[test] #[should_panic] fn foo() { }"));
        assert!(parses("#[derive(Debug)] #[derive(Clone)] struct Foo { }"));
    }
}

// =============================================================================
// GENERICS TESTS
// =============================================================================

mod generics {
    use super::*;

    #[test]
    fn test_type_parameters() {
        assert!(parses("fn foo<T>() { }"));
        assert!(parses("fn foo<T, U>() { }"));
        assert!(parses("fn foo<T: Clone>() { }"));
        assert!(parses("fn foo<T: Clone + Send>() { }"));
        assert!(parses("fn foo<T: ?Sized>() { }"));
    }

    #[test]
    fn test_lifetime_parameters() {
        assert!(parses("fn foo<'a>() { }"));
        assert!(parses("fn foo<'a, 'b>() { }"));
        assert!(parses("fn foo<'a: 'b>() { }"));
        assert!(parses("fn foo<'a, T>() { }"));
    }

    #[test]
    fn test_const_parameters() {
        assert!(parses("fn foo<const N: usize>() { }"));
        assert!(parses("struct Arr<T, const N: usize> { data: [T; N] }"));
    }

    #[test]
    fn test_where_clauses() {
        assert!(parses("fn foo<T>() where T: Clone { }"));
        assert!(parses("fn foo<T>() where T: Clone + Send { }"));
        assert!(parses("fn foo<T, U>() where T: Clone, U: Send { }"));
    }

    #[test]
    fn test_default_type_parameters() {
        assert!(parses("struct Foo<T = i32> { x: T }"));
    }
}

// =============================================================================
// VISIBILITY TESTS
// =============================================================================

mod visibility {
    use super::*;

    #[test]
    fn test_visibility_modifiers() {
        assert!(parses("fn foo() { }"));           // private
        assert!(parses("pub fn foo() { }"));       // public
        assert!(parses("pub(crate) fn foo() { }"));
        assert!(parses("pub(super) fn foo() { }"));
        assert!(parses("pub(self) fn foo() { }"));
        assert!(parses("pub(in crate::module) fn foo() { }"));
    }
}

// =============================================================================
// ERROR TESTS
// =============================================================================

mod errors {
    use super::*;

    #[test]
    fn test_missing_semicolon() {
        // Expression statements need semicolons
        assert!(fails("fn main() { let x = 1 }"));  // missing semicolon
    }

    #[test]
    fn test_invalid_assignment_target() {
        // Can't assign to literals
        assert!(fails("fn main() { 1 = x; }"));
    }

    #[test]
    fn test_unclosed_delimiter() {
        assert!(fails("fn main() { (1 + 2 }"));
        assert!(fails("fn main() { [1, 2 }"));
        assert!(fails("fn main( { }"));
    }

    #[test]
    fn test_unexpected_token() {
        assert!(fails("fn main() { + }"));
        assert!(fails("fn main() { let = 1; }"));
    }

    #[test]
    fn test_missing_type() {
        assert!(fails("fn main(x) { }"));  // missing type annotation
    }
}

// =============================================================================
// QUANTALANG EXTENSION TESTS
// =============================================================================

mod quantalang_extensions {
    use super::*;

    #[test]
    fn test_effect_definition() {
        assert!(parses("effect Console { fn print(msg: &str); fn read() -> String; }"));
    }

    // TODO: Add more QuantaLang-specific tests when features are fully implemented
}
