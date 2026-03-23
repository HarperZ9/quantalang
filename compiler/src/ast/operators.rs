// ===============================================================================
// QUANTALANG AST - OPERATORS
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Operator definitions and precedence for QuantaLang.
//!
//! Precedence levels follow Rust conventions with QuantaLang extensions:
//! - Higher numbers = tighter binding
//! - Associativity determines left-to-right vs right-to-left parsing

use std::fmt;

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    // =========================================================================
    // ARITHMETIC OPERATORS
    // =========================================================================

    /// Addition: `+`
    Add,
    /// Subtraction: `-`
    Sub,
    /// Multiplication: `*`
    Mul,
    /// Division: `/`
    Div,
    /// Remainder/Modulo: `%`
    Rem,
    /// Power: `**` (QuantaLang extension)
    Pow,

    // =========================================================================
    // BITWISE OPERATORS
    // =========================================================================

    /// Bitwise AND: `&`
    BitAnd,
    /// Bitwise OR: `|`
    BitOr,
    /// Bitwise XOR: `^`
    BitXor,
    /// Left shift: `<<`
    Shl,
    /// Right shift: `>>`
    Shr,

    // =========================================================================
    // LOGICAL OPERATORS
    // =========================================================================

    /// Logical AND: `&&`
    And,
    /// Logical OR: `||`
    Or,

    // =========================================================================
    // COMPARISON OPERATORS
    // =========================================================================

    /// Equality: `==`
    Eq,
    /// Inequality: `!=`
    Ne,
    /// Less than: `<`
    Lt,
    /// Less than or equal: `<=`
    Le,
    /// Greater than: `>`
    Gt,
    /// Greater than or equal: `>=`
    Ge,

    // =========================================================================
    // RANGE OPERATORS
    // =========================================================================

    /// Exclusive range: `..`
    Range,
    /// Inclusive range: `..=`
    RangeInclusive,

    // =========================================================================
    // SPECIAL OPERATORS (QuantaLang extensions)
    // =========================================================================

    /// Pipe operator: `|>` (function application)
    Pipe,
    /// Compose operator: `>>` (function composition)
    Compose,
}

impl BinOp {
    /// Get the precedence of this operator.
    /// Higher values bind tighter.
    pub fn precedence(&self) -> u8 {
        match self {
            // Lowest precedence
            BinOp::Range | BinOp::RangeInclusive => 1,

            // Logical OR
            BinOp::Or => 2,

            // Logical AND
            BinOp::And => 3,

            // Comparison
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 4,

            // Bitwise OR
            BinOp::BitOr => 5,

            // Bitwise XOR
            BinOp::BitXor => 6,

            // Bitwise AND
            BinOp::BitAnd => 7,

            // Shift
            BinOp::Shl | BinOp::Shr => 8,

            // Pipe and compose (special operators)
            BinOp::Pipe | BinOp::Compose => 9,

            // Addition and subtraction
            BinOp::Add | BinOp::Sub => 10,

            // Multiplication, division, remainder
            BinOp::Mul | BinOp::Div | BinOp::Rem => 11,

            // Power (highest arithmetic precedence)
            BinOp::Pow => 12,
        }
    }

    /// Get the associativity of this operator.
    pub fn associativity(&self) -> Associativity {
        match self {
            // Right-associative operators
            BinOp::Pow => Associativity::Right,
            BinOp::Pipe | BinOp::Compose => Associativity::Left,

            // Most operators are left-associative
            _ => Associativity::Left,
        }
    }

    /// Get the binding power for Pratt parsing.
    /// Returns (left_bp, right_bp) where higher = tighter binding.
    pub fn binding_power(&self) -> (u8, u8) {
        let prec = self.precedence() * 2;
        match self.associativity() {
            Associativity::Left => (prec, prec + 1),
            Associativity::Right => (prec + 1, prec),
            Associativity::None => (prec, prec),
        }
    }

    /// Get the operator symbol.
    pub fn as_str(&self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Pow => "**",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
            BinOp::And => "&&",
            BinOp::Or => "||",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::Range => "..",
            BinOp::RangeInclusive => "..=",
            BinOp::Pipe => "|>",
            BinOp::Compose => ">>",
        }
    }

    /// Check if this is a comparison operator.
    pub fn is_comparison(&self) -> bool {
        matches!(
            self,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
        )
    }

    /// Check if this is a logical operator.
    pub fn is_logical(&self) -> bool {
        matches!(self, BinOp::And | BinOp::Or)
    }

    /// Check if this is an arithmetic operator.
    pub fn is_arithmetic(&self) -> bool {
        matches!(
            self,
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem | BinOp::Pow
        )
    }

    /// Check if this is a bitwise operator.
    pub fn is_bitwise(&self) -> bool {
        matches!(
            self,
            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr
        )
    }
}

impl fmt::Display for BinOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Compound assignment operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssignOp {
    /// `=`
    Assign,
    /// `+=`
    AddAssign,
    /// `-=`
    SubAssign,
    /// `*=`
    MulAssign,
    /// `/=`
    DivAssign,
    /// `%=`
    RemAssign,
    /// `&=`
    BitAndAssign,
    /// `|=`
    BitOrAssign,
    /// `^=`
    BitXorAssign,
    /// `<<=`
    ShlAssign,
    /// `>>=`
    ShrAssign,
}

impl AssignOp {
    /// Get the corresponding binary operator (if any).
    pub fn to_bin_op(&self) -> Option<BinOp> {
        match self {
            AssignOp::Assign => None,
            AssignOp::AddAssign => Some(BinOp::Add),
            AssignOp::SubAssign => Some(BinOp::Sub),
            AssignOp::MulAssign => Some(BinOp::Mul),
            AssignOp::DivAssign => Some(BinOp::Div),
            AssignOp::RemAssign => Some(BinOp::Rem),
            AssignOp::BitAndAssign => Some(BinOp::BitAnd),
            AssignOp::BitOrAssign => Some(BinOp::BitOr),
            AssignOp::BitXorAssign => Some(BinOp::BitXor),
            AssignOp::ShlAssign => Some(BinOp::Shl),
            AssignOp::ShrAssign => Some(BinOp::Shr),
        }
    }

    /// Get the operator symbol.
    pub fn as_str(&self) -> &'static str {
        match self {
            AssignOp::Assign => "=",
            AssignOp::AddAssign => "+=",
            AssignOp::SubAssign => "-=",
            AssignOp::MulAssign => "*=",
            AssignOp::DivAssign => "/=",
            AssignOp::RemAssign => "%=",
            AssignOp::BitAndAssign => "&=",
            AssignOp::BitOrAssign => "|=",
            AssignOp::BitXorAssign => "^=",
            AssignOp::ShlAssign => "<<=",
            AssignOp::ShrAssign => ">>=",
        }
    }
}

impl fmt::Display for AssignOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    /// Negation: `-`
    Neg,
    /// Logical NOT: `!`
    Not,
    /// Bitwise NOT: `~`
    BitNot,
    /// Dereference: `*`
    Deref,
    /// Reference: `&`
    Ref,
    /// Mutable reference: `&mut`
    RefMut,
}

impl UnaryOp {
    /// Get the binding power for prefix operators.
    pub fn prefix_binding_power(&self) -> u8 {
        // All prefix operators have the same (high) precedence
        25
    }

    /// Get the operator symbol.
    pub fn as_str(&self) -> &'static str {
        match self {
            UnaryOp::Neg => "-",
            UnaryOp::Not => "!",
            UnaryOp::BitNot => "~",
            UnaryOp::Deref => "*",
            UnaryOp::Ref => "&",
            UnaryOp::RefMut => "&mut",
        }
    }
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Associativity of an operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Associativity {
    /// Left-to-right: `a + b + c` = `(a + b) + c`
    Left,
    /// Right-to-left: `a ** b ** c` = `a ** (b ** c)`
    Right,
    /// Non-associative (comparison chaining)
    None,
}

/// Postfix operators and their binding power.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostfixOp {
    /// Function call: `f(args)`
    Call,
    /// Method call: `x.method(args)`
    MethodCall,
    /// Field access: `x.field`
    Field,
    /// Index: `x[index]`
    Index,
    /// Try operator: `x?`
    Try,
    /// Await: `x.await`
    Await,
}

impl PostfixOp {
    /// Get the binding power for postfix operators.
    /// All postfix operators bind very tightly.
    pub fn binding_power(&self) -> u8 {
        27 // Higher than any prefix or infix operator
    }
}

/// Operator precedence levels (for reference).
/// These are the actual values used in the Pratt parser.
pub mod precedence {
    /// Assignment (lowest)
    pub const ASSIGN: u8 = 0;
    /// Range operators
    pub const RANGE: u8 = 2;
    /// Logical OR
    pub const OR: u8 = 4;
    /// Logical AND
    pub const AND: u8 = 6;
    /// Comparison
    pub const COMPARE: u8 = 8;
    /// Bitwise OR
    pub const BIT_OR: u8 = 10;
    /// Bitwise XOR
    pub const BIT_XOR: u8 = 12;
    /// Bitwise AND
    pub const BIT_AND: u8 = 14;
    /// Shift
    pub const SHIFT: u8 = 16;
    /// Pipe operator
    pub const PIPE: u8 = 18;
    /// Addition/Subtraction
    pub const SUM: u8 = 20;
    /// Multiplication/Division
    pub const PRODUCT: u8 = 22;
    /// Power
    pub const POWER: u8 = 24;
    /// Prefix operators
    pub const PREFIX: u8 = 25;
    /// Postfix operators (highest)
    pub const POSTFIX: u8 = 27;
    /// Type ascription
    pub const AS: u8 = 26;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_precedence_ordering() {
        // Multiplication binds tighter than addition
        assert!(BinOp::Mul.precedence() > BinOp::Add.precedence());

        // Logical AND binds tighter than OR
        assert!(BinOp::And.precedence() > BinOp::Or.precedence());

        // Comparison is between logical operators and arithmetic
        assert!(BinOp::Eq.precedence() > BinOp::Or.precedence());
        assert!(BinOp::Eq.precedence() < BinOp::Add.precedence());
    }

    #[test]
    fn test_binding_power() {
        // Left-associative: left < right
        let (l, r) = BinOp::Add.binding_power();
        assert!(l < r);

        // Right-associative: left > right
        let (l, r) = BinOp::Pow.binding_power();
        assert!(l > r);
    }

    #[test]
    fn test_assign_op_conversion() {
        assert_eq!(AssignOp::AddAssign.to_bin_op(), Some(BinOp::Add));
        assert_eq!(AssignOp::Assign.to_bin_op(), None);
    }
}
