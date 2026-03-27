// ===============================================================================
// QUANTALANG CODE GENERATOR - MID-LEVEL IR
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Mid-level Intermediate Representation (MIR).
//!
//! MIR is a control-flow graph based representation that sits between the AST
//! and target-specific code generation. It uses SSA (Static Single Assignment)
//! form for easier optimization and analysis.
//!
//! ## Design
//!
//! - Functions are represented as control-flow graphs
//! - Each basic block contains a sequence of statements and a terminator
//! - Values are in SSA form (each value assigned exactly once)
//! - Types are explicit and fully resolved

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

// ============================================================================
// MODULE STRUCTURE
// ============================================================================

/// Vtable definition for trait object dynamic dispatch.
#[derive(Debug, Clone)]
pub struct MirVtable {
    /// Trait name.
    pub trait_name: Arc<str>,
    /// Concrete type name.
    pub type_name: Arc<str>,
    /// Method entries: (method_name, mangled_function_name, fn_sig with void* self).
    pub methods: Vec<(Arc<str>, Arc<str>, MirFnSig)>,
}

/// A MIR module (compilation unit).
pub struct MirModule {
    /// Module name.
    pub name: Arc<str>,
    /// Functions.
    pub functions: Vec<MirFunction>,
    /// Global variables.
    pub globals: Vec<MirGlobal>,
    /// Type definitions.
    pub types: Vec<MirTypeDef>,
    /// String literals.
    pub strings: Vec<Arc<str>>,
    /// External declarations.
    pub externals: Vec<MirExternal>,
    /// Vtable definitions for dynamic dispatch.
    pub vtables: Vec<MirVtable>,
    /// Trait method signatures: trait_name → [(method_name, fn_sig)].
    pub trait_methods: HashMap<Arc<str>, Vec<(Arc<str>, MirFnSig)>>,
    /// Shader uniform declarations (#[uniform] on module-level constants).
    pub uniforms: Vec<MirUniform>,
}

/// A shader uniform declaration.
#[derive(Debug, Clone)]
pub struct MirUniform {
    /// Uniform name.
    pub name: Arc<str>,
    /// Uniform type.
    pub ty: MirType,
    /// Default value.
    pub default: Option<MirConst>,
}

impl MirModule {
    /// Create a new MIR module.
    pub fn new(name: impl Into<Arc<str>>) -> Self {
        Self {
            name: name.into(),
            functions: Vec::new(),
            globals: Vec::new(),
            types: Vec::new(),
            strings: Vec::new(),
            externals: Vec::new(),
            vtables: Vec::new(),
            trait_methods: HashMap::new(),
            uniforms: Vec::new(),
        }
    }

    /// Add a function.
    pub fn add_function(&mut self, func: MirFunction) {
        self.functions.push(func);
    }

    /// Add a global variable.
    pub fn add_global(&mut self, global: MirGlobal) {
        self.globals.push(global);
    }

    /// Find a global variable by name.
    pub fn find_global(&self, name: &str) -> Option<&MirGlobal> {
        self.globals.iter().find(|g| g.name.as_ref() == name)
    }

    /// Add a type definition.
    pub fn add_type(&mut self, ty: MirTypeDef) {
        self.types.push(ty);
    }

    /// Intern a string literal and return its index.
    pub fn intern_string(&mut self, s: impl Into<Arc<str>>) -> u32 {
        let s = s.into();
        if let Some(idx) = self.strings.iter().position(|x| x.as_ref() == s.as_ref()) {
            idx as u32
        } else {
            let idx = self.strings.len() as u32;
            self.strings.push(s);
            idx
        }
    }

    /// Find a function by name.
    pub fn find_function(&self, name: &str) -> Option<&MirFunction> {
        self.functions.iter().find(|f| f.name.as_ref() == name)
    }
}

// ============================================================================
// FUNCTIONS
// ============================================================================

/// GPU shader stage for graphics pipeline functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderStage {
    /// Vertex shader stage.
    Vertex,
    /// Fragment (pixel) shader stage.
    Fragment,
    /// Compute shader stage.
    Compute,
}

/// Descriptor binding kind for shader resources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingKind {
    /// Uniform buffer (constant data from CPU).
    UniformBuffer(Arc<str>),
    /// 2D texture (sampled image).
    Texture2D,
    /// Sampler (texture filtering/addressing).
    Sampler,
    /// Storage buffer (read/write from GPU).
    StorageBuffer(Arc<str>),
}

/// A shader resource binding (descriptor set + binding index).
#[derive(Debug, Clone)]
pub struct ShaderBinding {
    /// Descriptor set index.
    pub set: u32,
    /// Binding index within the set.
    pub binding: u32,
    /// What kind of resource this binding represents.
    pub kind: BindingKind,
    /// The MIR type of the bound resource.
    pub ty: MirType,
}

/// A MIR function.
#[derive(Debug, Clone)]
pub struct MirFunction {
    /// Function name.
    pub name: Arc<str>,
    /// Function signature.
    pub sig: MirFnSig,
    /// Basic blocks (None for declarations).
    pub blocks: Option<Vec<MirBlock>>,
    /// Local variables.
    pub locals: Vec<MirLocal>,
    /// Is this function public?
    pub is_public: bool,
    /// Linkage type.
    pub linkage: Linkage,
    /// Optional GPU shader stage (vertex, fragment, compute).
    pub shader_stage: Option<ShaderStage>,
    /// Shader resource bindings (uniform buffers, textures, samplers).
    pub bindings: Vec<ShaderBinding>,
}

impl MirFunction {
    /// Create a new function.
    pub fn new(name: impl Into<Arc<str>>, sig: MirFnSig) -> Self {
        Self {
            name: name.into(),
            sig,
            blocks: None,  // Start as declaration, add_block() makes it a definition
            locals: Vec::new(),
            is_public: false,
            linkage: Linkage::Internal,
            shader_stage: None,
            bindings: Vec::new(),
        }
    }

    /// Create a function declaration (no body).
    pub fn declaration(name: impl Into<Arc<str>>, sig: MirFnSig) -> Self {
        Self {
            name: name.into(),
            sig,
            blocks: None,
            locals: Vec::new(),
            is_public: false,
            linkage: Linkage::External,
            shader_stage: None,
            bindings: Vec::new(),
        }
    }

    /// Check if this is a declaration (no body).
    pub fn is_declaration(&self) -> bool {
        self.blocks.is_none()
    }

    /// Get the entry block.
    pub fn entry_block(&self) -> Option<&MirBlock> {
        self.blocks.as_ref().and_then(|b| b.first())
    }

    /// Get a block by ID.
    pub fn block(&self, id: BlockId) -> Option<&MirBlock> {
        self.blocks.as_ref().and_then(|b| b.get(id.0 as usize))
    }

    /// Get a mutable block by ID.
    pub fn block_mut(&mut self, id: BlockId) -> Option<&mut MirBlock> {
        self.blocks.as_mut().and_then(|b| b.get_mut(id.0 as usize))
    }

    /// Add a new block and return its ID.
    pub fn add_block(&mut self, block: MirBlock) -> BlockId {
        if let Some(blocks) = &mut self.blocks {
            let id = BlockId(blocks.len() as u32);
            blocks.push(block);
            id
        } else {
            self.blocks = Some(vec![block]);
            BlockId(0)
        }
    }

    /// Add a local variable and return its ID.
    pub fn add_local(&mut self, local: MirLocal) -> LocalId {
        let id = LocalId(self.locals.len() as u32);
        self.locals.push(local);
        id
    }
}

/// Function signature.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MirFnSig {
    /// Parameter types.
    pub params: Vec<MirType>,
    /// Return type.
    pub ret: MirType,
    /// Is this variadic?
    pub is_variadic: bool,
    /// Calling convention.
    pub calling_conv: CallingConv,
}

impl MirFnSig {
    /// Create a new function signature.
    pub fn new(params: Vec<MirType>, ret: MirType) -> Self {
        Self {
            params,
            ret,
            is_variadic: false,
            calling_conv: CallingConv::Quanta,
        }
    }
}

/// Calling convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CallingConv {
    /// Default QuantaLang calling convention.
    Quanta,
    /// C calling convention.
    C,
    /// Fast calling convention.
    Fast,
    /// Cold calling convention (rarely called).
    Cold,
}

/// Linkage type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Linkage {
    /// Internal (private to the module).
    Internal,
    /// External (visible to other modules).
    External,
    /// Weak (can be overridden).
    Weak,
    /// Link once (merged across modules).
    LinkOnce,
}

// ============================================================================
// BASIC BLOCKS
// ============================================================================

/// A basic block ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub u32);

impl BlockId {
    /// The entry block ID.
    pub const ENTRY: Self = Self(0);
}

impl fmt::Display for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bb{}", self.0)
    }
}

/// A basic block.
#[derive(Debug, Clone)]
pub struct MirBlock {
    /// Block ID.
    pub id: BlockId,
    /// Optional label.
    pub label: Option<Arc<str>>,
    /// Statements in the block.
    pub stmts: Vec<MirStmt>,
    /// Block terminator.
    pub terminator: Option<MirTerminator>,
}

impl MirBlock {
    /// Create a new basic block.
    pub fn new(id: BlockId) -> Self {
        Self {
            id,
            label: None,
            stmts: Vec::new(),
            terminator: None,
        }
    }

    /// Create a labeled block.
    pub fn with_label(id: BlockId, label: impl Into<Arc<str>>) -> Self {
        Self {
            id,
            label: Some(label.into()),
            stmts: Vec::new(),
            terminator: None,
        }
    }

    /// Add a statement.
    pub fn push_stmt(&mut self, stmt: MirStmt) {
        self.stmts.push(stmt);
    }

    /// Set the terminator.
    pub fn set_terminator(&mut self, term: MirTerminator) {
        self.terminator = Some(term);
    }
}

// ============================================================================
// LOCALS AND VALUES
// ============================================================================

/// A local variable ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalId(pub u32);

impl fmt::Display for LocalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "_{}", self.0)
    }
}

/// A local variable declaration.
#[derive(Debug, Clone)]
pub struct MirLocal {
    /// Local ID.
    pub id: LocalId,
    /// Optional name (for debugging).
    pub name: Option<Arc<str>>,
    /// Type.
    pub ty: MirType,
    /// Is this mutable?
    pub is_mut: bool,
    /// Is this a function parameter?
    pub is_param: bool,
    /// Type annotations (e.g., "ColorSpace:Linear", "Precision:Half").
    /// Preserved from the source type system for shader output.
    pub annotations: Vec<Arc<str>>,
}

impl MirLocal {
    /// Create a new local.
    pub fn new(id: LocalId, ty: MirType) -> Self {
        Self {
            id,
            name: None,
            ty,
            is_mut: false,
            is_param: false,
            annotations: Vec::new(),
        }
    }

    /// Create a named local.
    pub fn named(id: LocalId, name: impl Into<Arc<str>>, ty: MirType) -> Self {
        Self {
            id,
            name: Some(name.into()),
            ty,
            is_mut: false,
            is_param: false,
            annotations: Vec::new(),
        }
    }
}

/// A value (operand).
#[derive(Debug, Clone)]
pub enum MirValue {
    /// Reference to a local.
    Local(LocalId),
    /// Constant value.
    Const(MirConst),
    /// Global reference.
    Global(Arc<str>),
    /// Function reference.
    Function(Arc<str>),
}

impl fmt::Display for MirValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MirValue::Local(id) => write!(f, "{}", id),
            MirValue::Const(c) => write!(f, "{}", c),
            MirValue::Global(name) => write!(f, "@{}", name),
            MirValue::Function(name) => write!(f, "@{}", name),
        }
    }
}

// ============================================================================
// STATEMENTS
// ============================================================================

/// A MIR statement.
#[derive(Debug, Clone)]
pub struct MirStmt {
    /// The kind of statement.
    pub kind: MirStmtKind,
}

impl MirStmt {
    /// Create a new statement.
    pub fn new(kind: MirStmtKind) -> Self {
        Self { kind }
    }

    /// Create an assignment statement.
    pub fn assign(dest: LocalId, value: MirRValue) -> Self {
        Self::new(MirStmtKind::Assign { dest, value })
    }

    /// Create a storage_live statement.
    pub fn storage_live(local: LocalId) -> Self {
        Self::new(MirStmtKind::StorageLive(local))
    }

    /// Create a storage_dead statement.
    pub fn storage_dead(local: LocalId) -> Self {
        Self::new(MirStmtKind::StorageDead(local))
    }

    /// Create a no-op statement.
    pub fn nop() -> Self {
        Self::new(MirStmtKind::Nop)
    }
}

/// Kind of MIR statement.
#[derive(Debug, Clone)]
pub enum MirStmtKind {
    /// Assignment: `local = rvalue`
    Assign {
        dest: LocalId,
        value: MirRValue,
    },

    /// Store through a pointer: `*ptr = value`
    DerefAssign {
        ptr: LocalId,
        value: MirRValue,
    },

    /// Store to a field through a pointer: `ptr->field = value`
    FieldDerefAssign {
        ptr: LocalId,
        field_name: Arc<str>,
        value: MirRValue,
    },

    /// Store to a field on a local struct: `local.field = value`
    FieldAssign {
        base: LocalId,
        field_name: Arc<str>,
        value: MirRValue,
    },

    /// Storage live (local becomes valid).
    StorageLive(LocalId),

    /// Storage dead (local becomes invalid).
    StorageDead(LocalId),

    /// No-op (placeholder).
    Nop,
}

/// A right-hand value (rvalue).
#[derive(Debug, Clone)]
pub enum MirRValue {
    /// Use a value directly.
    Use(MirValue),

    /// Binary operation.
    BinaryOp {
        op: BinOp,
        left: MirValue,
        right: MirValue,
    },

    /// Unary operation.
    UnaryOp {
        op: UnaryOp,
        operand: MirValue,
    },

    /// Create a reference.
    Ref {
        is_mut: bool,
        place: MirPlace,
    },

    /// Address of.
    AddressOf {
        is_mut: bool,
        place: MirPlace,
    },

    /// Cast.
    Cast {
        kind: CastKind,
        value: MirValue,
        ty: MirType,
    },

    /// Aggregate (tuple, struct, array).
    Aggregate {
        kind: AggregateKind,
        operands: Vec<MirValue>,
    },

    /// Array repeat: `[value; count]`
    Repeat {
        value: MirValue,
        count: u64,
    },

    /// Discriminant read (for enums).
    Discriminant(MirPlace),

    /// Length of slice/array.
    Len(MirPlace),

    /// Null check.
    NullaryOp(NullaryOp, MirType),

    /// Struct field access: `base.field_name`
    FieldAccess {
        base: MirValue,
        field_name: Arc<str>,
        field_ty: MirType,
    },

    /// Enum variant field access: `base.data.VariantName._N`
    VariantField {
        base: MirValue,
        variant_name: Arc<str>,
        field_index: u32,
        field_ty: MirType,
    },

    /// Array index access: `base[index]`
    IndexAccess {
        base: MirValue,
        index: MirValue,
        elem_ty: MirType,
    },

    /// Dereference a pointer: `*ptr`
    Deref {
        ptr: MirValue,
        pointee_ty: MirType,
    },

    /// Sample a texture at given coordinates: `texture_sample(tex, sampler, uv)`
    TextureSample {
        texture: MirValue,
        sampler: MirValue,
        coords: MirValue,
    },
}

/// A place (lvalue).
#[derive(Debug, Clone)]
pub struct MirPlace {
    /// The base local.
    pub local: LocalId,
    /// Projections (field access, indexing, etc.).
    pub projections: Vec<PlaceProjection>,
}

impl MirPlace {
    /// Create a simple place from a local.
    pub fn local(id: LocalId) -> Self {
        Self {
            local: id,
            projections: Vec::new(),
        }
    }

    /// Add a field projection.
    pub fn field(mut self, idx: u32, ty: MirType) -> Self {
        self.projections.push(PlaceProjection::Field(idx, ty));
        self
    }

    /// Add an index projection.
    pub fn index(mut self, idx: LocalId) -> Self {
        self.projections.push(PlaceProjection::Index(idx));
        self
    }

    /// Add a deref projection.
    pub fn deref(mut self) -> Self {
        self.projections.push(PlaceProjection::Deref);
        self
    }
}

/// Place projection.
#[derive(Debug, Clone)]
pub enum PlaceProjection {
    /// Dereference.
    Deref,
    /// Field access.
    Field(u32, MirType),
    /// Array/slice index.
    Index(LocalId),
    /// Constant index.
    ConstantIndex {
        offset: u64,
        from_end: bool,
    },
    /// Subslice.
    Subslice {
        from: u64,
        to: u64,
        from_end: bool,
    },
    /// Downcast (enum variant).
    Downcast(u32),
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    // Arithmetic
    Add, Sub, Mul, Div, Rem,
    /// Exponentiation (base ** exponent)
    Pow,
    // Bitwise
    BitAnd, BitOr, BitXor, Shl, Shr,
    // Comparison
    Eq, Ne, Lt, Le, Gt, Ge,
    // Checked arithmetic (returns (result, overflow))
    AddChecked, SubChecked, MulChecked,
    // Wrapping arithmetic
    AddWrapping, SubWrapping, MulWrapping,
    // Saturating arithmetic
    AddSaturating, SubSaturating,
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// Logical/bitwise not.
    Not,
    /// Arithmetic negation.
    Neg,
}

/// Cast kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastKind {
    /// Integer to integer.
    IntToInt,
    /// Float to float.
    FloatToFloat,
    /// Integer to float.
    IntToFloat,
    /// Float to integer.
    FloatToInt,
    /// Pointer to integer.
    PtrToInt,
    /// Integer to pointer.
    IntToPtr,
    /// Pointer to pointer.
    PtrToPtr,
    /// Function to pointer.
    FnToPtr,
    /// Transmute (reinterpret bits).
    Transmute,
}

/// Aggregate kinds.
#[derive(Debug, Clone)]
pub enum AggregateKind {
    /// Array.
    Array(MirType),
    /// Tuple.
    Tuple,
    /// Struct.
    Struct(Arc<str>),
    /// Enum variant: (enum_name, discriminant, variant_name).
    Variant(Arc<str>, u32, Arc<str>),
    /// Closure.
    Closure(Arc<str>),
}

/// Nullary operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NullaryOp {
    /// Size of type.
    SizeOf,
    /// Alignment of type.
    AlignOf,
}

// ============================================================================
// TERMINATORS
// ============================================================================

/// Block terminator.
#[derive(Debug, Clone)]
pub enum MirTerminator {
    /// Unconditional goto.
    Goto(BlockId),

    /// Conditional branch.
    If {
        cond: MirValue,
        then_block: BlockId,
        else_block: BlockId,
    },

    /// Multi-way branch (switch).
    Switch {
        value: MirValue,
        targets: Vec<(MirConst, BlockId)>,
        default: BlockId,
    },

    /// Function call.
    Call {
        func: MirValue,
        args: Vec<MirValue>,
        dest: Option<LocalId>,
        target: Option<BlockId>,
        unwind: Option<BlockId>,
    },

    /// Return from function.
    Return(Option<MirValue>),

    /// Unreachable code.
    Unreachable,

    /// Drop a value.
    Drop {
        place: MirPlace,
        target: BlockId,
        unwind: Option<BlockId>,
    },

    /// Assert condition.
    Assert {
        cond: MirValue,
        expected: bool,
        msg: Arc<str>,
        target: BlockId,
        unwind: Option<BlockId>,
    },

    /// Resume unwinding.
    Resume,

    /// Abort execution.
    Abort,
}

// ============================================================================
// TYPES
// ============================================================================

/// MIR type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MirType {
    /// Void (unit).
    Void,
    /// Boolean.
    Bool,
    /// Integer.
    Int(IntSize, bool), // (size, signed)
    /// Float.
    Float(FloatSize),
    /// Pointer.
    Ptr(Box<MirType>),
    /// Array.
    Array(Box<MirType>, u64),
    /// Slice (fat pointer).
    Slice(Box<MirType>),
    /// Struct.
    Struct(Arc<str>),
    /// Function pointer.
    FnPtr(Box<MirFnSig>),
    /// Never type.
    Never,
    /// SIMD Vector type (element type, lane count).
    Vector(Box<MirType>, u32),
    /// 2D Texture (sampled image) — element type is the texel format (e.g. f32 for RGBA float).
    Texture2D(Box<MirType>),
    /// Opaque sampler type (for texture sampling).
    Sampler,
    /// Combined sampled image type (texture + sampler).
    SampledImage(Box<MirType>),
    /// Trait object: fat pointer (data_ptr: *void, vtable_ptr: *VTable).
    /// The Arc<str> is the trait name.
    TraitObject(Arc<str>),
    /// Dynamic array Vec<T>: heap-allocated handle wrapping QuantaVec.
    Vec(Box<MirType>),
    /// HashMap<K, V>: heap-allocated handle wrapping QuantaHashMap.
    Map(Box<MirType>, Box<MirType>),
    /// Tuple type: (T0, T1, ...).
    Tuple(Vec<MirType>),
}

impl MirType {
    /// Create an i8 type.
    pub fn i8() -> Self { MirType::Int(IntSize::I8, true) }
    /// Create an i16 type.
    pub fn i16() -> Self { MirType::Int(IntSize::I16, true) }
    /// Create an i32 type.
    pub fn i32() -> Self { MirType::Int(IntSize::I32, true) }
    /// Create an i64 type.
    pub fn i64() -> Self { MirType::Int(IntSize::I64, true) }
    /// Create a u8 type.
    pub fn u8() -> Self { MirType::Int(IntSize::I8, false) }
    /// Create a u16 type.
    pub fn u16() -> Self { MirType::Int(IntSize::I16, false) }
    /// Create a u32 type.
    pub fn u32() -> Self { MirType::Int(IntSize::I32, false) }
    /// Create a u64 type.
    pub fn u64() -> Self { MirType::Int(IntSize::I64, false) }
    /// Create an isize type.
    pub fn isize() -> Self { MirType::Int(IntSize::ISize, true) }
    /// Create a usize type.
    pub fn usize() -> Self { MirType::Int(IntSize::ISize, false) }
    /// Create an f32 type.
    pub fn f32() -> Self { MirType::Float(FloatSize::F32) }
    /// Create an f64 type.
    pub fn f64() -> Self { MirType::Float(FloatSize::F64) }

    /// Create a vector type.
    pub fn vector(elem: MirType, lanes: u32) -> Self {
        MirType::Vector(Box::new(elem), lanes)
    }

    /// Create a 128-bit vector of 4 f32s (for SSE/NEON).
    pub fn v4f32() -> Self { MirType::vector(MirType::f32(), 4) }
    /// Create a 256-bit vector of 8 f32s (for AVX).
    pub fn v8f32() -> Self { MirType::vector(MirType::f32(), 8) }
    /// Create a 128-bit vector of 2 f64s.
    pub fn v2f64() -> Self { MirType::vector(MirType::f64(), 2) }
    /// Create a 256-bit vector of 4 f64s.
    pub fn v4f64() -> Self { MirType::vector(MirType::f64(), 4) }
    /// Create a 128-bit vector of 4 i32s.
    pub fn v4i32() -> Self { MirType::vector(MirType::i32(), 4) }
    /// Create a 256-bit vector of 8 i32s.
    pub fn v8i32() -> Self { MirType::vector(MirType::i32(), 8) }
    /// Create a 128-bit vector of 16 i8s.
    pub fn v16i8() -> Self { MirType::vector(MirType::i8(), 16) }
    /// Create a 256-bit vector of 32 i8s.
    pub fn v32i8() -> Self { MirType::vector(MirType::i8(), 32) }

    /// Create a 2D texture type with the given element type.
    pub fn texture2d(elem: MirType) -> Self { MirType::Texture2D(Box::new(elem)) }
    /// Create a sampler type.
    pub fn sampler() -> Self { MirType::Sampler }
    /// Create a combined sampled image type.
    pub fn sampled_image(elem: MirType) -> Self { MirType::SampledImage(Box::new(elem)) }

    /// Create a tuple type.
    pub fn tuple(elems: Vec<MirType>) -> Self { MirType::Tuple(elems) }

    /// Generate the canonical C typedef name for a tuple type.
    pub fn tuple_type_name(elems: &[MirType]) -> Arc<str> {
        let parts: Vec<&str> = elems.iter().map(|t| match t {
            MirType::Void => "void",
            MirType::Bool => "bool",
            MirType::Int(IntSize::I8, true) => "i8",
            MirType::Int(IntSize::I16, true) => "i16",
            MirType::Int(IntSize::I32, true) => "i32",
            MirType::Int(IntSize::I64, true) => "i64",
            MirType::Int(IntSize::I128, true) => "i128",
            MirType::Int(IntSize::ISize, true) => "isize",
            MirType::Int(IntSize::I8, false) => "u8",
            MirType::Int(IntSize::I16, false) => "u16",
            MirType::Int(IntSize::I32, false) => "u32",
            MirType::Int(IntSize::I64, false) => "u64",
            MirType::Int(IntSize::I128, false) => "u128",
            MirType::Int(IntSize::ISize, false) => "usize",
            MirType::Float(FloatSize::F32) => "f32",
            MirType::Float(FloatSize::F64) => "f64",
            MirType::Ptr(_) => "ptr",
            MirType::Struct(name) => name.as_ref(),
            _ => "unknown",
        }).collect();
        Arc::from(format!("Tuple_{}", parts.join("_")))
    }

    /// Check if this is a signed integer.
    pub fn is_signed(&self) -> bool {
        matches!(self, MirType::Int(_, true))
    }

    /// Check if this is an integer type.
    pub fn is_integer(&self) -> bool {
        matches!(self, MirType::Int(_, _))
    }

    /// Check if this is a float type.
    pub fn is_float(&self) -> bool {
        matches!(self, MirType::Float(_))
    }

    /// Check if this is a pointer type.
    pub fn is_pointer(&self) -> bool {
        matches!(self, MirType::Ptr(_))
    }

    /// Get the size in bits.
    pub fn bit_size(&self, ptr_size: u32) -> Option<u32> {
        match self {
            MirType::Void => Some(0),
            MirType::Bool => Some(8),
            MirType::Int(size, _) => Some(size.bits(ptr_size)),
            MirType::Float(size) => Some(size.bits()),
            MirType::Ptr(_) => Some(ptr_size),
            MirType::Array(elem, count) => {
                elem.bit_size(ptr_size).map(|s| s * (*count as u32))
            }
            MirType::Slice(_) => Some(ptr_size * 2), // ptr + len
            MirType::Struct(_) => None, // Need type info
            MirType::FnPtr(_) => Some(ptr_size),
            MirType::Never => Some(0),
            MirType::Vector(elem, lanes) => {
                elem.bit_size(ptr_size).map(|s| s * lanes)
            }
            MirType::Texture2D(_) | MirType::Sampler | MirType::SampledImage(_) => {
                None // Opaque GPU types — no CPU bit size
            }
            MirType::TraitObject(_) => Some(ptr_size * 2), // Fat pointer: data + vtable
            MirType::Vec(_) => Some(ptr_size), // QuantaVecHandle is a pointer
            MirType::Map(_, _) => Some(ptr_size), // QuantaMapHandle is a pointer
            MirType::Tuple(elems) => {
                let mut total = 0u32;
                for e in elems {
                    total += e.bit_size(ptr_size)?;
                }
                Some(total)
            }
        }
    }
}

impl fmt::Display for MirType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MirType::Void => write!(f, "void"),
            MirType::Bool => write!(f, "bool"),
            MirType::Int(size, signed) => {
                let prefix = if *signed { "i" } else { "u" };
                match size {
                    IntSize::I8 => write!(f, "{}8", prefix),
                    IntSize::I16 => write!(f, "{}16", prefix),
                    IntSize::I32 => write!(f, "{}32", prefix),
                    IntSize::I64 => write!(f, "{}64", prefix),
                    IntSize::I128 => write!(f, "{}128", prefix),
                    IntSize::ISize => write!(f, "{}size", prefix),
                }
            }
            MirType::Float(size) => match size {
                FloatSize::F32 => write!(f, "f32"),
                FloatSize::F64 => write!(f, "f64"),
            },
            MirType::Ptr(inner) => write!(f, "*{}", inner),
            MirType::Array(elem, len) => write!(f, "[{}; {}]", elem, len),
            MirType::Slice(elem) => write!(f, "[{}]", elem),
            MirType::Struct(name) => write!(f, "{}", name),
            MirType::FnPtr(sig) => {
                write!(f, "fn(")?;
                for (i, p) in sig.params.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", p)?;
                }
                write!(f, ") -> {}", sig.ret)
            }
            MirType::Never => write!(f, "!"),
            MirType::Vector(elem, lanes) => write!(f, "<{} x {}>", lanes, elem),
            MirType::Texture2D(elem) => write!(f, "texture2d<{}>", elem),
            MirType::Sampler => write!(f, "sampler"),
            MirType::SampledImage(elem) => write!(f, "sampled_image<{}>", elem),
            MirType::TraitObject(name) => write!(f, "dyn {}", name),
            MirType::Vec(elem) => write!(f, "Vec<{}>", elem),
            MirType::Map(key, val) => write!(f, "HashMap<{}, {}>", key, val),
            MirType::Tuple(elems) => {
                write!(f, "(")?;
                for (i, e) in elems.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", e)?;
                }
                write!(f, ")")
            }
        }
    }
}

/// Integer sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntSize {
    I8, I16, I32, I64, I128, ISize,
}

impl IntSize {
    /// Get size in bits.
    pub fn bits(&self, ptr_size: u32) -> u32 {
        match self {
            IntSize::I8 => 8,
            IntSize::I16 => 16,
            IntSize::I32 => 32,
            IntSize::I64 => 64,
            IntSize::I128 => 128,
            IntSize::ISize => ptr_size,
        }
    }
}

/// Float sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FloatSize {
    F32, F64,
}

impl FloatSize {
    /// Get size in bits.
    pub fn bits(&self) -> u32 {
        match self {
            FloatSize::F32 => 32,
            FloatSize::F64 => 64,
        }
    }
}

// ============================================================================
// CONSTANTS
// ============================================================================

/// MIR constant.
#[derive(Debug, Clone)]
pub enum MirConst {
    /// Boolean.
    Bool(bool),
    /// Integer.
    Int(i128, MirType),
    /// Unsigned integer.
    Uint(u128, MirType),
    /// Float.
    Float(f64, MirType),
    /// String (index into string table).
    Str(u32),
    /// Byte string.
    ByteStr(Vec<u8>),
    /// Null pointer.
    Null(MirType),
    /// Unit value.
    Unit,
    /// Zero-initialized value.
    Zeroed(MirType),
    /// Undefined value.
    Undef(MirType),
    /// Struct constant: (struct_name, field_values).
    Struct(Arc<str>, Vec<MirConst>),
}

impl fmt::Display for MirConst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MirConst::Bool(b) => write!(f, "{}", b),
            MirConst::Int(v, ty) => write!(f, "{}{}", v, ty),
            MirConst::Uint(v, ty) => write!(f, "{}{}", v, ty),
            MirConst::Float(v, ty) => write!(f, "{}{}", v, ty),
            MirConst::Str(idx) => write!(f, "str#{}", idx),
            MirConst::ByteStr(bytes) => write!(f, "b\"{}\"", bytes.len()),
            MirConst::Null(ty) => write!(f, "null:{}", ty),
            MirConst::Unit => write!(f, "()"),
            MirConst::Zeroed(ty) => write!(f, "zeroed:{}", ty),
            MirConst::Undef(ty) => write!(f, "undef:{}", ty),
            MirConst::Struct(name, fields) => {
                write!(f, "{}{{", name)?;
                for (i, fv) in fields.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", fv)?;
                }
                write!(f, "}}")
            }
        }
    }
}

// ============================================================================
// GLOBALS
// ============================================================================

/// Global variable.
#[derive(Debug, Clone)]
pub struct MirGlobal {
    /// Name.
    pub name: Arc<str>,
    /// Type.
    pub ty: MirType,
    /// Initial value.
    pub init: Option<MirConst>,
    /// Is mutable?
    pub is_mut: bool,
    /// Linkage.
    pub linkage: Linkage,
}

impl MirGlobal {
    /// Create a new global.
    pub fn new(name: impl Into<Arc<str>>, ty: MirType) -> Self {
        Self {
            name: name.into(),
            ty,
            init: None,
            is_mut: false,
            linkage: Linkage::Internal,
        }
    }
}

/// External declaration.
#[derive(Debug, Clone)]
pub struct MirExternal {
    /// Name.
    pub name: Arc<str>,
    /// Kind.
    pub kind: ExternalKind,
}

/// Kind of external.
#[derive(Debug, Clone)]
pub enum ExternalKind {
    /// External function.
    Function(MirFnSig),
    /// External global.
    Global(MirType),
}

// ============================================================================
// TYPE DEFINITIONS
// ============================================================================

/// Type definition.
#[derive(Debug, Clone)]
pub struct MirTypeDef {
    /// Name.
    pub name: Arc<str>,
    /// Kind.
    pub kind: TypeDefKind,
}

/// Kind of type definition.
#[derive(Debug, Clone)]
pub enum TypeDefKind {
    /// Struct.
    Struct {
        fields: Vec<(Option<Arc<str>>, MirType)>,
        packed: bool,
    },
    /// Union.
    Union {
        variants: Vec<(Arc<str>, MirType)>,
    },
    /// Enum.
    Enum {
        discriminant_ty: MirType,
        variants: Vec<MirEnumVariant>,
    },
}

/// Enum variant.
#[derive(Debug, Clone)]
pub struct MirEnumVariant {
    /// Variant name.
    pub name: Arc<str>,
    /// Discriminant value.
    pub discriminant: i128,
    /// Fields.
    pub fields: Vec<(Option<Arc<str>>, MirType)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // MIR TYPE TESTS
    // =========================================================================

    #[test]
    fn test_mir_type_display() {
        assert_eq!(format!("{}", MirType::i32()), "i32");
        assert_eq!(format!("{}", MirType::u64()), "u64");
        assert_eq!(format!("{}", MirType::f32()), "f32");
        assert_eq!(format!("{}", MirType::Bool), "bool");
        assert_eq!(format!("{}", MirType::Ptr(Box::new(MirType::i32()))), "*i32");
        assert_eq!(format!("{}", MirType::Void), "void");
        assert_eq!(format!("{}", MirType::Never), "!");
    }

    #[test]
    fn test_mir_type_constructors() {
        assert_eq!(MirType::i8(), MirType::Int(IntSize::I8, true));
        assert_eq!(MirType::i16(), MirType::Int(IntSize::I16, true));
        assert_eq!(MirType::i32(), MirType::Int(IntSize::I32, true));
        assert_eq!(MirType::i64(), MirType::Int(IntSize::I64, true));
        assert_eq!(MirType::u8(), MirType::Int(IntSize::I8, false));
        assert_eq!(MirType::u16(), MirType::Int(IntSize::I16, false));
        assert_eq!(MirType::u32(), MirType::Int(IntSize::I32, false));
        assert_eq!(MirType::u64(), MirType::Int(IntSize::I64, false));
        assert_eq!(MirType::isize(), MirType::Int(IntSize::ISize, true));
        assert_eq!(MirType::usize(), MirType::Int(IntSize::ISize, false));
        assert_eq!(MirType::f32(), MirType::Float(FloatSize::F32));
        assert_eq!(MirType::f64(), MirType::Float(FloatSize::F64));
    }

    #[test]
    fn test_mir_type_predicates() {
        assert!(MirType::i32().is_integer());
        assert!(MirType::u64().is_integer());
        assert!(!MirType::f32().is_integer());
        assert!(!MirType::Bool.is_integer());

        assert!(MirType::f32().is_float());
        assert!(MirType::f64().is_float());
        assert!(!MirType::i32().is_float());

        assert!(MirType::i32().is_signed());
        assert!(!MirType::u32().is_signed());

        assert!(MirType::Ptr(Box::new(MirType::i32())).is_pointer());
        assert!(!MirType::i32().is_pointer());
    }

    #[test]
    fn test_mir_type_bit_size() {
        let ptr_size = 64;
        assert_eq!(MirType::Void.bit_size(ptr_size), Some(0));
        assert_eq!(MirType::Bool.bit_size(ptr_size), Some(8));
        assert_eq!(MirType::i8().bit_size(ptr_size), Some(8));
        assert_eq!(MirType::i16().bit_size(ptr_size), Some(16));
        assert_eq!(MirType::i32().bit_size(ptr_size), Some(32));
        assert_eq!(MirType::i64().bit_size(ptr_size), Some(64));
        assert_eq!(MirType::Int(IntSize::I128, true).bit_size(ptr_size), Some(128));
        assert_eq!(MirType::f32().bit_size(ptr_size), Some(32));
        assert_eq!(MirType::f64().bit_size(ptr_size), Some(64));
        assert_eq!(MirType::Ptr(Box::new(MirType::i32())).bit_size(ptr_size), Some(64));
        assert_eq!(MirType::isize().bit_size(ptr_size), Some(64));
    }

    #[test]
    fn test_int_size_bits() {
        assert_eq!(IntSize::I8.bits(64), 8);
        assert_eq!(IntSize::I16.bits(64), 16);
        assert_eq!(IntSize::I32.bits(64), 32);
        assert_eq!(IntSize::I64.bits(64), 64);
        assert_eq!(IntSize::I128.bits(64), 128);
        assert_eq!(IntSize::ISize.bits(64), 64);
        assert_eq!(IntSize::ISize.bits(32), 32);
    }

    #[test]
    fn test_float_size_bits() {
        assert_eq!(FloatSize::F32.bits(), 32);
        assert_eq!(FloatSize::F64.bits(), 64);
    }

    // =========================================================================
    // MIR MODULE TESTS
    // =========================================================================

    #[test]
    fn test_mir_module() {
        let mut module = MirModule::new("test");
        let idx = module.intern_string("hello");
        assert_eq!(idx, 0);
        assert_eq!(module.intern_string("hello"), 0); // Same string
        assert_eq!(module.intern_string("world"), 1); // Different string
    }

    #[test]
    fn test_mir_module_add_function() {
        let mut module = MirModule::new("test");
        let sig = MirFnSig::new(vec![MirType::i32()], MirType::i32());
        let func = MirFunction::new("my_func", sig);
        module.add_function(func);

        assert_eq!(module.functions.len(), 1);
        assert_eq!(module.functions[0].name.as_ref(), "my_func");
    }

    #[test]
    fn test_mir_module_add_global() {
        let mut module = MirModule::new("test");
        let global = MirGlobal::new("my_global", MirType::i32());
        module.add_global(global);

        assert_eq!(module.globals.len(), 1);
        assert_eq!(module.globals[0].name.as_ref(), "my_global");
    }

    // =========================================================================
    // MIR FUNCTION TESTS
    // =========================================================================

    #[test]
    fn test_mir_function_new() {
        let sig = MirFnSig::new(vec![MirType::i32(), MirType::i64()], MirType::Bool);
        let func = MirFunction::new("test_func", sig);

        assert_eq!(func.name.as_ref(), "test_func");
        assert_eq!(func.sig.params.len(), 2);
        assert_eq!(func.sig.ret, MirType::Bool);
        assert!(func.blocks.is_none());
    }

    #[test]
    fn test_mir_function_is_declaration() {
        let sig = MirFnSig::new(vec![], MirType::Void);
        let mut func = MirFunction::new("decl_func", sig);

        assert!(func.is_declaration());

        func.add_block(MirBlock::new(BlockId::ENTRY));
        assert!(!func.is_declaration());
    }

    #[test]
    fn test_mir_function_add_block() {
        let sig = MirFnSig::new(vec![], MirType::Void);
        let mut func = MirFunction::new("test", sig);

        func.add_block(MirBlock::new(BlockId(0)));
        func.add_block(MirBlock::new(BlockId(1)));

        assert_eq!(func.blocks.as_ref().unwrap().len(), 2);
    }

    // =========================================================================
    // MIR BLOCK TESTS
    // =========================================================================

    #[test]
    fn test_mir_block_new() {
        let block = MirBlock::new(BlockId(5));
        assert_eq!(block.id.0, 5);
        assert!(block.label.is_none());
        assert!(block.stmts.is_empty());
        assert!(block.terminator.is_none());
    }

    #[test]
    fn test_mir_block_with_label() {
        let block = MirBlock::with_label(BlockId(0), "entry");
        assert_eq!(block.label.as_ref().unwrap().as_ref(), "entry");
    }

    #[test]
    fn test_mir_block_push_stmt() {
        let mut block = MirBlock::new(BlockId(0));
        block.push_stmt(MirStmt::nop());
        block.push_stmt(MirStmt::nop());

        assert_eq!(block.stmts.len(), 2);
    }

    #[test]
    fn test_mir_block_set_terminator() {
        let mut block = MirBlock::new(BlockId(0));
        assert!(block.terminator.is_none());

        block.set_terminator(MirTerminator::Return(None));
        assert!(block.terminator.is_some());
    }

    // =========================================================================
    // MIR STATEMENT TESTS
    // =========================================================================

    #[test]
    fn test_mir_stmt_assign() {
        let stmt = MirStmt::assign(LocalId(0), MirRValue::Use(MirValue::Const(MirConst::Bool(true))));
        match stmt.kind {
            MirStmtKind::Assign { dest, .. } => assert_eq!(dest.0, 0),
            _ => panic!("Expected Assign"),
        }
    }

    #[test]
    fn test_mir_stmt_nop() {
        let stmt = MirStmt::nop();
        assert!(matches!(stmt.kind, MirStmtKind::Nop));
    }

    // =========================================================================
    // MIR VALUE TESTS
    // =========================================================================

    #[test]
    fn test_mir_value_local() {
        let val = MirValue::Local(LocalId(42));
        match val {
            MirValue::Local(id) => assert_eq!(id.0, 42),
            _ => panic!("Expected Local"),
        }
    }

    #[test]
    fn test_mir_value_const_int() {
        let val = MirValue::Const(MirConst::Int(123, MirType::i32()));
        match val {
            MirValue::Const(MirConst::Int(v, ty)) => {
                assert_eq!(v, 123);
                assert_eq!(ty, MirType::i32());
            }
            _ => panic!("Expected Const Int"),
        }
    }

    // =========================================================================
    // MIR CONST TESTS
    // =========================================================================

    #[test]
    fn test_mir_const_display() {
        assert_eq!(format!("{}", MirConst::Bool(true)), "true");
        assert_eq!(format!("{}", MirConst::Bool(false)), "false");
        assert_eq!(format!("{}", MirConst::Int(42, MirType::i32())), "42i32");
        assert_eq!(format!("{}", MirConst::Uint(100, MirType::u64())), "100u64");
        assert_eq!(format!("{}", MirConst::Str(0)), "str#0");
        assert_eq!(format!("{}", MirConst::Unit), "()");
        assert_eq!(
            format!("{}", MirConst::Struct(
                Arc::from("Color"),
                vec![
                    MirConst::Float(1.0, MirType::f64()),
                    MirConst::Float(0.5, MirType::f64()),
                    MirConst::Float(0.0, MirType::f64()),
                ],
            )),
            "Color{1f64, 0.5f64, 0f64}",
        );
    }

    // =========================================================================
    // MIR TERMINATOR TESTS
    // =========================================================================

    #[test]
    fn test_mir_terminator_return() {
        let term = MirTerminator::Return(Some(MirValue::Const(MirConst::Int(0, MirType::i32()))));
        match term {
            MirTerminator::Return(Some(_)) => {}
            _ => panic!("Expected Return with value"),
        }
    }

    #[test]
    fn test_mir_terminator_goto() {
        let term = MirTerminator::Goto(BlockId(5));
        match term {
            MirTerminator::Goto(id) => assert_eq!(id.0, 5),
            _ => panic!("Expected Goto"),
        }
    }

    #[test]
    fn test_mir_terminator_if() {
        let term = MirTerminator::If {
            cond: MirValue::Const(MirConst::Bool(true)),
            then_block: BlockId(1),
            else_block: BlockId(2),
        };
        match term {
            MirTerminator::If { then_block, else_block, .. } => {
                assert_eq!(then_block.0, 1);
                assert_eq!(else_block.0, 2);
            }
            _ => panic!("Expected If"),
        }
    }

    // =========================================================================
    // MIR RVALUE TESTS
    // =========================================================================

    #[test]
    fn test_mir_rvalue_binary_op() {
        let rvalue = MirRValue::BinaryOp {
            op: BinOp::Add,
            left: MirValue::Local(LocalId(0)),
            right: MirValue::Local(LocalId(1)),
        };
        match rvalue {
            MirRValue::BinaryOp { op, .. } => assert_eq!(op, BinOp::Add),
            _ => panic!("Expected BinaryOp"),
        }
    }

    #[test]
    fn test_mir_rvalue_unary_op() {
        let rvalue = MirRValue::UnaryOp {
            op: UnaryOp::Neg,
            operand: MirValue::Local(LocalId(0)),
        };
        match rvalue {
            MirRValue::UnaryOp { op, .. } => assert_eq!(op, UnaryOp::Neg),
            _ => panic!("Expected UnaryOp"),
        }
    }

    // =========================================================================
    // MIR GLOBAL TESTS
    // =========================================================================

    #[test]
    fn test_mir_global_new() {
        let global = MirGlobal::new("my_var", MirType::i64());
        assert_eq!(global.name.as_ref(), "my_var");
        assert_eq!(global.ty, MirType::i64());
        assert!(!global.is_mut);
        assert!(global.init.is_none());
    }

    // =========================================================================
    // MIR FN SIG TESTS
    // =========================================================================

    #[test]
    fn test_mir_fn_sig_new() {
        let sig = MirFnSig::new(
            vec![MirType::i32(), MirType::f64()],
            MirType::Bool,
        );
        assert_eq!(sig.params.len(), 2);
        assert_eq!(sig.params[0], MirType::i32());
        assert_eq!(sig.params[1], MirType::f64());
        assert_eq!(sig.ret, MirType::Bool);
        assert!(!sig.is_variadic);
    }

    #[test]
    fn test_mir_fn_sig_void() {
        let sig = MirFnSig::new(vec![], MirType::Void);
        assert!(sig.params.is_empty());
        assert_eq!(sig.ret, MirType::Void);
    }

    // =========================================================================
    // BLOCK ID TESTS
    // =========================================================================

    #[test]
    fn test_block_id_entry() {
        assert_eq!(BlockId::ENTRY.0, 0);
    }

    // =========================================================================
    // LOCAL ID TESTS
    // =========================================================================

    #[test]
    fn test_local_id() {
        let id = LocalId(10);
        assert_eq!(id.0, 10);
    }
}
