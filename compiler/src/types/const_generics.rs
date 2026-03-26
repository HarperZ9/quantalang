// ===============================================================================
// QUANTALANG TYPE SYSTEM - CONST GENERICS
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Const generics for compile-time values as type parameters.
//!
//! Const generics allow types to be parameterized by compile-time constant
//! values, enabling:
//! - Fixed-size arrays with generic length: `[T; N]`
//! - Bit widths: `Int<N>` for N-bit integers
//! - Compile-time dimensions: `Matrix<T, M, N>`
//! - Dependent types (limited)
//!
//! ## Syntax
//!
//! ```quanta
//! // Array with const generic length
//! struct Array<T, const N: usize> {
//!     data: [T; N],
//! }
//!
//! // Function with const generic
//! fn zeros<const N: usize>() -> [i32; N] {
//!     [0; N]
//! }
//!
//! // Const expressions in types
//! fn concat<const M: usize, const N: usize>(
//!     a: [i32; M],
//!     b: [i32; N],
//! ) -> [i32; M + N] {
//!     // ...
//! }
//! ```
//!
//! ## Const Expressions
//!
//! Limited expressions are allowed in const contexts:
//! - Literals
//! - Const parameters
//! - Arithmetic operations (+, -, *, /, %)
//! - Comparisons (==, !=, <, <=, >, >=)
//! - Logical operations (&&, ||, !)
//! - Const function calls

use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use super::{Ty, TyKind, IntTy};

/// A unique identifier for const variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstVarId(pub u32);

impl ConstVarId {
    /// Create a fresh const variable ID.
    pub fn fresh() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

impl fmt::Display for ConstVarId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "?C{}", self.0)
    }
}

/// A compile-time constant value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConstValue {
    /// Boolean constant.
    Bool(bool),

    /// Integer constant with type.
    Int(i128, IntTy),

    /// Unsigned integer constant.
    Uint(u128, IntTy),

    /// Character constant.
    Char(char),

    /// String constant.
    Str(Arc<str>),

    /// Array of constants.
    Array(Vec<ConstValue>),

    /// Tuple of constants.
    Tuple(Vec<ConstValue>),

    /// Named constant reference.
    Named(Arc<str>),

    /// Error value (for error recovery).
    Error,
}

impl ConstValue {
    /// Create a boolean constant.
    pub fn bool(value: bool) -> Self {
        ConstValue::Bool(value)
    }

    /// Create an i32 constant.
    pub fn i32(value: i32) -> Self {
        ConstValue::Int(value as i128, IntTy::I32)
    }

    /// Create an i64 constant.
    pub fn i64(value: i64) -> Self {
        ConstValue::Int(value as i128, IntTy::I64)
    }

    /// Create a usize constant.
    pub fn usize(value: usize) -> Self {
        ConstValue::Uint(value as u128, IntTy::Usize)
    }

    /// Create a char constant.
    pub fn char(value: char) -> Self {
        ConstValue::Char(value)
    }

    /// Create a string constant.
    pub fn str(value: impl Into<Arc<str>>) -> Self {
        ConstValue::Str(value.into())
    }

    /// Get the type of this constant.
    pub fn ty(&self) -> Ty {
        match self {
            ConstValue::Bool(_) => Ty::bool(),
            ConstValue::Int(_, int_ty) => Ty::int(*int_ty),
            ConstValue::Uint(_, int_ty) => Ty::int(*int_ty),
            ConstValue::Char(_) => Ty::char(),
            ConstValue::Str(_) => Ty::str(),
            ConstValue::Array(elems) => {
                if let Some(first) = elems.first() {
                    Ty::array(first.ty(), elems.len())
                } else {
                    Ty::array(Ty::fresh_var(), 0)
                }
            }
            ConstValue::Tuple(elems) => {
                Ty::tuple(elems.iter().map(|e| e.ty()).collect())
            }
            ConstValue::Named(_) => Ty::fresh_var(), // Type inferred
            ConstValue::Error => Ty::error(),
        }
    }

    /// Try to convert to a usize.
    pub fn to_usize(&self) -> Option<usize> {
        match self {
            ConstValue::Int(v, _) if *v >= 0 => Some(*v as usize),
            ConstValue::Uint(v, _) => Some(*v as usize),
            _ => None,
        }
    }

    /// Try to convert to an i128.
    pub fn to_i128(&self) -> Option<i128> {
        match self {
            ConstValue::Int(v, _) => Some(*v),
            ConstValue::Uint(v, _) => Some(*v as i128),
            _ => None,
        }
    }

    /// Try to convert to a bool.
    pub fn to_bool(&self) -> Option<bool> {
        match self {
            ConstValue::Bool(v) => Some(*v),
            _ => None,
        }
    }
}

impl fmt::Display for ConstValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConstValue::Bool(v) => write!(f, "{}", v),
            ConstValue::Int(v, ty) => write!(f, "{}{}", v, ty),
            ConstValue::Uint(v, ty) => write!(f, "{}{}", v, ty),
            ConstValue::Char(c) => write!(f, "'{}'", c),
            ConstValue::Str(s) => write!(f, "\"{}\"", s),
            ConstValue::Array(elems) => {
                write!(f, "[")?;
                for (i, elem) in elems.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", elem)?;
                }
                write!(f, "]")
            }
            ConstValue::Tuple(elems) => {
                write!(f, "(")?;
                for (i, elem) in elems.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", elem)?;
                }
                write!(f, ")")
            }
            ConstValue::Named(name) => write!(f, "{}", name),
            ConstValue::Error => write!(f, "{{error}}"),
        }
    }
}

/// A const generic expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConstExpr {
    /// A literal value.
    Literal(ConstValue),

    /// A const parameter reference.
    Param(ConstParam),

    /// A const variable (for inference).
    Var(ConstVarId),

    /// Binary operation.
    Binary {
        op: ConstBinOp,
        left: Box<ConstExpr>,
        right: Box<ConstExpr>,
    },

    /// Unary operation.
    Unary {
        op: ConstUnaryOp,
        operand: Box<ConstExpr>,
    },

    /// Conditional expression.
    If {
        condition: Box<ConstExpr>,
        then_branch: Box<ConstExpr>,
        else_branch: Box<ConstExpr>,
    },

    /// Const function call.
    Call {
        func: Arc<str>,
        args: Vec<ConstExpr>,
    },

    /// Array length.
    ArrayLen(Box<ConstExpr>),

    /// Size of a type.
    SizeOf(Box<Ty>),

    /// Alignment of a type.
    AlignOf(Box<Ty>),

    /// Error expression.
    Error,
}

impl ConstExpr {
    /// Create a literal expression.
    pub fn literal(value: ConstValue) -> Self {
        ConstExpr::Literal(value)
    }

    /// Create a const parameter reference.
    pub fn param(param: ConstParam) -> Self {
        ConstExpr::Param(param)
    }

    /// Create a fresh const variable.
    pub fn fresh_var() -> Self {
        ConstExpr::Var(ConstVarId::fresh())
    }

    /// Create an addition expression.
    pub fn add(left: ConstExpr, right: ConstExpr) -> Self {
        ConstExpr::Binary {
            op: ConstBinOp::Add,
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    /// Create a subtraction expression.
    pub fn sub(left: ConstExpr, right: ConstExpr) -> Self {
        ConstExpr::Binary {
            op: ConstBinOp::Sub,
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    /// Create a multiplication expression.
    pub fn mul(left: ConstExpr, right: ConstExpr) -> Self {
        ConstExpr::Binary {
            op: ConstBinOp::Mul,
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    /// Try to evaluate this expression to a constant value.
    pub fn evaluate(&self, ctx: &ConstEvalContext) -> Result<ConstValue, ConstEvalError> {
        match self {
            ConstExpr::Literal(v) => Ok(v.clone()),

            ConstExpr::Param(param) => {
                ctx.get_param(&param.name)
                    .cloned()
                    .ok_or_else(|| ConstEvalError::UnboundParam(param.name.to_string()))
            }

            ConstExpr::Var(var) => {
                ctx.get_var(*var)
                    .cloned()
                    .ok_or(ConstEvalError::UnresolvedVar(*var))
            }

            ConstExpr::Binary { op, left, right } => {
                let lval = left.evaluate(ctx)?;
                let rval = right.evaluate(ctx)?;
                op.evaluate(&lval, &rval)
            }

            ConstExpr::Unary { op, operand } => {
                let val = operand.evaluate(ctx)?;
                op.evaluate(&val)
            }

            ConstExpr::If { condition, then_branch, else_branch } => {
                let cond = condition.evaluate(ctx)?;
                match cond {
                    ConstValue::Bool(true) => then_branch.evaluate(ctx),
                    ConstValue::Bool(false) => else_branch.evaluate(ctx),
                    _ => Err(ConstEvalError::TypeMismatch {
                        expected: "bool".to_string(),
                        found: format!("{}", cond.ty()),
                    }),
                }
            }

            ConstExpr::Call { func, args } => {
                let arg_vals: Result<Vec<_>, _> = args.iter()
                    .map(|a| a.evaluate(ctx))
                    .collect();
                ctx.call_function(func, &arg_vals?)
            }

            ConstExpr::ArrayLen(arr) => {
                let val = arr.evaluate(ctx)?;
                match val {
                    ConstValue::Array(elems) => Ok(ConstValue::usize(elems.len())),
                    ConstValue::Str(s) => Ok(ConstValue::usize(s.len())),
                    _ => Err(ConstEvalError::TypeMismatch {
                        expected: "array or string".to_string(),
                        found: format!("{}", val.ty()),
                    }),
                }
            }

            ConstExpr::SizeOf(ty) => {
                // Simplified - would need full layout calculation
                let size = match &ty.kind {
                    TyKind::Bool => 1,
                    TyKind::Char => 4,
                    TyKind::Int(IntTy::I8) | TyKind::Int(IntTy::U8) => 1,
                    TyKind::Int(IntTy::I16) | TyKind::Int(IntTy::U16) => 2,
                    TyKind::Int(IntTy::I32) | TyKind::Int(IntTy::U32) => 4,
                    TyKind::Int(IntTy::I64) | TyKind::Int(IntTy::U64) => 8,
                    TyKind::Int(IntTy::I128) | TyKind::Int(IntTy::U128) => 16,
                    TyKind::Int(IntTy::Isize) | TyKind::Int(IntTy::Usize) => 8, // 64-bit
                    TyKind::Float(super::FloatTy::F16) => 2,
                    TyKind::Float(super::FloatTy::F32) => 4,
                    TyKind::Float(super::FloatTy::F64) => 8,
                    TyKind::Ptr(_, _) | TyKind::Ref(_, _, _) => 8,
                    _ => return Err(ConstEvalError::CannotCompute("sizeof".to_string())),
                };
                Ok(ConstValue::usize(size))
            }

            ConstExpr::AlignOf(ty) => {
                // Simplified alignment
                let align = match &ty.kind {
                    TyKind::Bool | TyKind::Int(IntTy::I8) | TyKind::Int(IntTy::U8) => 1,
                    TyKind::Int(IntTy::I16) | TyKind::Int(IntTy::U16) => 2,
                    TyKind::Int(IntTy::I32) | TyKind::Int(IntTy::U32) |
                    TyKind::Float(super::FloatTy::F32) => 4,
                    TyKind::Int(IntTy::I64) | TyKind::Int(IntTy::U64) |
                    TyKind::Float(super::FloatTy::F64) |
                    TyKind::Ptr(_, _) | TyKind::Ref(_, _, _) => 8,
                    TyKind::Int(IntTy::I128) | TyKind::Int(IntTy::U128) => 16,
                    _ => return Err(ConstEvalError::CannotCompute("alignof".to_string())),
                };
                Ok(ConstValue::usize(align))
            }

            ConstExpr::Error => Err(ConstEvalError::EvalError("error expression".to_string())),
        }
    }

    /// Check if this expression is a simple literal or param.
    pub fn is_simple(&self) -> bool {
        matches!(self, ConstExpr::Literal(_) | ConstExpr::Param(_))
    }

    /// Check if this expression contains any variables.
    pub fn has_vars(&self) -> bool {
        match self {
            ConstExpr::Var(_) => true,
            ConstExpr::Literal(_) | ConstExpr::Param(_) => false,
            ConstExpr::Binary { left, right, .. } => left.has_vars() || right.has_vars(),
            ConstExpr::Unary { operand, .. } => operand.has_vars(),
            ConstExpr::If { condition, then_branch, else_branch } => {
                condition.has_vars() || then_branch.has_vars() || else_branch.has_vars()
            }
            ConstExpr::Call { args, .. } => args.iter().any(|a| a.has_vars()),
            ConstExpr::ArrayLen(e) => e.has_vars(),
            ConstExpr::SizeOf(_) | ConstExpr::AlignOf(_) => false,
            ConstExpr::Error => false,
        }
    }

    /// Substitute const variables.
    pub fn substitute(&self, subst: &ConstSubstitution) -> ConstExpr {
        match self {
            ConstExpr::Var(var) => {
                if let Some(expr) = subst.get(*var) {
                    expr.substitute(subst)
                } else {
                    self.clone()
                }
            }
            ConstExpr::Binary { op, left, right } => {
                ConstExpr::Binary {
                    op: *op,
                    left: Box::new(left.substitute(subst)),
                    right: Box::new(right.substitute(subst)),
                }
            }
            ConstExpr::Unary { op, operand } => {
                ConstExpr::Unary {
                    op: *op,
                    operand: Box::new(operand.substitute(subst)),
                }
            }
            ConstExpr::If { condition, then_branch, else_branch } => {
                ConstExpr::If {
                    condition: Box::new(condition.substitute(subst)),
                    then_branch: Box::new(then_branch.substitute(subst)),
                    else_branch: Box::new(else_branch.substitute(subst)),
                }
            }
            ConstExpr::Call { func, args } => {
                ConstExpr::Call {
                    func: func.clone(),
                    args: args.iter().map(|a| a.substitute(subst)).collect(),
                }
            }
            ConstExpr::ArrayLen(e) => ConstExpr::ArrayLen(Box::new(e.substitute(subst))),
            _ => self.clone(),
        }
    }
}

impl fmt::Display for ConstExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConstExpr::Literal(v) => write!(f, "{}", v),
            ConstExpr::Param(p) => write!(f, "{}", p.name),
            ConstExpr::Var(v) => write!(f, "{}", v),
            ConstExpr::Binary { op, left, right } => write!(f, "({} {} {})", left, op, right),
            ConstExpr::Unary { op, operand } => write!(f, "{}{}", op, operand),
            ConstExpr::If { condition, then_branch, else_branch } => {
                write!(f, "if {} {{ {} }} else {{ {} }}", condition, then_branch, else_branch)
            }
            ConstExpr::Call { func, args } => {
                write!(f, "{}(", func)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ")")
            }
            ConstExpr::ArrayLen(e) => write!(f, "len({})", e),
            ConstExpr::SizeOf(ty) => write!(f, "sizeof({})", ty),
            ConstExpr::AlignOf(ty) => write!(f, "alignof({})", ty),
            ConstExpr::Error => write!(f, "{{error}}"),
        }
    }
}

/// Binary operations for const expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConstBinOp {
    // Arithmetic
    Add, Sub, Mul, Div, Rem,
    // Bitwise
    BitAnd, BitOr, BitXor, Shl, Shr,
    // Comparison
    Eq, Ne, Lt, Le, Gt, Ge,
    // Logical
    And, Or,
}

impl ConstBinOp {
    /// Evaluate this operation on two values.
    pub fn evaluate(&self, left: &ConstValue, right: &ConstValue) -> Result<ConstValue, ConstEvalError> {
        match (left, right) {
            (ConstValue::Int(l, ty), ConstValue::Int(r, _)) => {
                let result = match self {
                    ConstBinOp::Add => l.checked_add(*r),
                    ConstBinOp::Sub => l.checked_sub(*r),
                    ConstBinOp::Mul => l.checked_mul(*r),
                    ConstBinOp::Div => l.checked_div(*r),
                    ConstBinOp::Rem => l.checked_rem(*r),
                    ConstBinOp::BitAnd => Some(l & r),
                    ConstBinOp::BitOr => Some(l | r),
                    ConstBinOp::BitXor => Some(l ^ r),
                    ConstBinOp::Shl => l.checked_shl(*r as u32),
                    ConstBinOp::Shr => l.checked_shr(*r as u32),
                    ConstBinOp::Eq => return Ok(ConstValue::bool(l == r)),
                    ConstBinOp::Ne => return Ok(ConstValue::bool(l != r)),
                    ConstBinOp::Lt => return Ok(ConstValue::bool(l < r)),
                    ConstBinOp::Le => return Ok(ConstValue::bool(l <= r)),
                    ConstBinOp::Gt => return Ok(ConstValue::bool(l > r)),
                    ConstBinOp::Ge => return Ok(ConstValue::bool(l >= r)),
                    ConstBinOp::And | ConstBinOp::Or => {
                        return Err(ConstEvalError::TypeMismatch {
                            expected: "bool".to_string(),
                            found: format!("{}", ty),
                        });
                    }
                };
                result
                    .map(|v| ConstValue::Int(v, *ty))
                    .ok_or(ConstEvalError::Overflow)
            }

            (ConstValue::Uint(l, ty), ConstValue::Uint(r, _)) => {
                let result = match self {
                    ConstBinOp::Add => l.checked_add(*r),
                    ConstBinOp::Sub => l.checked_sub(*r),
                    ConstBinOp::Mul => l.checked_mul(*r),
                    ConstBinOp::Div => l.checked_div(*r),
                    ConstBinOp::Rem => l.checked_rem(*r),
                    ConstBinOp::BitAnd => Some(l & r),
                    ConstBinOp::BitOr => Some(l | r),
                    ConstBinOp::BitXor => Some(l ^ r),
                    ConstBinOp::Shl => l.checked_shl(*r as u32),
                    ConstBinOp::Shr => l.checked_shr(*r as u32),
                    ConstBinOp::Eq => return Ok(ConstValue::bool(l == r)),
                    ConstBinOp::Ne => return Ok(ConstValue::bool(l != r)),
                    ConstBinOp::Lt => return Ok(ConstValue::bool(l < r)),
                    ConstBinOp::Le => return Ok(ConstValue::bool(l <= r)),
                    ConstBinOp::Gt => return Ok(ConstValue::bool(l > r)),
                    ConstBinOp::Ge => return Ok(ConstValue::bool(l >= r)),
                    ConstBinOp::And | ConstBinOp::Or => {
                        return Err(ConstEvalError::TypeMismatch {
                            expected: "bool".to_string(),
                            found: format!("{}", ty),
                        });
                    }
                };
                result
                    .map(|v| ConstValue::Uint(v, *ty))
                    .ok_or(ConstEvalError::Overflow)
            }

            (ConstValue::Bool(l), ConstValue::Bool(r)) => {
                match self {
                    ConstBinOp::And => Ok(ConstValue::bool(*l && *r)),
                    ConstBinOp::Or => Ok(ConstValue::bool(*l || *r)),
                    ConstBinOp::Eq => Ok(ConstValue::bool(l == r)),
                    ConstBinOp::Ne => Ok(ConstValue::bool(l != r)),
                    _ => Err(ConstEvalError::InvalidOperation(format!(
                        "cannot apply {} to booleans", self
                    ))),
                }
            }

            _ => Err(ConstEvalError::TypeMismatch {
                expected: format!("{}", left.ty()),
                found: format!("{}", right.ty()),
            }),
        }
    }
}

impl fmt::Display for ConstBinOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConstBinOp::Add => write!(f, "+"),
            ConstBinOp::Sub => write!(f, "-"),
            ConstBinOp::Mul => write!(f, "*"),
            ConstBinOp::Div => write!(f, "/"),
            ConstBinOp::Rem => write!(f, "%"),
            ConstBinOp::BitAnd => write!(f, "&"),
            ConstBinOp::BitOr => write!(f, "|"),
            ConstBinOp::BitXor => write!(f, "^"),
            ConstBinOp::Shl => write!(f, "<<"),
            ConstBinOp::Shr => write!(f, ">>"),
            ConstBinOp::Eq => write!(f, "=="),
            ConstBinOp::Ne => write!(f, "!="),
            ConstBinOp::Lt => write!(f, "<"),
            ConstBinOp::Le => write!(f, "<="),
            ConstBinOp::Gt => write!(f, ">"),
            ConstBinOp::Ge => write!(f, ">="),
            ConstBinOp::And => write!(f, "&&"),
            ConstBinOp::Or => write!(f, "||"),
        }
    }
}

/// Unary operations for const expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConstUnaryOp {
    Neg,
    Not,
    BitNot,
}

impl ConstUnaryOp {
    /// Evaluate this operation on a value.
    pub fn evaluate(&self, operand: &ConstValue) -> Result<ConstValue, ConstEvalError> {
        match (self, operand) {
            (ConstUnaryOp::Neg, ConstValue::Int(v, ty)) => {
                v.checked_neg()
                    .map(|r| ConstValue::Int(r, *ty))
                    .ok_or(ConstEvalError::Overflow)
            }
            (ConstUnaryOp::Not, ConstValue::Bool(v)) => Ok(ConstValue::bool(!v)),
            (ConstUnaryOp::BitNot, ConstValue::Int(v, ty)) => Ok(ConstValue::Int(!v, *ty)),
            (ConstUnaryOp::BitNot, ConstValue::Uint(v, ty)) => Ok(ConstValue::Uint(!v, *ty)),
            _ => Err(ConstEvalError::InvalidOperation(format!(
                "cannot apply {} to {}", self, operand.ty()
            ))),
        }
    }
}

impl fmt::Display for ConstUnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConstUnaryOp::Neg => write!(f, "-"),
            ConstUnaryOp::Not => write!(f, "!"),
            ConstUnaryOp::BitNot => write!(f, "~"),
        }
    }
}

/// A const generic parameter.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConstParam {
    /// Parameter name.
    pub name: Arc<str>,
    /// Parameter index.
    pub index: u32,
    /// The type of this const parameter.
    pub ty: Ty,
}

impl ConstParam {
    /// Create a new const parameter.
    pub fn new(name: impl Into<Arc<str>>, index: u32, ty: Ty) -> Self {
        Self {
            name: name.into(),
            index,
            ty,
        }
    }

    /// Create a usize const parameter.
    pub fn usize_param(name: impl Into<Arc<str>>, index: u32) -> Self {
        Self::new(name, index, Ty::int(IntTy::Usize))
    }
}

impl fmt::Display for ConstParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "const {}: {}", self.name, self.ty)
    }
}

/// A substitution for const variables.
#[derive(Debug, Clone, Default)]
pub struct ConstSubstitution {
    map: HashMap<ConstVarId, ConstExpr>,
}

impl ConstSubstitution {
    /// Create an empty substitution.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a mapping.
    pub fn insert(&mut self, var: ConstVarId, expr: ConstExpr) {
        self.map.insert(var, expr);
    }

    /// Get the expression for a variable.
    pub fn get(&self, var: ConstVarId) -> Option<&ConstExpr> {
        self.map.get(&var)
    }

    /// Check if a variable is bound.
    pub fn contains(&self, var: ConstVarId) -> bool {
        self.map.contains_key(&var)
    }
}

/// Context for const evaluation.
#[derive(Debug, Default)]
pub struct ConstEvalContext {
    /// Const parameter values.
    params: HashMap<Arc<str>, ConstValue>,
    /// Const variable values.
    vars: HashMap<ConstVarId, ConstValue>,
    /// Const functions.
    functions: HashMap<Arc<str>, ConstFn>,
}

impl ConstEvalContext {
    /// Create a new evaluation context.
    pub fn new() -> Self {
        let mut ctx = Self::default();
        ctx.register_builtin_functions();
        ctx
    }

    /// Register built-in const functions.
    fn register_builtin_functions(&mut self) {
        // min
        self.functions.insert(
            Arc::from("min"),
            ConstFn::new(2, |args| {
                match (&args[0], &args[1]) {
                    (ConstValue::Int(a, ty), ConstValue::Int(b, _)) => {
                        Ok(ConstValue::Int((*a).min(*b), *ty))
                    }
                    (ConstValue::Uint(a, ty), ConstValue::Uint(b, _)) => {
                        Ok(ConstValue::Uint((*a).min(*b), *ty))
                    }
                    _ => Err(ConstEvalError::TypeMismatch {
                        expected: "numeric".to_string(),
                        found: format!("{}", args[0].ty()),
                    }),
                }
            }),
        );

        // max
        self.functions.insert(
            Arc::from("max"),
            ConstFn::new(2, |args| {
                match (&args[0], &args[1]) {
                    (ConstValue::Int(a, ty), ConstValue::Int(b, _)) => {
                        Ok(ConstValue::Int((*a).max(*b), *ty))
                    }
                    (ConstValue::Uint(a, ty), ConstValue::Uint(b, _)) => {
                        Ok(ConstValue::Uint((*a).max(*b), *ty))
                    }
                    _ => Err(ConstEvalError::TypeMismatch {
                        expected: "numeric".to_string(),
                        found: format!("{}", args[0].ty()),
                    }),
                }
            }),
        );

        // pow
        self.functions.insert(
            Arc::from("pow"),
            ConstFn::new(2, |args| {
                match (&args[0], &args[1]) {
                    (ConstValue::Int(base, ty), ConstValue::Uint(exp, _)) => {
                        let exp = *exp as u32;
                        base.checked_pow(exp)
                            .map(|r| ConstValue::Int(r, *ty))
                            .ok_or(ConstEvalError::Overflow)
                    }
                    (ConstValue::Uint(base, ty), ConstValue::Uint(exp, _)) => {
                        let exp = *exp as u32;
                        base.checked_pow(exp)
                            .map(|r| ConstValue::Uint(r, *ty))
                            .ok_or(ConstEvalError::Overflow)
                    }
                    _ => Err(ConstEvalError::TypeMismatch {
                        expected: "numeric".to_string(),
                        found: format!("{}", args[0].ty()),
                    }),
                }
            }),
        );
    }

    /// Set a const parameter value.
    pub fn set_param(&mut self, name: impl Into<Arc<str>>, value: ConstValue) {
        self.params.insert(name.into(), value);
    }

    /// Get a const parameter value.
    pub fn get_param(&self, name: &str) -> Option<&ConstValue> {
        self.params.get(name)
    }

    /// Set a const variable value.
    pub fn set_var(&mut self, var: ConstVarId, value: ConstValue) {
        self.vars.insert(var, value);
    }

    /// Get a const variable value.
    pub fn get_var(&self, var: ConstVarId) -> Option<&ConstValue> {
        self.vars.get(&var)
    }

    /// Call a const function.
    pub fn call_function(&self, name: &str, args: &[ConstValue]) -> Result<ConstValue, ConstEvalError> {
        let func = self.functions.get(name)
            .ok_or_else(|| ConstEvalError::UnknownFunction(name.to_string()))?;

        if args.len() != func.arity {
            return Err(ConstEvalError::ArityMismatch {
                expected: func.arity,
                found: args.len(),
            });
        }

        (func.body)(args)
    }
}

/// A const function.
pub struct ConstFn {
    arity: usize,
    body: Box<dyn Fn(&[ConstValue]) -> Result<ConstValue, ConstEvalError> + Send + Sync>,
}

impl ConstFn {
    fn new<F>(arity: usize, body: F) -> Self
    where
        F: Fn(&[ConstValue]) -> Result<ConstValue, ConstEvalError> + Send + Sync + 'static,
    {
        Self {
            arity,
            body: Box::new(body),
        }
    }
}

impl fmt::Debug for ConstFn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConstFn")
            .field("arity", &self.arity)
            .finish()
    }
}

/// Const evaluation errors.
#[derive(Debug, Clone)]
pub enum ConstEvalError {
    /// Unbound const parameter.
    UnboundParam(String),

    /// Unresolved const variable.
    UnresolvedVar(ConstVarId),

    /// Type mismatch.
    TypeMismatch { expected: String, found: String },

    /// Arithmetic overflow.
    Overflow,

    /// Division by zero.
    DivByZero,

    /// Invalid operation.
    InvalidOperation(String),

    /// Cannot compute at compile time.
    CannotCompute(String),

    /// Unknown function.
    UnknownFunction(String),

    /// Arity mismatch.
    ArityMismatch { expected: usize, found: usize },

    /// General evaluation error.
    EvalError(String),
}

impl fmt::Display for ConstEvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConstEvalError::UnboundParam(name) => {
                write!(f, "unbound const parameter: {}", name)
            }
            ConstEvalError::UnresolvedVar(var) => {
                write!(f, "unresolved const variable: {}", var)
            }
            ConstEvalError::TypeMismatch { expected, found } => {
                write!(f, "type mismatch: expected {}, found {}", expected, found)
            }
            ConstEvalError::Overflow => write!(f, "arithmetic overflow"),
            ConstEvalError::DivByZero => write!(f, "division by zero"),
            ConstEvalError::InvalidOperation(op) => write!(f, "invalid operation: {}", op),
            ConstEvalError::CannotCompute(what) => {
                write!(f, "cannot compute {} at compile time", what)
            }
            ConstEvalError::UnknownFunction(name) => {
                write!(f, "unknown const function: {}", name)
            }
            ConstEvalError::ArityMismatch { expected, found } => {
                write!(f, "arity mismatch: expected {} arguments, found {}", expected, found)
            }
            ConstEvalError::EvalError(msg) => write!(f, "evaluation error: {}", msg),
        }
    }
}

impl std::error::Error for ConstEvalError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_const_value() {
        let i = ConstValue::i32(42);
        assert_eq!(format!("{}", i), "42i32");
        assert_eq!(i.to_i128(), Some(42));

        let b = ConstValue::bool(true);
        assert_eq!(format!("{}", b), "true");
        assert_eq!(b.to_bool(), Some(true));

        let u = ConstValue::usize(100);
        assert_eq!(u.to_usize(), Some(100));
    }

    #[test]
    fn test_const_expr_eval() {
        let ctx = ConstEvalContext::new();

        // Literal
        let lit = ConstExpr::literal(ConstValue::i32(42));
        assert_eq!(lit.evaluate(&ctx).unwrap(), ConstValue::i32(42));

        // Addition
        let add = ConstExpr::add(
            ConstExpr::literal(ConstValue::i32(10)),
            ConstExpr::literal(ConstValue::i32(20)),
        );
        assert_eq!(add.evaluate(&ctx).unwrap(), ConstValue::i32(30));

        // Multiplication
        let mul = ConstExpr::mul(
            ConstExpr::literal(ConstValue::i32(6)),
            ConstExpr::literal(ConstValue::i32(7)),
        );
        assert_eq!(mul.evaluate(&ctx).unwrap(), ConstValue::i32(42));
    }

    #[test]
    fn test_const_param() {
        let mut ctx = ConstEvalContext::new();
        ctx.set_param("N", ConstValue::usize(10));

        let param = ConstParam::usize_param("N", 0);
        let expr = ConstExpr::param(param);

        assert_eq!(expr.evaluate(&ctx).unwrap(), ConstValue::usize(10));
    }

    #[test]
    fn test_const_binary_ops() {
        let ctx = ConstEvalContext::new();

        // Comparison
        let lt = ConstExpr::Binary {
            op: ConstBinOp::Lt,
            left: Box::new(ConstExpr::literal(ConstValue::i32(5))),
            right: Box::new(ConstExpr::literal(ConstValue::i32(10))),
        };
        assert_eq!(lt.evaluate(&ctx).unwrap(), ConstValue::bool(true));

        // Boolean and
        let and = ConstExpr::Binary {
            op: ConstBinOp::And,
            left: Box::new(ConstExpr::literal(ConstValue::bool(true))),
            right: Box::new(ConstExpr::literal(ConstValue::bool(false))),
        };
        assert_eq!(and.evaluate(&ctx).unwrap(), ConstValue::bool(false));
    }

    #[test]
    fn test_const_functions() {
        let ctx = ConstEvalContext::new();

        // min
        let min_expr = ConstExpr::Call {
            func: Arc::from("min"),
            args: vec![
                ConstExpr::literal(ConstValue::i32(10)),
                ConstExpr::literal(ConstValue::i32(5)),
            ],
        };
        assert_eq!(min_expr.evaluate(&ctx).unwrap(), ConstValue::i32(5));

        // max
        let max_expr = ConstExpr::Call {
            func: Arc::from("max"),
            args: vec![
                ConstExpr::literal(ConstValue::i32(10)),
                ConstExpr::literal(ConstValue::i32(5)),
            ],
        };
        assert_eq!(max_expr.evaluate(&ctx).unwrap(), ConstValue::i32(10));
    }

    #[test]
    fn test_sizeof() {
        let ctx = ConstEvalContext::new();

        let sizeof_i32 = ConstExpr::SizeOf(Box::new(Ty::int(IntTy::I32)));
        assert_eq!(sizeof_i32.evaluate(&ctx).unwrap(), ConstValue::usize(4));

        let sizeof_i64 = ConstExpr::SizeOf(Box::new(Ty::int(IntTy::I64)));
        assert_eq!(sizeof_i64.evaluate(&ctx).unwrap(), ConstValue::usize(8));
    }
}
