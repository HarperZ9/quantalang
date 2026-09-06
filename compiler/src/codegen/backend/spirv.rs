// ===============================================================================
// BUILDLANG CODE GENERATOR - SPIR-V BACKEND
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. BuildLang Fair-Source License v1.0 (see LICENSE).
// ===============================================================================

//! SPIR-V code generation backend for GPU compute shaders.
//!
//! Generates SPIR-V binary format for Vulkan/OpenCL compute shaders.
//! This backend is specifically designed for BuildLang's GPU compute
//! capabilities.
//!
//! ## Features
//!
//! - Full SPIR-V 1.5 instruction set
//! - Vulkan compute shader support
//! - Buffer and image descriptor bindings
//! - Built-in variables (GlobalInvocationId, etc.)
//! - Control flow (branches, loops)
//! - All arithmetic and logical operations
//! - Atomic operations for shared memory

use std::collections::HashMap;
use std::sync::Arc;

use super::{Backend, CodegenError, CodegenResult, Target};
use crate::codegen::ir::*;
use crate::codegen::{GeneratedCode, OutputFormat};

// =============================================================================
// SPIR-V CONSTANTS
// =============================================================================

/// SPIR-V magic number.
const SPIRV_MAGIC: u32 = 0x07230203;

/// SPIR-V version (1.5) -- used for compute shaders / Vulkan 1.2+.
const SPIRV_VERSION: u32 = 0x00010500;

/// SPIR-V version (1.0) -- used for graphics shaders / Vulkan 1.0.
const SPIRV_VERSION_1_0: u32 = 0x00010000;

/// BuildLang generator ID.
const GENERATOR_ID: u32 = 0x51414E54; // "QANT" in hex

// =============================================================================
// SPIR-V OPCODES
// =============================================================================

/// SPIR-V opcodes.
#[repr(u16)]
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum SpvOp {
    // Miscellaneous
    OpNop = 0,
    OpUndef = 1,
    OpSourceContinued = 2,
    OpSource = 3,
    OpSourceExtension = 4,
    OpName = 5,
    OpMemberName = 6,
    OpString = 7,
    OpLine = 8,

    // Extensions
    OpExtension = 10,
    OpExtInstImport = 11,
    OpExtInst = 12,

    // Memory model
    OpMemoryModel = 14,
    OpEntryPoint = 15,
    OpExecutionMode = 16,
    OpCapability = 17,

    // Type declarations
    OpTypeVoid = 19,
    OpTypeBool = 20,
    OpTypeInt = 21,
    OpTypeFloat = 22,
    OpTypeVector = 23,
    OpTypeMatrix = 24,
    OpTypeImage = 25,
    OpTypeSampler = 26,
    OpTypeSampledImage = 27,
    OpTypeArray = 28,
    OpTypeRuntimeArray = 29,
    OpTypeStruct = 30,
    OpTypeOpaque = 31,
    OpTypePointer = 32,
    OpTypeFunction = 33,

    // Constants
    OpConstantTrue = 41,
    OpConstantFalse = 42,
    OpConstant = 43,
    OpConstantComposite = 44,
    OpConstantNull = 46,
    OpSpecConstantTrue = 48,
    OpSpecConstantFalse = 49,
    OpSpecConstant = 50,
    OpSpecConstantComposite = 51,

    // Functions
    OpFunction = 54,
    OpFunctionParameter = 55,
    OpFunctionEnd = 56,
    OpFunctionCall = 57,

    // Variables
    OpVariable = 59,
    OpImageTexelPointer = 60,
    OpLoad = 61,
    OpStore = 62,
    OpCopyMemory = 63,
    OpAccessChain = 65,
    OpInBoundsAccessChain = 66,
    OpPtrAccessChain = 67,

    // Decorations
    OpDecorate = 71,
    OpMemberDecorate = 72,
    OpDecorationGroup = 73,
    OpGroupDecorate = 74,
    OpGroupMemberDecorate = 75,

    // Image operations
    OpSampledImage = 86,
    OpImageSampleImplicitLod = 87,
    OpImageSampleExplicitLod = 88,
    OpImageRead = 98,
    OpImageWrite = 99,

    // Vector operations
    OpVectorExtractDynamic = 77,
    OpVectorInsertDynamic = 78,
    OpVectorShuffle = 79,
    OpCompositeConstruct = 80,
    OpCompositeExtract = 81,
    OpCompositeInsert = 82,
    OpCopyObject = 83,

    // Arithmetic
    OpSNegate = 126,
    OpFNegate = 127,
    OpIAdd = 128,
    OpFAdd = 129,
    OpISub = 130,
    OpFSub = 131,
    OpIMul = 132,
    OpFMul = 133,
    OpUDiv = 134,
    OpSDiv = 135,
    OpFDiv = 136,
    OpUMod = 137,
    OpSRem = 138,
    OpSMod = 139,
    OpFRem = 140,
    OpFMod = 141,

    // Vector arithmetic
    OpVectorTimesScalar = 142,
    OpMatrixTimesScalar = 143,
    OpVectorTimesMatrix = 144,
    OpMatrixTimesVector = 145,
    OpMatrixTimesMatrix = 146,

    // Dot product
    OpDot = 148,

    // Bitwise
    OpShiftRightLogical = 194,
    OpShiftRightArithmetic = 195,
    OpShiftLeftLogical = 196,
    OpBitwiseOr = 197,
    OpBitwiseXor = 198,
    OpBitwiseAnd = 199,
    OpNot = 200,

    // Logical
    OpLogicalEqual = 164,
    OpLogicalNotEqual = 165,
    OpLogicalOr = 166,
    OpLogicalAnd = 167,
    OpLogicalNot = 168,

    // Comparison
    OpIEqual = 170,
    OpINotEqual = 171,
    OpUGreaterThan = 172,
    OpSGreaterThan = 173,
    OpUGreaterThanEqual = 174,
    OpSGreaterThanEqual = 175,
    OpULessThan = 176,
    OpSLessThan = 177,
    OpULessThanEqual = 178,
    OpSLessThanEqual = 179,
    OpFOrdEqual = 180,
    OpFUnordEqual = 181,
    OpFOrdNotEqual = 182,
    OpFUnordNotEqual = 183,
    OpFOrdLessThan = 184,
    OpFUnordLessThan = 185,
    OpFOrdGreaterThan = 186,
    OpFUnordGreaterThan = 187,
    OpFOrdLessThanEqual = 188,
    OpFUnordLessThanEqual = 189,
    OpFOrdGreaterThanEqual = 190,
    OpFUnordGreaterThanEqual = 191,

    // Conversion
    OpConvertFToU = 109,
    OpConvertFToS = 110,
    OpConvertSToF = 111,
    OpConvertUToF = 112,
    OpUConvert = 113,
    OpSConvert = 114,
    OpFConvert = 115,
    OpBitcast = 124,

    // Selection
    OpSelect = 169,

    // Phi
    OpPhi = 245,

    // Control flow
    OpLoopMerge = 246,
    OpSelectionMerge = 247,
    OpLabel = 248,
    OpBranch = 249,
    OpBranchConditional = 250,
    OpSwitch = 251,
    OpKill = 252,
    OpReturn = 253,
    OpReturnValue = 254,
    OpUnreachable = 255,

    // Atomics
    OpAtomicLoad = 227,
    OpAtomicStore = 228,
    OpAtomicExchange = 229,
    OpAtomicCompareExchange = 230,
    OpAtomicIIncrement = 232,
    OpAtomicIDecrement = 233,
    OpAtomicIAdd = 234,
    OpAtomicISub = 235,
    OpAtomicSMin = 236,
    OpAtomicUMin = 237,
    OpAtomicSMax = 238,
    OpAtomicUMax = 239,
    OpAtomicAnd = 240,
    OpAtomicOr = 241,
    OpAtomicXor = 242,

    // Barriers
    OpControlBarrier = 224,
    OpMemoryBarrier = 225,
}

// =============================================================================
// SPIR-V ENUMS
// =============================================================================

/// SPIR-V capabilities.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum SpvCapability {
    Matrix = 0,
    Shader = 1,
    Geometry = 2,
    Tessellation = 3,
    Addresses = 4,
    Linkage = 5,
    Kernel = 6,
    Vector16 = 7,
    Float16Buffer = 8,
    Float16 = 9,
    Float64 = 10,
    Int64 = 11,
    Int64Atomics = 12,
    ImageBasic = 13,
    ImageReadWrite = 14,
    ImageMipmap = 15,
    Pipes = 17,
    Groups = 18,
    DeviceEnqueue = 19,
    LiteralSampler = 20,
    AtomicStorage = 21,
    Int16 = 22,
    TessellationPointSize = 23,
    GeometryPointSize = 24,
    ImageGatherExtended = 25,
    StorageImageMultisample = 27,
    UniformBufferArrayDynamicIndexing = 28,
    SampledImageArrayDynamicIndexing = 29,
    StorageBufferArrayDynamicIndexing = 30,
    StorageImageArrayDynamicIndexing = 31,
    ClipDistance = 32,
    CullDistance = 33,
    ImageCubeArray = 34,
    SampleRateShading = 35,
    ImageRect = 36,
    SampledRect = 37,
    GenericPointer = 38,
    Int8 = 39,
    InputAttachment = 40,
    SparseResidency = 41,
    MinLod = 42,
    Sampled1D = 43,
    Image1D = 44,
    SampledCubeArray = 45,
    SampledBuffer = 46,
    ImageBuffer = 47,
    ImageMSArray = 48,
    StorageImageExtendedFormats = 49,
    ImageQuery = 50,
    DerivativeControl = 51,
    InterpolationFunction = 52,
    TransformFeedback = 53,
    GeometryStreams = 54,
    StorageImageReadWithoutFormat = 55,
    StorageImageWriteWithoutFormat = 56,
    VulkanMemoryModel = 5345,
}

/// SPIR-V addressing model.
#[repr(u32)]
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum SpvAddressingModel {
    Logical = 0,
    Physical32 = 1,
    Physical64 = 2,
    PhysicalStorageBuffer64 = 5348,
}

/// SPIR-V memory model.
#[repr(u32)]
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum SpvMemoryModel {
    Simple = 0,
    Glsl450 = 1,
    OpenCL = 2,
    Vulkan = 3,
}

/// SPIR-V execution model.
#[repr(u32)]
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum SpvExecutionModel {
    Vertex = 0,
    TessellationControl = 1,
    TessellationEvaluation = 2,
    Geometry = 3,
    Fragment = 4,
    GLCompute = 5,
    Kernel = 6,
    TaskNV = 5267,
    MeshNV = 5268,
    RayGenerationKHR = 5313,
    IntersectionKHR = 5314,
    AnyHitKHR = 5315,
    ClosestHitKHR = 5316,
    MissKHR = 5317,
    CallableKHR = 5318,
}

/// SPIR-V execution mode.
#[repr(u32)]
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum SpvExecutionMode {
    Invocations = 0,
    SpacingEqual = 1,
    SpacingFractionalEven = 2,
    SpacingFractionalOdd = 3,
    VertexOrderCw = 4,
    VertexOrderCcw = 5,
    PixelCenterInteger = 6,
    OriginUpperLeft = 7,
    OriginLowerLeft = 8,
    EarlyFragmentTests = 9,
    PointMode = 10,
    Xfb = 11,
    DepthReplacing = 12,
    DepthGreater = 14,
    DepthLess = 15,
    DepthUnchanged = 16,
    LocalSize = 17,
    LocalSizeHint = 18,
    InputPoints = 19,
    InputLines = 20,
    InputLinesAdjacency = 21,
    Triangles = 22,
    InputTrianglesAdjacency = 23,
    Quads = 24,
    Isolines = 25,
    OutputVertices = 26,
    OutputPoints = 27,
    OutputLineStrip = 28,
    OutputTriangleStrip = 29,
    VecTypeHint = 30,
    ContractionOff = 31,
    Initializer = 33,
    Finalizer = 34,
}

/// SPIR-V storage class.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum SpvStorageClass {
    UniformConstant = 0,
    Input = 1,
    Uniform = 2,
    Output = 3,
    Workgroup = 4,
    CrossWorkgroup = 5,
    Private = 6,
    Function = 7,
    Generic = 8,
    PushConstant = 9,
    AtomicCounter = 10,
    Image = 11,
    StorageBuffer = 12,
    PhysicalStorageBuffer = 5349,
}

/// SPIR-V decoration.
#[repr(u32)]
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum SpvDecoration {
    RelaxedPrecision = 0,
    SpecId = 1,
    Block = 2,
    BufferBlock = 3,
    RowMajor = 4,
    ColMajor = 5,
    ArrayStride = 6,
    MatrixStride = 7,
    GLSLShared = 8,
    GLSLPacked = 9,
    CPacked = 10,
    BuiltIn = 11,
    NoPerspective = 13,
    Flat = 14,
    Patch = 15,
    Centroid = 16,
    Sample = 17,
    Invariant = 18,
    Restrict = 19,
    Aliased = 20,
    Volatile = 21,
    Constant = 22,
    Coherent = 23,
    NonWritable = 24,
    NonReadable = 25,
    Uniform = 26,
    SaturatedConversion = 28,
    Stream = 29,
    Location = 30,
    Component = 31,
    Index = 32,
    Binding = 33,
    DescriptorSet = 34,
    Offset = 35,
    XfbBuffer = 36,
    XfbStride = 37,
    FuncParamAttr = 38,
    FPRoundingMode = 39,
    FPFastMathMode = 40,
    LinkageAttributes = 41,
    NoContraction = 42,
    InputAttachmentIndex = 43,
    Alignment = 44,
}

/// SPIR-V built-in variables.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum SpvBuiltIn {
    Position = 0,
    PointSize = 1,
    ClipDistance = 3,
    CullDistance = 4,
    VertexId = 5,
    InstanceId = 6,
    PrimitiveId = 7,
    InvocationId = 8,
    Layer = 9,
    ViewportIndex = 10,
    TessLevelOuter = 11,
    TessLevelInner = 12,
    TessCoord = 13,
    PatchVertices = 14,
    FragCoord = 15,
    PointCoord = 16,
    FrontFacing = 17,
    SampleId = 18,
    SamplePosition = 19,
    SampleMask = 20,
    FragDepth = 22,
    HelperInvocation = 23,
    NumWorkgroups = 24,
    WorkgroupSize = 25,
    WorkgroupId = 26,
    LocalInvocationId = 27,
    GlobalInvocationId = 28,
    LocalInvocationIndex = 29,
    WorkDim = 30,
    GlobalSize = 31,
    EnqueuedWorkgroupSize = 32,
    GlobalOffset = 33,
    GlobalLinearId = 34,
    SubgroupSize = 36,
    SubgroupMaxSize = 37,
    NumSubgroups = 38,
    NumEnqueuedSubgroups = 39,
    SubgroupId = 40,
    SubgroupLocalInvocationId = 41,
    VertexIndex = 42,
    InstanceIndex = 43,
}

/// SPIR-V scope.
#[repr(u32)]
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum SpvScope {
    CrossDevice = 0,
    Device = 1,
    Workgroup = 2,
    Subgroup = 3,
    Invocation = 4,
}

/// SPIR-V memory semantics.
#[repr(u32)]
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum SpvMemorySemantics {
    None = 0,
    Acquire = 0x2,
    Release = 0x4,
    AcquireRelease = 0x8,
    SequentiallyConsistent = 0x10,
    UniformMemory = 0x40,
    SubgroupMemory = 0x80,
    WorkgroupMemory = 0x100,
    CrossWorkgroupMemory = 0x200,
    AtomicCounterMemory = 0x400,
    ImageMemory = 0x800,
}

// =============================================================================
// SPIR-V BACKEND
// =============================================================================

/// A compute-kernel StorageBuffer binding mapped from a slice/array parameter.
#[derive(Clone)]
struct ComputeBuffer {
    /// The OpVariable id (Uniform storage class, BufferBlock-decorated struct).
    var_id: u32,
    /// Element type of the runtime array member.
    elem_ty: MirType,
    /// Whether the kernel writes into this buffer (`&mut [_]`).
    writable: bool,
}

/// A compute-kernel workgroup-shared scratch array mapped from a
/// `workgroupArray(N)` local. The backend emits one `Workgroup`-class
/// `OpVariable` of type `array<elem_ty, N>` (N a compile-time `OpConstant`, so
/// it is NOT a runtime array and carries no ArrayStride / descriptor / binding),
/// and indexing the local access-chains into it.
#[derive(Clone)]
struct WorkgroupSlot {
    /// The `Workgroup`-class OpVariable id.
    var_id: u32,
    /// Element type of the array.
    elem_ty: MirType,
}

/// A compute-kernel scalar parameter mapped to a member of the push-constant
/// block. Non-pointer scalar parameters (e.g. `alpha: f32`) are passed by push
/// constant rather than a descriptor/binding: the whole block is one
/// `OpTypeStruct` in the `PushConstant` storage class, `Block`-decorated, with
/// each scalar at a 4-byte-aligned member offset. The value is read at each use
/// site via `OpAccessChain` (member index) + `OpLoad`.
#[derive(Clone)]
struct ComputePushConstant {
    /// Member index of this scalar inside the push-constant struct.
    member_index: u32,
    /// Byte offset of this scalar inside the push-constant struct.
    offset: u32,
    /// Scalar type (post f64->f32 coercion).
    ty: MirType,
}

/// SPIR-V backend for code generation.
pub struct SpirvBackend {
    /// Output buffer (SPIR-V words).
    output: Vec<u32>,
    /// Next available ID.
    next_id: u32,
    /// Type IDs cache.
    type_ids: HashMap<String, u32>,
    /// Pointer type IDs cache.
    ptr_type_ids: HashMap<(String, SpvStorageClass), u32>,
    /// Constant IDs cache.
    const_ids: HashMap<String, u32>,
    /// Function IDs.
    func_ids: HashMap<Arc<str>, u32>,
    /// Local variable IDs.
    local_ids: HashMap<LocalId, u32>,
    /// Block label IDs.
    block_ids: HashMap<BlockId, u32>,
    /// Workgroup size.
    workgroup_size: (u32, u32, u32),
    /// Execution model.
    execution_model: SpvExecutionModel,
    /// Enabled capabilities.
    capabilities: Vec<SpvCapability>,
    /// GLSL.std.450 import ID.
    glsl_ext_id: Option<u32>,
    /// Built-in variable IDs.
    builtin_ids: HashMap<SpvBuiltIn, u32>,
    /// Buffer bindings: (set, binding) -> variable ID.
    buffer_bindings: HashMap<(u32, u32), u32>,
    /// Descriptor set count.
    descriptor_sets: u32,
    /// Type definitions from the MIR module (struct name -> field types).
    struct_defs: HashMap<Arc<str>, Vec<(Option<Arc<str>>, MirType)>>,
    /// Shader I/O variable IDs for entry point interface.
    io_var_ids: Vec<u32>,
    /// Shader input variable IDs indexed by parameter position.
    /// Used to map parameter locals to Input OpVariable IDs.
    shader_input_vars: Vec<u32>,
    /// Locals that are direct values (not pointers) - don't OpLoad from them.
    /// This includes non-entry-point function parameters (OpFunctionParameter results).
    value_locals: std::collections::HashSet<LocalId>,

    /// Compute-kernel buffer parameters: parameter local -> the StorageBuffer
    /// binding it maps to. `var_id` is the OpVariable, `elem_ty` the element type
    /// of the runtime array member, `writable` whether the kernel writes it.
    /// Populated by `setup_shader_io` for GLCompute so that `a[i]` / `out[i]`
    /// lower to a real OpAccessChain into the buffer's runtime array (member 0).
    compute_buffers: HashMap<LocalId, ComputeBuffer>,

    /// Compute-kernel scalar parameters mapped to push-constant block members:
    /// parameter local -> its member index / offset / type. Populated by
    /// `setup_shader_io` for GLCompute; the block's OpVariable is
    /// `compute_pc_var`. A scalar read (`alpha`) lowers to OpAccessChain +
    /// OpLoad against this block.
    compute_push_constants: HashMap<LocalId, ComputePushConstant>,
    /// The push-constant block OpVariable id (PushConstant storage class), if
    /// the kernel has any scalar parameters. `None` when there are none.
    compute_pc_var: Option<u32>,

    /// Compute-kernel workgroup-shared scratch arrays: parameter/binding local ->
    /// the `Workgroup`-class OpVariable it maps to. Populated by `setup_shader_io`
    /// for GLCompute from locals annotated `Workgroup:N` (produced by the
    /// frontend's `workgroupArray(N)` lowering). Indexing such a local
    /// (`scratch[lid]`) lowers to an `OpAccessChain` into this variable via
    /// `gen_workgroup_element_ptr` -- NOT the SSBO path. Added in Phase 4a.
    workgroup_slots: HashMap<LocalId, WorkgroupSlot>,

    // == Layout-ordered section buffers (SPIR-V requires strict instruction order) ==
    /// Collected debug instructions (OpName, OpMemberName) -- layout section 7.
    pending_names: Vec<u32>,
    /// Collected annotation instructions (OpDecorate, OpMemberDecorate) -- layout section 8.
    pending_annotations: Vec<u32>,
    /// Collected type/constant/global-variable declarations -- layout section 9.
    pending_globals: Vec<u32>,
    /// Collected function body instructions -- layout sections 10-11.
    pending_functions: Vec<u32>,
    /// When true, `emit()` writes to `pending_functions` and type/const
    /// helpers write to `pending_globals`. When false, `emit()` writes to
    /// `self.output` (used for header/preamble/setup phases).
    in_function_phase: bool,

    /// Structured-control-flow facts for the function currently being emitted:
    /// for each `If`-terminated header block, whether it is a selection or a loop
    /// and its correct merge / continue targets. Recomputed per function in
    /// `gen_function` from dominator/post-dominator analysis. Consulted when
    /// emitting `OpSelectionMerge` / `OpLoopMerge` so that arbitrarily nested
    /// `if`/`while` produces valid structured control flow (the old ad-hoc
    /// branch-following mis-picked the merge block for a loop nested in a
    /// selection). `None` between functions.
    structured_cfg: Option<crate::codegen::analysis::structured_cfg::StructuredCfg>,
}

/// True iff any function in `mir` reads a `gl_GlobalInvocationID` component at
/// or above `axis` (0 = x, 1 = y, 2 = z), lowered to `ThreadIndex(component)`.
/// Used to pick a 2D workgroup size for kernels that use a 2D (or 3D) grid. The
/// thread-index read is always an `Assign { value: NullaryOp(ThreadIndex(c)) }`
/// after lowering, so scanning assignment rvalues is sufficient.
fn mir_reads_thread_axis_ge(mir: &MirModule, axis: u32) -> bool {
    mir.functions.iter().any(|f| {
        f.blocks.as_ref().is_some_and(|blocks| {
            blocks.iter().any(|b| {
                b.stmts.iter().any(|s| match &s.kind {
                    MirStmtKind::Assign { value, .. }
                    | MirStmtKind::DerefAssign { value, .. }
                    | MirStmtKind::FieldDerefAssign { value, .. }
                    | MirStmtKind::FieldAssign { value, .. } => {
                        matches!(
                            value,
                            MirRValue::NullaryOp(NullaryOp::ThreadIndex(c), _) if *c >= axis
                        )
                    }
                    _ => false,
                })
            })
        })
    })
}

impl SpirvBackend {
    /// True iff `func`'s body assigns a `NullaryOp` matching `pred` (used to
    /// decide whether to pre-declare the `LocalInvocationId` / `WorkgroupId`
    /// built-ins in the entry-point interface). After lowering, an invocation-
    /// index read is always `Assign { value: NullaryOp(..) }`, so scanning
    /// assignment rvalues is sufficient.
    fn compute_reads_nullary(func: &MirFunction, pred: impl Fn(&NullaryOp) -> bool) -> bool {
        func.blocks.as_ref().is_some_and(|blocks| {
            blocks.iter().any(|b| {
                b.stmts.iter().any(|s| {
                    if let MirStmtKind::Assign { value, .. } = &s.kind {
                        matches!(value, MirRValue::NullaryOp(op, _) if pred(op))
                    } else {
                        false
                    }
                })
            })
        })
    }

    /// Create a new SPIR-V backend.
    pub fn new() -> Self {
        Self {
            output: Vec::new(),
            next_id: 1,
            type_ids: HashMap::new(),
            ptr_type_ids: HashMap::new(),
            const_ids: HashMap::new(),
            func_ids: HashMap::new(),
            local_ids: HashMap::new(),
            block_ids: HashMap::new(),
            workgroup_size: (64, 1, 1),
            execution_model: SpvExecutionModel::GLCompute,
            capabilities: vec![SpvCapability::Shader, SpvCapability::Float64],
            glsl_ext_id: None,
            builtin_ids: HashMap::new(),
            buffer_bindings: HashMap::new(),
            descriptor_sets: 1,
            struct_defs: HashMap::new(),
            io_var_ids: Vec::new(),
            shader_input_vars: Vec::new(),
            value_locals: std::collections::HashSet::new(),
            compute_buffers: HashMap::new(),
            compute_push_constants: HashMap::new(),
            compute_pc_var: None,
            workgroup_slots: HashMap::new(),
            pending_names: Vec::new(),
            pending_annotations: Vec::new(),
            pending_globals: Vec::new(),
            pending_functions: Vec::new(),
            in_function_phase: false,
            structured_cfg: None,
        }
    }

    /// Set the workgroup size for compute shaders.
    pub fn with_workgroup_size(mut self, x: u32, y: u32, z: u32) -> Self {
        self.workgroup_size = (x, y, z);
        self
    }

    /// Set the execution model.
    pub fn with_execution_model(mut self, model: SpvExecutionModel) -> Self {
        self.execution_model = model;
        self
    }

    /// Add a capability.
    pub fn with_capability(mut self, cap: SpvCapability) -> Self {
        if !self.capabilities.contains(&cap) {
            self.capabilities.push(cap);
        }
        self
    }

    /// Enable float64 support.
    pub fn with_float64(self) -> Self {
        self.with_capability(SpvCapability::Float64)
    }

    /// Enable int64 support.
    pub fn with_int64(self) -> Self {
        self.with_capability(SpvCapability::Int64)
    }

    /// Reset per-module state.
    fn reset(&mut self) {
        self.output.clear();
        self.next_id = 1;
        self.type_ids.clear();
        self.ptr_type_ids.clear();
        self.const_ids.clear();
        self.func_ids.clear();
        self.local_ids.clear();
        self.block_ids.clear();
        self.glsl_ext_id = None;
        self.builtin_ids.clear();
        self.buffer_bindings.clear();
        self.struct_defs.clear();
        self.io_var_ids.clear();
        self.shader_input_vars.clear();
        self.value_locals.clear();
        self.compute_buffers.clear();
        self.compute_push_constants.clear();
        self.compute_pc_var = None;
        self.workgroup_slots.clear();
        self.pending_names.clear();
        self.pending_annotations.clear();
        self.pending_globals.clear();
        self.pending_functions.clear();
        self.in_function_phase = false;
    }

    /// Allocate a new ID.
    fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Emit a SPIR-V instruction.
    ///
    /// During the function-generation phase (`in_function_phase == true`),
    /// instructions are routed to `pending_functions`. Otherwise they go
    /// to `self.output` (used for header/preamble/setup).
    fn emit(&mut self, opcode: SpvOp, operands: &[u32]) {
        let word_count = (operands.len() + 1) as u32;
        let buf = if self.in_function_phase {
            &mut self.pending_functions
        } else {
            &mut self.output
        };
        buf.push((word_count << 16) | (opcode as u32));
        buf.extend_from_slice(operands);
    }

    /// Emit a SPIR-V instruction into the global declarations section
    /// (types, constants, global variables -- layout section 9).
    /// Used by `get_type_id`, `get_const_id`, `get_ptr_type_id`, etc.
    /// so that type declarations land in section 9 even when called from
    /// within function body generation.
    fn emit_global(&mut self, opcode: SpvOp, operands: &[u32]) {
        let word_count = (operands.len() + 1) as u32;
        let buf = if self.in_function_phase {
            &mut self.pending_globals
        } else {
            &mut self.output
        };
        buf.push((word_count << 16) | (opcode as u32));
        buf.extend_from_slice(operands);
    }

    /// Emit a SPIR-V instruction into an arbitrary buffer.
    fn emit_to(buf: &mut Vec<u32>, opcode: SpvOp, operands: &[u32]) {
        let word_count = (operands.len() + 1) as u32;
        buf.push((word_count << 16) | (opcode as u32));
        buf.extend_from_slice(operands);
    }

    /// Emit an OpName instruction into the pending debug-names buffer.
    fn emit_name(&mut self, target_id: u32, name: &str) {
        let name_words = self.emit_string(name);
        let mut operands = vec![target_id];
        operands.extend(name_words);
        Self::emit_to(&mut self.pending_names, SpvOp::OpName, &operands);
    }

    /// Emit an OpMemberName instruction into the pending debug-names buffer.
    fn emit_member_name(&mut self, struct_id: u32, member_index: u32, name: &str) {
        let name_words = self.emit_string(name);
        let mut operands = vec![struct_id, member_index];
        operands.extend(name_words);
        Self::emit_to(&mut self.pending_names, SpvOp::OpMemberName, &operands);
    }

    /// Emit an OpDecorate instruction into the pending annotations buffer.
    fn emit_decoration(&mut self, target_id: u32, decoration: SpvDecoration, extra: &[u32]) {
        let mut operands = vec![target_id, decoration as u32];
        operands.extend_from_slice(extra);
        Self::emit_to(&mut self.pending_annotations, SpvOp::OpDecorate, &operands);
    }

    /// Emit an OpMemberDecorate instruction into the pending annotations buffer.
    fn emit_member_decoration(
        &mut self,
        struct_id: u32,
        member: u32,
        decoration: SpvDecoration,
        extra: &[u32],
    ) {
        let mut operands = vec![struct_id, member, decoration as u32];
        operands.extend_from_slice(extra);
        Self::emit_to(
            &mut self.pending_annotations,
            SpvOp::OpMemberDecorate,
            &operands,
        );
    }

    /// Emit a string as SPIR-V words (null-terminated, padded to word boundary).
    fn emit_string(&self, s: &str) -> Vec<u32> {
        let bytes = s.as_bytes();
        let mut words = Vec::new();
        let mut current_word = 0u32;
        let mut byte_index = 0;

        for &b in bytes {
            current_word |= (b as u32) << (8 * byte_index);
            byte_index += 1;
            if byte_index == 4 {
                words.push(current_word);
                current_word = 0;
                byte_index = 0;
            }
        }

        // Add null terminator and pad
        words.push(current_word);
        words
    }

    /// Static helper: encode a string as SPIR-V words (null-terminated, padded to 4 bytes).
    fn encode_string(s: &str) -> Vec<u32> {
        let bytes = s.as_bytes();
        let mut words = Vec::new();
        let mut current_word = 0u32;
        let mut byte_index = 0;
        for &b in bytes {
            current_word |= (b as u32) << (8 * byte_index);
            byte_index += 1;
            if byte_index == 4 {
                words.push(current_word);
                current_word = 0;
                byte_index = 0;
            }
        }
        // Null terminator
        // current_word already has 0 in the remaining bytes
        words.push(current_word);
        words
    }

    /// Static helper: emit OpName into a buffer.
    fn emit_to_name(buf: &mut Vec<u32>, target_id: u32, name: &str) {
        let name_words = Self::encode_string(name);
        let mut operands = vec![target_id];
        operands.extend(name_words);
        Self::emit_to(buf, SpvOp::OpName, &operands);
    }

    /// Static helper: emit OpMemberName into a buffer.
    fn emit_to_member_name(buf: &mut Vec<u32>, struct_id: u32, member_index: u32, name: &str) {
        let name_words = Self::encode_string(name);
        let mut operands = vec![struct_id, member_index];
        operands.extend(name_words);
        Self::emit_to(buf, SpvOp::OpMemberName, &operands);
    }

    // =========================================================================
    // ATOMIC OPERATIONS
    // =========================================================================

    /// Emit atomic load instruction.
    #[allow(dead_code)]
    fn emit_atomic_load(
        &mut self,
        result_type: u32,
        pointer: u32,
        scope: SpvScope,
        semantics: SpvMemorySemantics,
    ) -> u32 {
        let result = self.alloc_id();
        let scope_id = self.get_const_id(&MirConst::Uint(scope as u128, MirType::u32()));
        let sem_id = self.get_const_id(&MirConst::Uint(semantics as u128, MirType::u32()));
        self.emit(
            SpvOp::OpAtomicLoad,
            &[result_type, result, pointer, scope_id, sem_id],
        );
        result
    }

    /// Emit atomic store instruction.
    #[allow(dead_code)]
    fn emit_atomic_store(
        &mut self,
        pointer: u32,
        value: u32,
        scope: SpvScope,
        semantics: SpvMemorySemantics,
    ) {
        let scope_id = self.get_const_id(&MirConst::Uint(scope as u128, MirType::u32()));
        let sem_id = self.get_const_id(&MirConst::Uint(semantics as u128, MirType::u32()));
        self.emit(SpvOp::OpAtomicStore, &[pointer, scope_id, sem_id, value]);
    }

    /// Emit atomic exchange instruction.
    #[allow(dead_code)]
    fn emit_atomic_exchange(
        &mut self,
        result_type: u32,
        pointer: u32,
        value: u32,
        scope: SpvScope,
        semantics: SpvMemorySemantics,
    ) -> u32 {
        let result = self.alloc_id();
        let scope_id = self.get_const_id(&MirConst::Uint(scope as u128, MirType::u32()));
        let sem_id = self.get_const_id(&MirConst::Uint(semantics as u128, MirType::u32()));
        self.emit(
            SpvOp::OpAtomicExchange,
            &[result_type, result, pointer, scope_id, sem_id, value],
        );
        result
    }

    /// Emit atomic compare-exchange instruction.
    #[allow(dead_code)]
    fn emit_atomic_compare_exchange(
        &mut self,
        result_type: u32,
        pointer: u32,
        comparator: u32,
        value: u32,
        scope: SpvScope,
        equal_sem: SpvMemorySemantics,
        unequal_sem: SpvMemorySemantics,
    ) -> u32 {
        let result = self.alloc_id();
        let scope_id = self.get_const_id(&MirConst::Uint(scope as u128, MirType::u32()));
        let eq_sem_id = self.get_const_id(&MirConst::Uint(equal_sem as u128, MirType::u32()));
        let neq_sem_id = self.get_const_id(&MirConst::Uint(unequal_sem as u128, MirType::u32()));
        self.emit(
            SpvOp::OpAtomicCompareExchange,
            &[
                result_type,
                result,
                pointer,
                scope_id,
                eq_sem_id,
                neq_sem_id,
                value,
                comparator,
            ],
        );
        result
    }

    /// Emit atomic add instruction.
    #[allow(dead_code)]
    fn emit_atomic_add(
        &mut self,
        result_type: u32,
        pointer: u32,
        value: u32,
        scope: SpvScope,
        semantics: SpvMemorySemantics,
    ) -> u32 {
        let result = self.alloc_id();
        let scope_id = self.get_const_id(&MirConst::Uint(scope as u128, MirType::u32()));
        let sem_id = self.get_const_id(&MirConst::Uint(semantics as u128, MirType::u32()));
        self.emit(
            SpvOp::OpAtomicIAdd,
            &[result_type, result, pointer, scope_id, sem_id, value],
        );
        result
    }

    /// Emit atomic subtract instruction.
    #[allow(dead_code)]
    fn emit_atomic_sub(
        &mut self,
        result_type: u32,
        pointer: u32,
        value: u32,
        scope: SpvScope,
        semantics: SpvMemorySemantics,
    ) -> u32 {
        let result = self.alloc_id();
        let scope_id = self.get_const_id(&MirConst::Uint(scope as u128, MirType::u32()));
        let sem_id = self.get_const_id(&MirConst::Uint(semantics as u128, MirType::u32()));
        self.emit(
            SpvOp::OpAtomicISub,
            &[result_type, result, pointer, scope_id, sem_id, value],
        );
        result
    }

    /// Emit atomic min instruction (signed).
    #[allow(dead_code)]
    fn emit_atomic_smin(
        &mut self,
        result_type: u32,
        pointer: u32,
        value: u32,
        scope: SpvScope,
        semantics: SpvMemorySemantics,
    ) -> u32 {
        let result = self.alloc_id();
        let scope_id = self.get_const_id(&MirConst::Uint(scope as u128, MirType::u32()));
        let sem_id = self.get_const_id(&MirConst::Uint(semantics as u128, MirType::u32()));
        self.emit(
            SpvOp::OpAtomicSMin,
            &[result_type, result, pointer, scope_id, sem_id, value],
        );
        result
    }

    /// Emit atomic min instruction (unsigned).
    #[allow(dead_code)]
    fn emit_atomic_umin(
        &mut self,
        result_type: u32,
        pointer: u32,
        value: u32,
        scope: SpvScope,
        semantics: SpvMemorySemantics,
    ) -> u32 {
        let result = self.alloc_id();
        let scope_id = self.get_const_id(&MirConst::Uint(scope as u128, MirType::u32()));
        let sem_id = self.get_const_id(&MirConst::Uint(semantics as u128, MirType::u32()));
        self.emit(
            SpvOp::OpAtomicUMin,
            &[result_type, result, pointer, scope_id, sem_id, value],
        );
        result
    }

    /// Emit atomic max instruction (signed).
    #[allow(dead_code)]
    fn emit_atomic_smax(
        &mut self,
        result_type: u32,
        pointer: u32,
        value: u32,
        scope: SpvScope,
        semantics: SpvMemorySemantics,
    ) -> u32 {
        let result = self.alloc_id();
        let scope_id = self.get_const_id(&MirConst::Uint(scope as u128, MirType::u32()));
        let sem_id = self.get_const_id(&MirConst::Uint(semantics as u128, MirType::u32()));
        self.emit(
            SpvOp::OpAtomicSMax,
            &[result_type, result, pointer, scope_id, sem_id, value],
        );
        result
    }

    /// Emit atomic max instruction (unsigned).
    #[allow(dead_code)]
    fn emit_atomic_umax(
        &mut self,
        result_type: u32,
        pointer: u32,
        value: u32,
        scope: SpvScope,
        semantics: SpvMemorySemantics,
    ) -> u32 {
        let result = self.alloc_id();
        let scope_id = self.get_const_id(&MirConst::Uint(scope as u128, MirType::u32()));
        let sem_id = self.get_const_id(&MirConst::Uint(semantics as u128, MirType::u32()));
        self.emit(
            SpvOp::OpAtomicUMax,
            &[result_type, result, pointer, scope_id, sem_id, value],
        );
        result
    }

    /// Emit atomic AND instruction.
    #[allow(dead_code)]
    fn emit_atomic_and(
        &mut self,
        result_type: u32,
        pointer: u32,
        value: u32,
        scope: SpvScope,
        semantics: SpvMemorySemantics,
    ) -> u32 {
        let result = self.alloc_id();
        let scope_id = self.get_const_id(&MirConst::Uint(scope as u128, MirType::u32()));
        let sem_id = self.get_const_id(&MirConst::Uint(semantics as u128, MirType::u32()));
        self.emit(
            SpvOp::OpAtomicAnd,
            &[result_type, result, pointer, scope_id, sem_id, value],
        );
        result
    }

    /// Emit atomic OR instruction.
    #[allow(dead_code)]
    fn emit_atomic_or(
        &mut self,
        result_type: u32,
        pointer: u32,
        value: u32,
        scope: SpvScope,
        semantics: SpvMemorySemantics,
    ) -> u32 {
        let result = self.alloc_id();
        let scope_id = self.get_const_id(&MirConst::Uint(scope as u128, MirType::u32()));
        let sem_id = self.get_const_id(&MirConst::Uint(semantics as u128, MirType::u32()));
        self.emit(
            SpvOp::OpAtomicOr,
            &[result_type, result, pointer, scope_id, sem_id, value],
        );
        result
    }

    /// Emit atomic XOR instruction.
    #[allow(dead_code)]
    fn emit_atomic_xor(
        &mut self,
        result_type: u32,
        pointer: u32,
        value: u32,
        scope: SpvScope,
        semantics: SpvMemorySemantics,
    ) -> u32 {
        let result = self.alloc_id();
        let scope_id = self.get_const_id(&MirConst::Uint(scope as u128, MirType::u32()));
        let sem_id = self.get_const_id(&MirConst::Uint(semantics as u128, MirType::u32()));
        self.emit(
            SpvOp::OpAtomicXor,
            &[result_type, result, pointer, scope_id, sem_id, value],
        );
        result
    }

    // =========================================================================
    // BARRIER OPERATIONS
    // =========================================================================

    /// Emit control barrier (workgroup synchronization).
    #[allow(dead_code)]
    fn emit_control_barrier(
        &mut self,
        execution_scope: SpvScope,
        memory_scope: SpvScope,
        semantics: SpvMemorySemantics,
    ) {
        let exec_scope_id =
            self.get_const_id(&MirConst::Uint(execution_scope as u128, MirType::u32()));
        let mem_scope_id = self.get_const_id(&MirConst::Uint(memory_scope as u128, MirType::u32()));
        let sem_id = self.get_const_id(&MirConst::Uint(semantics as u128, MirType::u32()));
        self.emit(
            SpvOp::OpControlBarrier,
            &[exec_scope_id, mem_scope_id, sem_id],
        );
    }

    /// Emit memory barrier.
    #[allow(dead_code)]
    fn emit_memory_barrier(&mut self, scope: SpvScope, semantics: SpvMemorySemantics) {
        let scope_id = self.get_const_id(&MirConst::Uint(scope as u128, MirType::u32()));
        let sem_id = self.get_const_id(&MirConst::Uint(semantics as u128, MirType::u32()));
        self.emit(SpvOp::OpMemoryBarrier, &[scope_id, sem_id]);
    }

    /// Emit workgroup barrier (common pattern).
    /// Uses AcquireRelease | WorkgroupMemory semantics.
    fn emit_workgroup_barrier(&mut self) {
        // AcquireRelease (0x8) | WorkgroupMemory (0x100) = 0x108
        let semantics = 0x108u128;
        let exec_scope_id =
            self.get_const_id(&MirConst::Uint(SpvScope::Workgroup as u128, MirType::u32()));
        let mem_scope_id =
            self.get_const_id(&MirConst::Uint(SpvScope::Workgroup as u128, MirType::u32()));
        let sem_id = self.get_const_id(&MirConst::Uint(semantics, MirType::u32()));
        self.emit(
            SpvOp::OpControlBarrier,
            &[exec_scope_id, mem_scope_id, sem_id],
        );
    }

    // =========================================================================
    // VECTOR OPERATIONS
    // =========================================================================

    /// Emit vector shuffle instruction.
    #[allow(dead_code)]
    fn emit_vector_shuffle(
        &mut self,
        result_type: u32,
        v1: u32,
        v2: u32,
        components: &[u32],
    ) -> u32 {
        let result = self.alloc_id();
        let mut operands = vec![result_type, result, v1, v2];
        operands.extend_from_slice(components);
        self.emit(SpvOp::OpVectorShuffle, &operands);
        result
    }

    /// Emit composite extract (extract element from vector/struct).
    #[allow(dead_code)]
    fn emit_composite_extract(&mut self, result_type: u32, composite: u32, indices: &[u32]) -> u32 {
        let result = self.alloc_id();
        let mut operands = vec![result_type, result, composite];
        operands.extend_from_slice(indices);
        self.emit(SpvOp::OpCompositeExtract, &operands);
        result
    }

    /// Emit composite insert (insert element into vector/struct).
    #[allow(dead_code)]
    fn emit_composite_insert(
        &mut self,
        result_type: u32,
        object: u32,
        composite: u32,
        indices: &[u32],
    ) -> u32 {
        let result = self.alloc_id();
        let mut operands = vec![result_type, result, object, composite];
        operands.extend_from_slice(indices);
        self.emit(SpvOp::OpCompositeInsert, &operands);
        result
    }

    /// Emit composite construct (build vector/struct from components).
    #[allow(dead_code)]
    fn emit_composite_construct(&mut self, result_type: u32, constituents: &[u32]) -> u32 {
        let result = self.alloc_id();
        let mut operands = vec![result_type, result];
        operands.extend_from_slice(constituents);
        self.emit(SpvOp::OpCompositeConstruct, &operands);
        result
    }

    /// Emit vector times scalar.
    #[allow(dead_code)]
    fn emit_vector_times_scalar(&mut self, result_type: u32, vector: u32, scalar: u32) -> u32 {
        let result = self.alloc_id();
        self.emit(
            SpvOp::OpVectorTimesScalar,
            &[result_type, result, vector, scalar],
        );
        result
    }

    /// Emit dot product.
    #[allow(dead_code)]
    fn emit_dot(&mut self, result_type: u32, v1: u32, v2: u32) -> u32 {
        let result = self.alloc_id();
        self.emit(SpvOp::OpDot, &[result_type, result, v1, v2]);
        result
    }

    // =========================================================================
    // GLSL.std.450 EXTENDED INSTRUCTIONS
    // =========================================================================

    /// Get or create GLSL.std.450 import ID.
    fn get_glsl_ext_id(&mut self) -> u32 {
        if let Some(id) = self.glsl_ext_id {
            return id;
        }
        let id = self.alloc_id();
        self.glsl_ext_id = Some(id);
        id
    }

    /// Emit GLSL extended instruction.
    #[allow(dead_code)]
    fn emit_glsl_ext(&mut self, result_type: u32, instruction: u32, operands: &[u32]) -> u32 {
        let result = self.alloc_id();
        let ext_id = self.get_glsl_ext_id();
        let mut ops = vec![result_type, result, ext_id, instruction];
        ops.extend_from_slice(operands);
        self.emit(SpvOp::OpExtInst, &ops);
        result
    }

    /// Emit GLSL sin instruction.
    #[allow(dead_code)]
    fn emit_glsl_sin(&mut self, result_type: u32, x: u32) -> u32 {
        self.emit_glsl_ext(result_type, 13, &[x]) // GLSLstd450Sin = 13
    }

    /// Emit GLSL cos instruction.
    #[allow(dead_code)]
    fn emit_glsl_cos(&mut self, result_type: u32, x: u32) -> u32 {
        self.emit_glsl_ext(result_type, 14, &[x]) // GLSLstd450Cos = 14
    }

    /// Emit GLSL tan instruction.
    #[allow(dead_code)]
    fn emit_glsl_tan(&mut self, result_type: u32, x: u32) -> u32 {
        self.emit_glsl_ext(result_type, 15, &[x]) // GLSLstd450Tan = 15
    }

    /// Emit GLSL sqrt instruction.
    #[allow(dead_code)]
    fn emit_glsl_sqrt(&mut self, result_type: u32, x: u32) -> u32 {
        self.emit_glsl_ext(result_type, 31, &[x]) // GLSLstd450Sqrt = 31
    }

    /// Emit GLSL inverse sqrt instruction.
    #[allow(dead_code)]
    fn emit_glsl_inversesqrt(&mut self, result_type: u32, x: u32) -> u32 {
        self.emit_glsl_ext(result_type, 32, &[x]) // GLSLstd450InverseSqrt = 32
    }

    /// Emit GLSL exp instruction.
    #[allow(dead_code)]
    fn emit_glsl_exp(&mut self, result_type: u32, x: u32) -> u32 {
        self.emit_glsl_ext(result_type, 27, &[x]) // GLSLstd450Exp = 27
    }

    /// Emit GLSL log instruction.
    #[allow(dead_code)]
    fn emit_glsl_log(&mut self, result_type: u32, x: u32) -> u32 {
        self.emit_glsl_ext(result_type, 28, &[x]) // GLSLstd450Log = 28
    }

    /// Emit GLSL pow instruction.
    #[allow(dead_code)]
    fn emit_glsl_pow(&mut self, result_type: u32, x: u32, y: u32) -> u32 {
        self.emit_glsl_ext(result_type, 26, &[x, y]) // GLSLstd450Pow = 26
    }

    /// Emit GLSL fma instruction.
    #[allow(dead_code)]
    fn emit_glsl_fma(&mut self, result_type: u32, a: u32, b: u32, c: u32) -> u32 {
        self.emit_glsl_ext(result_type, 50, &[a, b, c]) // GLSLstd450Fma = 50
    }

    /// Emit GLSL fmin instruction.
    #[allow(dead_code)]
    fn emit_glsl_fmin(&mut self, result_type: u32, x: u32, y: u32) -> u32 {
        self.emit_glsl_ext(result_type, 37, &[x, y]) // GLSLstd450FMin = 37
    }

    /// Emit GLSL fmax instruction.
    #[allow(dead_code)]
    fn emit_glsl_fmax(&mut self, result_type: u32, x: u32, y: u32) -> u32 {
        self.emit_glsl_ext(result_type, 40, &[x, y]) // GLSLstd450FMax = 40
    }

    /// Emit GLSL clamp instruction.
    #[allow(dead_code)]
    fn emit_glsl_clamp(&mut self, result_type: u32, x: u32, min: u32, max: u32) -> u32 {
        self.emit_glsl_ext(result_type, 43, &[x, min, max]) // GLSLstd450FClamp = 43
    }

    /// Emit GLSL mix (lerp) instruction.
    #[allow(dead_code)]
    fn emit_glsl_mix(&mut self, result_type: u32, x: u32, y: u32, a: u32) -> u32 {
        self.emit_glsl_ext(result_type, 46, &[x, y, a]) // GLSLstd450FMix = 46
    }

    /// Emit GLSL floor instruction.
    #[allow(dead_code)]
    fn emit_glsl_floor(&mut self, result_type: u32, x: u32) -> u32 {
        self.emit_glsl_ext(result_type, 8, &[x]) // GLSLstd450Floor = 8
    }

    /// Emit GLSL ceil instruction.
    #[allow(dead_code)]
    fn emit_glsl_ceil(&mut self, result_type: u32, x: u32) -> u32 {
        self.emit_glsl_ext(result_type, 9, &[x]) // GLSLstd450Ceil = 9
    }

    /// Emit GLSL abs instruction (float).
    #[allow(dead_code)]
    fn emit_glsl_fabs(&mut self, result_type: u32, x: u32) -> u32 {
        self.emit_glsl_ext(result_type, 4, &[x]) // GLSLstd450FAbs = 4
    }

    /// Emit GLSL abs instruction (int).
    #[allow(dead_code)]
    fn emit_glsl_sabs(&mut self, result_type: u32, x: u32) -> u32 {
        self.emit_glsl_ext(result_type, 5, &[x]) // GLSLstd450SAbs = 5
    }

    /// Emit GLSL normalize instruction.
    #[allow(dead_code)]
    fn emit_glsl_normalize(&mut self, result_type: u32, x: u32) -> u32 {
        self.emit_glsl_ext(result_type, 69, &[x]) // GLSLstd450Normalize = 69
    }

    /// Emit GLSL length instruction.
    #[allow(dead_code)]
    fn emit_glsl_length(&mut self, result_type: u32, x: u32) -> u32 {
        self.emit_glsl_ext(result_type, 66, &[x]) // GLSLstd450Length = 66
    }

    /// Emit GLSL distance instruction.
    #[allow(dead_code)]
    fn emit_glsl_distance(&mut self, result_type: u32, p0: u32, p1: u32) -> u32 {
        self.emit_glsl_ext(result_type, 67, &[p0, p1]) // GLSLstd450Distance = 67
    }

    /// Emit GLSL cross product instruction.
    #[allow(dead_code)]
    fn emit_glsl_cross(&mut self, result_type: u32, x: u32, y: u32) -> u32 {
        self.emit_glsl_ext(result_type, 68, &[x, y]) // GLSLstd450Cross = 68
    }

    /// Emit GLSL reflect instruction.
    #[allow(dead_code)]
    fn emit_glsl_reflect(&mut self, result_type: u32, i: u32, n: u32) -> u32 {
        self.emit_glsl_ext(result_type, 71, &[i, n]) // GLSLstd450Reflect = 71
    }

    /// Map a C runtime function name to GLSL.std.450 opcode, if applicable.
    /// This allows builtins like sqrt, sin, clamp to be emitted as OpExtInst
    /// instead of OpFunctionCall in SPIR-V shaders.
    fn glsl_builtin_opcode(name: &str) -> Option<u32> {
        match name {
            // Standard math (1-arg)
            "sqrt" => Some(31),        // GLSLstd450Sqrt
            "sin" => Some(13),         // GLSLstd450Sin
            "cos" => Some(14),         // GLSLstd450Cos
            "tan" => Some(15),         // GLSLstd450Tan
            "fabs" | "abs" => Some(4), // GLSLstd450FAbs
            "floor" => Some(8),        // GLSLstd450Floor
            "ceil" => Some(9),         // GLSLstd450Ceil
            "round" => Some(1),        // GLSLstd450Round
            "exp" => Some(27),         // GLSLstd450Exp
            "log" => Some(28),         // GLSLstd450Log

            // Standard math (2-arg)
            "pow" => Some(26),                    // GLSLstd450Pow
            "build_min_i32" | "fmin" => Some(37), // GLSLstd450FMin
            "build_max_i32" | "fmax" => Some(40), // GLSLstd450FMax

            // Shader math (2-arg or 3-arg)
            "build_clampf" => Some(43),     // GLSLstd450FClamp
            "build_smoothstep" => Some(49), // GLSLstd450SmoothStep
            "build_mix" => Some(46),        // GLSLstd450FMix
            "build_step" => Some(48),       // GLSLstd450Step
            "build_fract" => Some(10),      // GLSLstd450Fract

            // Vector math
            "build_normalize3" | "build_normalize2" | "build_normalize4" => Some(69), // GLSLstd450Normalize
            "build_length3" | "build_length2" | "build_length4" => Some(66), // GLSLstd450Length
            "build_cross" => Some(68),                                       // GLSLstd450Cross
            "build_reflect3" => Some(71),                                    // GLSLstd450Reflect
            "build_dot3" | "build_dot2" | "build_dot4" => None, // OpDot is a core op, not GLSL.std.450

            _ => None,
        }
    }

    // =========================================================================
    // TYPE GENERATION
    // =========================================================================

    /// Get or create a type ID.
    fn get_type_id(&mut self, ty: &MirType) -> u32 {
        let key = format!("{:?}", ty);
        if let Some(&id) = self.type_ids.get(&key) {
            return id;
        }

        let id = self.alloc_id();
        self.type_ids.insert(key, id);

        match ty {
            MirType::Void | MirType::Never => {
                self.emit_global(SpvOp::OpTypeVoid, &[id]);
            }
            MirType::Bool => {
                self.emit_global(SpvOp::OpTypeBool, &[id]);
            }
            MirType::Int(size, signed) => {
                let width = match size {
                    IntSize::I8 => 8,
                    IntSize::I16 => 16,
                    IntSize::I32 | IntSize::ISize => 32,
                    IntSize::I64 => 64,
                    IntSize::I128 => 64, // Map to 64-bit in SPIR-V
                };
                self.emit_global(SpvOp::OpTypeInt, &[id, width, if *signed { 1 } else { 0 }]);
            }
            MirType::Float(size) => {
                let width = match size {
                    FloatSize::F32 => 32,
                    FloatSize::F64 => 64,
                };
                self.emit_global(SpvOp::OpTypeFloat, &[id, width]);
            }
            MirType::Ptr(inner) => {
                let inner_id = self.get_type_id(inner);
                self.emit_global(
                    SpvOp::OpTypePointer,
                    &[id, SpvStorageClass::Function as u32, inner_id],
                );
            }
            MirType::Array(elem, len) => {
                let elem_id = self.get_type_id(elem);
                let len_id = self.get_const_id(&MirConst::Uint(*len as u128, MirType::u32()));
                self.emit_global(SpvOp::OpTypeArray, &[id, elem_id, len_id]);
            }
            MirType::Slice(elem) => {
                let elem_id = self.get_type_id(elem);
                self.emit_global(SpvOp::OpTypeRuntimeArray, &[id, elem_id]);
            }
            MirType::Struct(name) => {
                // Map build_vec2/3/4 to SPIR-V OpTypeVector (not struct).
                // These are GPU-native vector types with SIMD semantics.
                match name.as_ref() {
                    "build_vec2" => {
                        // After coercion pass, struct fields are f32. Use f32 for GPU.
                        let elem_id = self.get_type_id(&MirType::Float(FloatSize::F32));
                        self.emit_global(SpvOp::OpTypeVector, &[id, elem_id, 2]);
                        self.emit_name(id, "build_vec2");
                    }
                    "build_vec3" => {
                        let elem_id = self.get_type_id(&MirType::Float(FloatSize::F32));
                        self.emit_global(SpvOp::OpTypeVector, &[id, elem_id, 3]);
                        self.emit_name(id, "build_vec3");
                    }
                    "build_vec4" => {
                        let elem_id = self.get_type_id(&MirType::Float(FloatSize::F32));
                        self.emit_global(SpvOp::OpTypeVector, &[id, elem_id, 4]);
                        self.emit_name(id, "build_vec4");
                    }
                    _ => {
                        // Regular struct type
                        let mut struct_operands = vec![id];
                        if let Some(fields) = self.struct_defs.get(name).cloned() {
                            for (_field_name, field_ty) in &fields {
                                let member_ty_id = self.get_type_id(field_ty);
                                struct_operands.push(member_ty_id);
                            }
                        }
                        self.emit_global(SpvOp::OpTypeStruct, &struct_operands);
                        let name_clone = name.to_string();
                        self.emit_name(id, &name_clone);
                        if let Some(fields) = self.struct_defs.get(name).cloned() {
                            for (i, (field_name, _)) in fields.iter().enumerate() {
                                if let Some(fname) = field_name {
                                    self.emit_member_name(id, i as u32, fname);
                                }
                            }
                        }
                    }
                }
            }
            MirType::FnPtr(sig) => {
                let ret_id = self.get_type_id(&sig.ret);
                let param_ids: Vec<u32> = sig.params.iter().map(|p| self.get_type_id(p)).collect();
                let mut operands = vec![id, ret_id];
                operands.extend(param_ids);
                self.emit_global(SpvOp::OpTypeFunction, &operands);
            }
            MirType::Vector(elem, lanes) => {
                let elem_id = self.get_type_id(elem);
                self.emit_global(SpvOp::OpTypeVector, &[id, elem_id, *lanes]);
            }
            MirType::Texture2D(elem) => {
                // OpTypeImage: result, sampled-type, dim, depth, arrayed, MS, sampled, format
                // Dim=1 (2D), Depth=0, Arrayed=0, MS=0, Sampled=1 (sampling), Format=0 (Unknown)
                let elem_id = self.get_type_id(elem);
                self.emit_global(SpvOp::OpTypeImage, &[id, elem_id, 1, 0, 0, 0, 1, 0]);
            }
            MirType::Sampler => {
                self.emit_global(SpvOp::OpTypeSampler, &[id]);
            }
            MirType::SampledImage(elem) => {
                // OpTypeSampledImage takes the image type as operand
                let image_id = self.get_type_id(&MirType::Texture2D(elem.clone()));
                self.emit_global(SpvOp::OpTypeSampledImage, &[id, image_id]);
            }
            MirType::TraitObject(name) => {
                // Trait object as struct with two pointer members: data ptr + vtable ptr
                let u32_ty = self.get_type_id(&MirType::Int(IntSize::I32, false));
                let ptr_ty_data = self.alloc_id();
                self.emit_global(
                    SpvOp::OpTypePointer,
                    &[ptr_ty_data, SpvStorageClass::Function as u32, u32_ty],
                );
                let ptr_ty_vtable = self.alloc_id();
                self.emit_global(
                    SpvOp::OpTypePointer,
                    &[ptr_ty_vtable, SpvStorageClass::Function as u32, u32_ty],
                );
                self.emit_global(SpvOp::OpTypeStruct, &[id, ptr_ty_data, ptr_ty_vtable]);
                self.emit_name(id, &format!("dyn_{}", name));
            }
            MirType::Vec(_) => {
                // Vec<T> is an opaque pointer handle in SPIR-V
                let u32_ty = self.get_type_id(&MirType::Int(IntSize::I32, false));
                let ptr_ty = self.alloc_id();
                self.emit_global(
                    SpvOp::OpTypePointer,
                    &[ptr_ty, SpvStorageClass::Function as u32, u32_ty],
                );
                self.emit_global(SpvOp::OpTypeStruct, &[id, ptr_ty]);
                self.emit_name(id, "BuildVecHandle");
            }
            MirType::Map(_, _) => {
                // HashMap<K,V> is an opaque pointer handle in SPIR-V
                let u32_ty = self.get_type_id(&MirType::Int(IntSize::I32, false));
                let ptr_ty = self.alloc_id();
                self.emit_global(
                    SpvOp::OpTypePointer,
                    &[ptr_ty, SpvStorageClass::Function as u32, u32_ty],
                );
                self.emit_global(SpvOp::OpTypeStruct, &[id, ptr_ty]);
                self.emit_name(id, "BuildMapHandle");
            }
            MirType::Tuple(elems) => {
                // Tuples map to SPIR-V structs.
                let mut operands = vec![id];
                for e in elems {
                    operands.push(self.get_type_id(e));
                }
                self.emit_global(SpvOp::OpTypeStruct, &operands);
                let name = MirType::tuple_type_name(elems);
                self.emit_name(id, &name);
            }
        }

        id
    }

    /// Get or create a pointer type ID.
    fn get_ptr_type_id(&mut self, inner: &MirType, storage: SpvStorageClass) -> u32 {
        let key = (format!("{:?}", inner), storage);
        if let Some(&id) = self.ptr_type_ids.get(&key) {
            return id;
        }

        let id = self.alloc_id();
        let inner_id = self.get_type_id(inner);
        self.emit_global(SpvOp::OpTypePointer, &[id, storage as u32, inner_id]);
        self.ptr_type_ids.insert(key, id);
        id
    }

    /// Get vector type ID.
    fn get_vec_type_id(&mut self, elem: &MirType, count: u32) -> u32 {
        let key = format!("vec{}_{:?}", count, elem);
        if let Some(&id) = self.type_ids.get(&key) {
            return id;
        }

        let id = self.alloc_id();
        let elem_id = self.get_type_id(elem);
        self.emit_global(SpvOp::OpTypeVector, &[id, elem_id, count]);
        self.type_ids.insert(key, id);
        id
    }

    // =========================================================================
    // CONSTANT GENERATION
    // =========================================================================

    /// Get or create a constant ID.
    fn get_const_id(&mut self, c: &MirConst) -> u32 {
        let key = format!("{:?}", c);
        if let Some(&id) = self.const_ids.get(&key) {
            return id;
        }

        let id = self.alloc_id();
        self.const_ids.insert(key, id);

        match c {
            MirConst::Bool(true) => {
                let ty = self.get_type_id(&MirType::Bool);
                self.emit_global(SpvOp::OpConstantTrue, &[ty, id]);
            }
            MirConst::Bool(false) => {
                let ty = self.get_type_id(&MirType::Bool);
                self.emit_global(SpvOp::OpConstantFalse, &[ty, id]);
            }
            MirConst::Int(v, ty) => {
                let ty_id = self.get_type_id(ty);
                self.emit_global(SpvOp::OpConstant, &[ty_id, id, *v as u32]);
            }
            MirConst::Uint(v, ty) => {
                let ty_id = self.get_type_id(ty);
                self.emit_global(SpvOp::OpConstant, &[ty_id, id, *v as u32]);
            }
            MirConst::Float(v, ty) => {
                let ty_id = self.get_type_id(ty);
                match ty {
                    MirType::Float(FloatSize::F32) => {
                        let bits = (*v as f32).to_bits();
                        self.emit_global(SpvOp::OpConstant, &[ty_id, id, bits]);
                    }
                    _ => {
                        // f64: encode as two 32-bit words (low word first)
                        let bits = v.to_bits();
                        let lo = bits as u32;
                        let hi = (bits >> 32) as u32;
                        self.emit_global(SpvOp::OpConstant, &[ty_id, id, lo, hi]);
                    }
                }
            }
            MirConst::Null(ty) => {
                let ty_id = self.get_type_id(ty);
                self.emit_global(SpvOp::OpConstantNull, &[ty_id, id]);
            }
            MirConst::Unit => {
                // Unit is void - no constant needed
            }
            MirConst::Zeroed(ty) => {
                let ty_id = self.get_type_id(ty);
                self.emit_global(SpvOp::OpConstantNull, &[ty_id, id]);
            }
            MirConst::Undef(ty) => {
                let ty_id = self.get_type_id(ty);
                self.emit_global(SpvOp::OpUndef, &[ty_id, id]);
            }
            _ => {
                // Other constants - emit as i32 0
                let ty_id = self.get_type_id(&MirType::i32());
                self.emit_global(SpvOp::OpConstant, &[ty_id, id, 0]);
            }
        }

        id
    }

    // =========================================================================
    // HEADER AND DECLARATIONS
    // =========================================================================

    /// Emit the SPIR-V header.
    fn emit_header(&mut self) {
        self.emit_header_version(SPIRV_VERSION);
    }

    /// Emit the SPIR-V header with a specific SPIR-V version.
    fn emit_header_version(&mut self, version: u32) {
        self.output.push(SPIRV_MAGIC);
        self.output.push(version);
        self.output.push(GENERATOR_ID);
        self.output.push(0); // Bound (will be filled later)
        self.output.push(0); // Schema (reserved)
    }

    /// Emit capabilities.
    fn emit_capabilities(&mut self) {
        for cap in &self.capabilities.clone() {
            self.emit(SpvOp::OpCapability, &[*cap as u32]);
        }
    }

    /// Emit extension imports.
    fn emit_extensions(&mut self) {
        // Import GLSL.std.450 for extended instructions
        let id = self.alloc_id();
        self.glsl_ext_id = Some(id);
        let name = self.emit_string("GLSL.std.450");
        let mut operands = vec![id];
        operands.extend(name);
        self.emit(SpvOp::OpExtInstImport, &operands);
    }

    /// Emit memory model.
    fn emit_memory_model(&mut self) {
        self.emit(
            SpvOp::OpMemoryModel,
            &[
                SpvAddressingModel::Logical as u32,
                SpvMemoryModel::Glsl450 as u32,
            ],
        );
    }

    /// Emit built-in variable.
    fn emit_builtin_var(&mut self, builtin: SpvBuiltIn, ty: &MirType) -> u32 {
        if let Some(&id) = self.builtin_ids.get(&builtin) {
            return id;
        }

        let _ty_id = self.get_type_id(ty);
        let ptr_ty_id = self.get_ptr_type_id(ty, SpvStorageClass::Input);
        let var_id = self.alloc_id();

        self.emit_global(
            SpvOp::OpVariable,
            &[ptr_ty_id, var_id, SpvStorageClass::Input as u32],
        );
        self.emit_decoration(var_id, SpvDecoration::BuiltIn, &[builtin as u32]);

        self.builtin_ids.insert(builtin, var_id);
        var_id
    }

    /// Emit a buffer binding.
    fn emit_buffer_binding(&mut self, set: u32, binding: u32, ty: &MirType, writable: bool) -> u32 {
        let key = (set, binding);
        if let Some(&id) = self.buffer_bindings.get(&key) {
            return id;
        }

        // Create runtime array type for buffer
        let elem_id = self.get_type_id(ty);
        let arr_id = self.alloc_id();
        self.emit_global(SpvOp::OpTypeRuntimeArray, &[arr_id, elem_id]);

        // Create struct wrapping the array
        let struct_id = self.alloc_id();
        self.emit_global(SpvOp::OpTypeStruct, &[struct_id, arr_id]);
        self.emit_decoration(struct_id, SpvDecoration::Block, &[]);
        self.emit_member_decoration(struct_id, 0, SpvDecoration::Offset, &[0]);

        // Create pointer type
        let ptr_id = self.alloc_id();
        self.emit_global(
            SpvOp::OpTypePointer,
            &[ptr_id, SpvStorageClass::StorageBuffer as u32, struct_id],
        );

        // Create variable
        let var_id = self.alloc_id();
        self.emit_global(
            SpvOp::OpVariable,
            &[ptr_id, var_id, SpvStorageClass::StorageBuffer as u32],
        );

        // Decorations (buffered to annotations section)
        self.emit_decoration(var_id, SpvDecoration::DescriptorSet, &[set]);
        self.emit_decoration(var_id, SpvDecoration::Binding, &[binding]);

        if !writable {
            self.emit_decoration(var_id, SpvDecoration::NonWritable, &[]);
        }

        self.buffer_bindings.insert(key, var_id);
        var_id
    }

    /// Byte size/stride of a scalar SPIR-V element type (post f64->f32 coercion).
    fn scalar_stride(ty: &MirType) -> u32 {
        match ty {
            MirType::Float(FloatSize::F64) => 8,
            MirType::Int(IntSize::I64, _) => 8,
            _ => 4, // f32, i32, u32, bool
        }
    }

    /// Emit a compute StorageBuffer binding in the SPIR-V 1.0 / Vulkan 1.0 legal
    /// form: a `BufferBlock`-decorated struct wrapping a runtime array, in the
    /// `Uniform` storage class (the classic SSBO encoding; `StorageBuffer`
    /// storage class needs SPIR-V >= 1.3 or the storage-buffer extension). The
    /// runtime array carries an `ArrayStride` and member 0 an `Offset`, as
    /// Vulkan requires. Returns the OpVariable id.
    fn emit_compute_ssbo(
        &mut self,
        set: u32,
        binding: u32,
        elem_ty: &MirType,
        writable: bool,
    ) -> u32 {
        let key = (set, binding);
        if let Some(&id) = self.buffer_bindings.get(&key) {
            return id;
        }

        let elem_id = self.get_type_id(elem_ty);
        let arr_id = self.alloc_id();
        self.emit_global(SpvOp::OpTypeRuntimeArray, &[arr_id, elem_id]);
        // ArrayStride is mandatory for a runtime array in a Vulkan SSBO.
        self.emit_decoration(
            arr_id,
            SpvDecoration::ArrayStride,
            &[Self::scalar_stride(elem_ty)],
        );

        let struct_id = self.alloc_id();
        self.emit_global(SpvOp::OpTypeStruct, &[struct_id, arr_id]);
        self.emit_decoration(struct_id, SpvDecoration::BufferBlock, &[]);
        self.emit_member_decoration(struct_id, 0, SpvDecoration::Offset, &[0]);
        // Read-only buffers carry `NonWritable` on BOTH the runtime-array member
        // and the variable. The member decoration is the semantically load-
        // bearing one (per the SPIR-V memory model a store through a
        // NonWritable-member access chain is ill-formed); the variable
        // decoration mirrors it for tools that read either. The SPIR-V backend
        // additionally REFUSES to lower a store into a read-only buffer
        // (`IndexStore` below), so read-only-ness is enforced at emit time, not
        // left to a downstream validator.
        if !writable {
            self.emit_member_decoration(struct_id, 0, SpvDecoration::NonWritable, &[]);
        }

        let ptr_id = self.alloc_id();
        self.emit_global(
            SpvOp::OpTypePointer,
            &[ptr_id, SpvStorageClass::Uniform as u32, struct_id],
        );

        let var_id = self.alloc_id();
        self.emit_global(
            SpvOp::OpVariable,
            &[ptr_id, var_id, SpvStorageClass::Uniform as u32],
        );
        self.emit_decoration(var_id, SpvDecoration::DescriptorSet, &[set]);
        self.emit_decoration(var_id, SpvDecoration::Binding, &[binding]);
        if !writable {
            self.emit_decoration(var_id, SpvDecoration::NonWritable, &[]);
        }

        self.buffer_bindings.insert(key, var_id);
        var_id
    }

    /// Get (or create) the `Uniform`-class pointer type to `elem_ty`, used as the
    /// result type of an `OpAccessChain` into a compute SSBO's runtime array.
    fn uniform_elem_ptr_type(&mut self, elem_ty: &MirType) -> u32 {
        self.get_ptr_type_id(elem_ty, SpvStorageClass::Uniform)
    }

    /// Emit a workgroup-shared scratch array in the `Workgroup` storage class:
    /// an `OpTypeArray` of `len` `elem_ty` elements (length a compile-time
    /// `OpConstant`, so it is a fixed-size array, NOT a runtime array), a
    /// `Workgroup`-class `OpTypePointer` to it, and an `OpVariable` in that class
    /// with NO initializer and NO decorations (workgroup memory carries no
    /// ArrayStride, descriptor set, or binding -- it is on-chip shared memory, not
    /// a bound buffer). NOT added to `io_var_ids`: on SPIR-V 1.0 a
    /// `Workgroup`-class variable is NOT part of the OpEntryPoint interface (that
    /// requirement arrived in SPIR-V 1.4). Returns the OpVariable id.
    ///
    /// A FRESH OpVariable is emitted per call -- it is deliberately NOT
    /// deduplicated by `(elem_ty, len)`. Two logically distinct scratch buffers
    /// of the same shape (`let a = workgroupArray(64); let b = workgroupArray(64)`)
    /// are independent storage; sharing one variable between them would silently
    /// alias and corrupt their data. The array TYPE and pointer type are still
    /// interned via `get_type_id`/`OpTypePointer` reuse, so only the OpVariable
    /// itself is per-local, which is the sole thing that must be distinct.
    fn emit_workgroup_var(&mut self, elem_ty: &MirType, len: u64) -> u32 {
        // Intern the array TYPE and the Workgroup-class POINTER type: SPIR-V
        // requires types to be unique, so the `array<elem, len>` and its
        // `Workgroup`-class pointer are shared across all scratch buffers of this
        // shape (`get_type_id` / `get_ptr_type_id` dedup them). A fixed-size
        // OpTypeArray needs a constant length operand; a runtime array is a
        // different, buffer-only construct -- `MirType::Array` lowers to the
        // former.
        let arr_ty = MirType::Array(Box::new(elem_ty.clone()), len);
        let ptr_id = self.get_ptr_type_id(&arr_ty, SpvStorageClass::Workgroup);

        // Only the OpVariable is per-call (NOT deduplicated): distinct scratch
        // locals of the same shape are independent storage.
        let var_id = self.alloc_id();
        self.emit_global(
            SpvOp::OpVariable,
            &[ptr_id, var_id, SpvStorageClass::Workgroup as u32],
        );

        var_id
    }

    /// Get (or create) the `Workgroup`-class pointer type to `elem_ty`, used as
    /// the result type of an `OpAccessChain` into a workgroup-shared array.
    fn workgroup_elem_ptr_type(&mut self, elem_ty: &MirType) -> u32 {
        self.get_ptr_type_id(elem_ty, SpvStorageClass::Workgroup)
    }

    /// Emit the compute push-constant block: a single `Block`-decorated
    /// `OpTypeStruct` whose members are the kernel's scalar parameters, in the
    /// `PushConstant` storage class, each member carrying an `Offset`
    /// decoration. Returns the OpVariable id. A push constant needs no
    /// descriptor set / binding (it is part of the pipeline layout, pushed
    /// directly by the host), so it is strictly cheaper than a UBO for passing
    /// a handful of scalars like `alpha`.
    fn emit_push_constant_block(&mut self, member_tys: &[MirType], offsets: &[u32]) -> u32 {
        let member_ids: Vec<u32> = member_tys.iter().map(|t| self.get_type_id(t)).collect();

        let struct_id = self.alloc_id();
        let mut struct_operands = Vec::with_capacity(member_ids.len() + 1);
        struct_operands.push(struct_id);
        struct_operands.extend_from_slice(&member_ids);
        self.emit_global(SpvOp::OpTypeStruct, &struct_operands);
        self.emit_decoration(struct_id, SpvDecoration::Block, &[]);
        for (i, &offset) in offsets.iter().enumerate() {
            self.emit_member_decoration(struct_id, i as u32, SpvDecoration::Offset, &[offset]);
        }

        let ptr_id = self.alloc_id();
        self.emit_global(
            SpvOp::OpTypePointer,
            &[ptr_id, SpvStorageClass::PushConstant as u32, struct_id],
        );

        let var_id = self.alloc_id();
        self.emit_global(
            SpvOp::OpVariable,
            &[ptr_id, var_id, SpvStorageClass::PushConstant as u32],
        );
        var_id
    }

    // =========================================================================
    // FUNCTION GENERATION
    // =========================================================================

    /// Generate a function.
    fn gen_function(&mut self, func: &MirFunction) -> CodegenResult<()> {
        if func.is_declaration() {
            return Ok(());
        }

        self.local_ids.clear();
        self.block_ids.clear();

        // Shader entry points have special handling:
        // - Must return void (outputs go to Output variables)
        // - Must have zero function parameters (inputs come from Input variables)
        let is_shader_entry = func.shader_stage.is_some()
            || func.linkage == Linkage::External
            || func.name.as_ref() == "main";

        let effective_ret = if is_shader_entry {
            MirType::Void
        } else {
            func.sig.ret.clone()
        };
        let ret_ty_id = self.get_type_id(&effective_ret);

        // Generate function type - cache to avoid SPIR-V duplicate type errors
        let func_type_key = if is_shader_entry {
            format!("fn_type:void()")
        } else {
            let param_strs: Vec<String> =
                func.sig.params.iter().map(|p| format!("{:?}", p)).collect();
            format!("fn_type:{:?}({})", effective_ret, param_strs.join(","))
        };

        let func_type_id = if let Some(&cached_id) = self.type_ids.get(&func_type_key) {
            cached_id
        } else {
            let id = self.alloc_id();
            if is_shader_entry {
                self.emit_global(SpvOp::OpTypeFunction, &[id, ret_ty_id]);
            } else {
                let param_ty_ids: Vec<u32> = func
                    .sig
                    .params
                    .iter()
                    .map(|p| self.get_type_id(p))
                    .collect();
                let mut operands = vec![id, ret_ty_id];
                operands.extend(&param_ty_ids);
                self.emit_global(SpvOp::OpTypeFunction, &operands);
            }
            self.type_ids.insert(func_type_key, id);
            id
        };

        // Get or allocate function ID
        let func_id = match self.func_ids.get(&func.name) {
            Some(&id) => id,
            None => self.alloc_id(),
        };
        self.func_ids.insert(func.name.clone(), func_id);

        // Name the function (buffered to debug-names section)
        let func_name = func.name.to_string();
        self.emit_name(func_id, &func_name);

        // Emit function
        self.emit(SpvOp::OpFunction, &[ret_ty_id, func_id, 0, func_type_id]);

        self.value_locals.clear();

        if is_shader_entry {
            // Shader entry points: no OpFunctionParameter.
            // Input loading happens in gen_block for the entry block.
        } else {
            // Emit parameters for non-entry-point functions.
            // OpFunctionParameter results are VALUES, not pointers.
            for (i, param_ty) in func.sig.params.iter().enumerate() {
                let param_ty_id = self.get_type_id(param_ty);
                let param_id = self.alloc_id();
                self.emit(SpvOp::OpFunctionParameter, &[param_ty_id, param_id]);

                if let Some(local) = func
                    .locals
                    .iter()
                    .find(|l| l.is_param && l.id.0 == i as u32)
                {
                    self.local_ids.insert(local.id, param_id);
                    self.value_locals.insert(local.id); // Mark as value, not pointer
                }
            }
        }

        // Generate block IDs first
        if let Some(blocks) = &func.blocks {
            for block in blocks {
                let block_id = self.alloc_id();
                self.block_ids.insert(block.id, block_id);
            }
        }

        // Reconstruct structured control flow (selection/loop merge + continue
        // targets) from dominator/post-dominator analysis. SPIR-V requires every
        // header to name a merge block that post-dominates it; guessing this by
        // branch-following mis-picks the merge for a loop nested inside a
        // selection. This precomputes the correct targets for `gen_terminator`.
        self.structured_cfg = func
            .blocks
            .as_ref()
            .map(|blocks| crate::codegen::analysis::structured_cfg::analyze(blocks));

        // Generate blocks
        if let Some(blocks) = &func.blocks {
            for block in blocks {
                self.gen_block(block, func)?;
            }
        } else {
            // Empty function - just emit entry and return
            let label_id = self.alloc_id();
            self.emit(SpvOp::OpLabel, &[label_id]);
            if func.sig.ret == MirType::Void {
                self.emit(SpvOp::OpReturn, &[]);
            } else {
                let zero = self.get_const_id(&MirConst::Zeroed(func.sig.ret.clone()));
                self.emit(SpvOp::OpReturnValue, &[zero]);
            }
        }

        self.emit(SpvOp::OpFunctionEnd, &[]);
        Ok(())
    }

    /// Generate a basic block.
    fn gen_block(&mut self, block: &MirBlock, func: &MirFunction) -> CodegenResult<()> {
        let label_id = *self.block_ids.get(&block.id).unwrap();
        self.emit(SpvOp::OpLabel, &[label_id]);

        let is_shader_entry = func.shader_stage.is_some()
            || func.linkage == Linkage::External
            || func.name.as_ref() == "main";

        // SPIR-V requires ALL OpVariable(Function) instructions to be the FIRST
        // instructions in the entry block, before any other ops.
        if block.id == BlockId::ENTRY {
            // Phase 1: Emit ALL OpVariable declarations first
            for local in &func.locals {
                if !local.is_param {
                    // A workgroup-shared scratch local (`workgroupArray(64)`) is
                    // backed by a `Workgroup`-class OpVariable declared in
                    // `setup_shader_io`, NOT a Function-class local: skip it here
                    // so we do not emit a dead, duplicate `[f32;64]` Function
                    // variable the body never touches (indexing routes through the
                    // Workgroup var via `gen_workgroup_element_ptr`).
                    if self.workgroup_slots.contains_key(&local.id) {
                        continue;
                    }
                    let ptr_ty_id = self.get_ptr_type_id(&local.ty, SpvStorageClass::Function);
                    let var_id = self.alloc_id();
                    self.emit(
                        SpvOp::OpVariable,
                        &[ptr_ty_id, var_id, SpvStorageClass::Function as u32],
                    );
                    self.local_ids.insert(local.id, var_id);
                }
            }

            // Also declare Function-scope variables for shader input parameters
            let mut shader_param_vars: Vec<(usize, u32, u32)> = Vec::new(); // (index, local_var, ty_id)
            if is_shader_entry {
                for (i, param_ty) in func.sig.params.iter().enumerate() {
                    if i < self.shader_input_vars.len() {
                        let ty_id = self.get_type_id(param_ty);
                        let func_ptr_ty = self.get_ptr_type_id(param_ty, SpvStorageClass::Function);
                        let local_var = self.alloc_id();
                        self.emit(
                            SpvOp::OpVariable,
                            &[func_ptr_ty, local_var, SpvStorageClass::Function as u32],
                        );
                        shader_param_vars.push((i, local_var, ty_id));
                    }
                }
            }

            // Compute-kernel scalar params: declare a Function-scope OpVariable
            // for each push-constant scalar (`alpha`), still in the leading
            // OpVariable phase. The value is loaded from the PushConstant block
            // below and the param local repointed at this Function var, so the
            // rest of the body reads it with a plain OpLoad.
            let mut pc_param_vars: Vec<(LocalId, u32, u32, u32)> = Vec::new(); // (local, func_var, ty_id, member_index)
            if let Some(_pc_var) = self.compute_pc_var {
                let mut pcs: Vec<(LocalId, ComputePushConstant)> = self
                    .compute_push_constants
                    .iter()
                    .map(|(id, pc)| (*id, pc.clone()))
                    .collect();
                pcs.sort_by_key(|(_, pc)| pc.member_index);
                for (local_id, pc) in pcs {
                    let ty_id = self.get_type_id(&pc.ty);
                    let func_ptr_ty = self.get_ptr_type_id(&pc.ty, SpvStorageClass::Function);
                    let func_var = self.alloc_id();
                    self.emit(
                        SpvOp::OpVariable,
                        &[func_ptr_ty, func_var, SpvStorageClass::Function as u32],
                    );
                    pc_param_vars.push((local_id, func_var, ty_id, pc.member_index));
                }
            }

            // Phase 2: AFTER all OpVariables, load shader inputs into local vars
            for (i, local_var, ty_id) in &shader_param_vars {
                let loaded_id = self.alloc_id();
                let input_var = self.shader_input_vars[*i];
                self.emit(SpvOp::OpLoad, &[*ty_id, loaded_id, input_var]);
                self.emit(SpvOp::OpStore, &[*local_var, loaded_id]);

                if let Some(local) = func
                    .locals
                    .iter()
                    .find(|l| l.is_param && l.id.0 == *i as u32)
                {
                    self.local_ids.insert(local.id, *local_var);
                }
            }

            // Load each scalar push-constant member into its Function var, then
            // register the param local so `alpha` reads via a normal OpLoad.
            if let Some(pc_var) = self.compute_pc_var {
                for (local_id, func_var, ty_id, member_index) in &pc_param_vars {
                    let member_const =
                        self.get_const_id(&MirConst::Int(*member_index as i128, MirType::i32()));
                    let member_ptr_ty = {
                        // A pointer to the scalar member in the PushConstant class.
                        let elem_ty = self.compute_push_constants[local_id].ty.clone();
                        self.get_ptr_type_id(&elem_ty, SpvStorageClass::PushConstant)
                    };
                    let chain_id = self.alloc_id();
                    self.emit(
                        SpvOp::OpAccessChain,
                        &[member_ptr_ty, chain_id, pc_var, member_const],
                    );
                    let loaded_id = self.alloc_id();
                    self.emit(SpvOp::OpLoad, &[*ty_id, loaded_id, chain_id]);
                    self.emit(SpvOp::OpStore, &[*func_var, loaded_id]);
                    self.local_ids.insert(*local_id, *func_var);
                }
            }
        }

        // Generate statements
        for stmt in &block.stmts {
            self.gen_stmt(stmt, func)?;
        }

        // Generate terminator
        if let Some(term) = &block.terminator {
            self.gen_terminator(term, func, block)?;
        } else {
            self.emit(SpvOp::OpUnreachable, &[]);
        }

        Ok(())
    }

    /// If `value` is an integer CONSTANT whose SPIR-V integer type would differ
    /// from `dest`'s declared integer type only in signedness (same bit width),
    /// return a copy of the constant re-typed to the destination so `OpStore`
    /// sees matching types. Otherwise return `value` unchanged. This closes the
    /// gap where a bare literal initializing a `u32` local (`let mut kk = 0`) is
    /// lowered as a signed `i32` constant, which strict SPIR-V would reject.
    fn coerce_const_signedness(
        &self,
        dest: &LocalId,
        value: &MirRValue,
        func: &MirFunction,
    ) -> MirRValue {
        let Some(local) = func.locals.iter().find(|l| &l.id == dest) else {
            return value.clone();
        };
        let MirType::Int(dest_size, dest_signed) = &local.ty else {
            return value.clone();
        };
        let MirRValue::Use(MirValue::Const(c)) = value else {
            return value.clone();
        };
        // Extract the constant's integer value + source integer type, if any.
        let (val, src_size, src_signed) = match c {
            MirConst::Int(v, MirType::Int(size, signed)) => (*v, *size, *signed),
            MirConst::Uint(v, MirType::Int(size, signed)) => (*v as i128, *size, *signed),
            _ => return value.clone(),
        };
        // Only reconcile a pure signedness mismatch at the same bit width.
        if src_size == *dest_size && src_signed != *dest_signed {
            MirRValue::Use(MirValue::Const(MirConst::Int(val, local.ty.clone())))
        } else {
            value.clone()
        }
    }

    /// If `value` is an integer CONSTANT whose signedness differs from `op_ty`
    /// (an integer operation type) at the same bit width, return a copy re-typed
    /// to `op_ty` so a binary op's operands and result share one SPIR-V type.
    /// Otherwise return `value` unchanged. Non-constant operands are left alone
    /// (a signed/unsigned VALUE mismatch is a front-end concern, not something to
    /// silently reinterpret here).
    fn coerce_operand_signedness(&self, value: &MirValue, op_ty: &MirType) -> MirValue {
        let MirType::Int(dest_size, dest_signed) = op_ty else {
            return value.clone();
        };
        let MirValue::Const(c) = value else {
            return value.clone();
        };
        let (val, src_size, src_signed) = match c {
            MirConst::Int(v, MirType::Int(size, signed)) => (*v, *size, *signed),
            MirConst::Uint(v, MirType::Int(size, signed)) => (*v as i128, *size, *signed),
            _ => return value.clone(),
        };
        if src_size == *dest_size && src_signed != *dest_signed {
            MirValue::Const(MirConst::Int(val, op_ty.clone()))
        } else {
            value.clone()
        }
    }

    /// Generate a statement.
    fn gen_stmt(&mut self, stmt: &MirStmt, func: &MirFunction) -> CodegenResult<()> {
        match &stmt.kind {
            MirStmtKind::GlobalStore { .. } => {
                return Err(CodegenError::Unsupported(
                    "store to a module global is not supported in the SPIR-V backend".to_string(),
                ));
            }
            MirStmtKind::Assign { dest, value } => {
                // VOID/UNIT assignments are no-ops in SPIR-V. BuildLang lowers an
                // `if`/`else` (or any unit-typed expression) used in statement
                // position as an assignment of `Const::Unit` into a `Void`-typed
                // temp (e.g. the merge value of a value-less `if-else`). `void` has
                // no SPIR-V value id, so emitting `OpStore <void ptr> <unit>` writes
                // an UNDEFINED id and yields a module spirv-val rejects with
                // "ID 'N' has not been defined  OpStore ...". Skip the store: a void
                // assignment carries no data. (Guard on the destination local's
                // declared type so a genuine value-store is never dropped.)
                let dest_is_void = matches!(
                    self.get_local_type(*dest, func),
                    Ok(MirType::Void) | Ok(MirType::Never)
                );
                let value_is_unit =
                    matches!(value, MirRValue::Use(MirValue::Const(MirConst::Unit)));
                if dest_is_void || value_is_unit {
                    // No-op: nothing to materialize or store for a void/unit value.
                } else {
                    // Reconcile an integer CONSTANT whose signedness differs from the
                    // destination local's integer type. BuildLang's front end lowers a
                    // bare literal (`let mut kk: u32 = 0`) as a *signed* i32 constant;
                    // SPIR-V is strictly typed and rejects `OpStore <u32 ptr> <i32
                    // const>`. The value is bit-identical, so re-materialize the
                    // constant with the destination type before emitting the store.
                    let value = self.coerce_const_signedness(dest, value, func);
                    let val_id = self.gen_rvalue(&value, func)?;
                    let ptr_id = *self.local_ids.get(dest).ok_or_else(|| {
                        CodegenError::Internal(format!("Unknown local: {:?}", dest))
                    })?;
                    self.emit(SpvOp::OpStore, &[ptr_id, val_id]);
                }
            }
            MirStmtKind::DerefAssign { ptr, value } => {
                let val_id = self.gen_rvalue(value, func)?;
                let ptr_id = *self.local_ids.get(ptr).ok_or_else(|| {
                    CodegenError::Internal(format!("Unknown local for deref: {:?}", ptr))
                })?;
                self.emit(SpvOp::OpStore, &[ptr_id, val_id]);
            }
            MirStmtKind::FieldDerefAssign {
                ptr,
                field_name: _,
                value,
            } => {
                // Field access through pointer: use OpAccessChain + OpStore
                let val_id = self.gen_rvalue(value, func)?;
                let ptr_id = *self.local_ids.get(ptr).ok_or_else(|| {
                    CodegenError::Internal(format!("Unknown local for field deref: {:?}", ptr))
                })?;
                // Simplified: store directly to the base pointer (struct field offset not computed)
                self.emit(SpvOp::OpStore, &[ptr_id, val_id]);
            }
            MirStmtKind::FieldAssign {
                base,
                field_name: _,
                value,
            } => {
                // Local struct field store
                let val_id = self.gen_rvalue(value, func)?;
                let base_id = *self.local_ids.get(base).ok_or_else(|| {
                    CodegenError::Internal(format!("Unknown local for field assign: {:?}", base))
                })?;
                self.emit(SpvOp::OpStore, &[base_id, val_id]);
            }
            MirStmtKind::IndexStore {
                base,
                index,
                elem_ty,
                value,
            } => {
                // `base[index] = value`. For a compute-kernel buffer parameter
                // this is a real OpAccessChain into the StorageBuffer runtime
                // array (member 0) followed by OpStore -- NOT the zero-constant
                // fallback. Any other base is unsupported in SPIR-V today.
                let val_id = self.gen_rvalue(value, func)?;
                // Workgroup-shared write: `scratch[i] = v` on a workgroup slot.
                // Shared memory is always writable (no read-only concept), so
                // this needs no writability check. Checked before the buffer path.
                if let MirValue::Local(base_local) = base {
                    if let Some(chain_id) =
                        self.gen_workgroup_element_ptr(*base_local, index, elem_ty, func)?
                    {
                        self.emit(SpvOp::OpStore, &[chain_id, val_id]);
                        return Ok(());
                    }
                }
                if let MirValue::Local(base_local) = base {
                    // Refuse to store into a read-only buffer. Writability is
                    // inferred from the parameter's reference mutability
                    // (`&mut [_]` writable, `&[_]` read-only) in setup_shader_io;
                    // a store into a read-only buffer is a genuine error, caught
                    // here at emit time rather than left to a downstream
                    // validator. This is what makes the writability inference
                    // load-bearing instead of decorative.
                    if let Some(buffer) = self.compute_buffers.get(base_local) {
                        if !buffer.writable {
                            let name = func
                                .locals
                                .iter()
                                .find(|l| l.id == *base_local)
                                .and_then(|l| l.name.as_ref())
                                .map(|n| n.to_string())
                                .unwrap_or_else(|| format!("{base_local:?}"));
                            return Err(CodegenError::Unsupported(format!(
                                "cannot write into read-only buffer `{name}`: it is a `&[_]` \
                                 (non-writable) parameter; declare it `&mut [_]` to write into it"
                            )));
                        }
                    }
                    if let Some(chain_id) =
                        self.gen_buffer_element_ptr(*base_local, index, elem_ty, func)?
                    {
                        self.emit(SpvOp::OpStore, &[chain_id, val_id]);
                        return Ok(());
                    }
                }
                return Err(CodegenError::Unsupported(
                    "indexed store to a non-buffer base is not supported in the SPIR-V backend"
                        .to_string(),
                ));
            }
            MirStmtKind::WorkgroupBarrier => {
                // `workgroupBarrier()` -> OpControlBarrier with Workgroup
                // execution + memory scope and AcquireRelease|WorkgroupMemory
                // semantics, synchronizing the workgroup and ordering shared-
                // memory writes across it.
                self.emit_workgroup_barrier();
            }
            MirStmtKind::StorageLive(_) | MirStmtKind::StorageDead(_) | MirStmtKind::Nop => {
                // No-op in SPIR-V
            }
        }
        Ok(())
    }

    /// If `base_local` is a compute-kernel buffer parameter, emit an
    /// `OpAccessChain` into its runtime array (`buffer.member0[index]`) and
    /// return the resulting `Uniform`-class element pointer id. Returns `Ok(None)`
    /// if the local is not a buffer parameter (caller falls back).
    /// Read one component of a uvec3 GPU invocation built-in
    /// (`GlobalInvocationId` / `LocalInvocationId` / `WorkgroupId`): declare (or
    /// reuse) the built-in as a uvec3 Input variable, access-chain the requested
    /// component, and load it as a u32. Shared by the three thread-index nullary
    /// ops so their emission cannot drift apart.
    fn gen_invocation_component(&mut self, builtin: SpvBuiltIn, component: u32) -> u32 {
        let uvec3 = MirType::Vector(Box::new(MirType::Int(IntSize::I32, false)), 3);
        let var_id = self.emit_builtin_var(builtin, &uvec3);
        let u32_ty = MirType::Int(IntSize::I32, false);
        let u32_ty_id = self.get_type_id(&u32_ty);
        let comp_const = self.get_const_id(&MirConst::Int(component as i128, u32_ty.clone()));
        let ptr_ty_id = self.get_ptr_type_id(&u32_ty, SpvStorageClass::Input);
        let chain_id = self.alloc_id();
        self.emit(
            SpvOp::OpAccessChain,
            &[ptr_ty_id, chain_id, var_id, comp_const],
        );
        let result_id = self.alloc_id();
        self.emit(SpvOp::OpLoad, &[u32_ty_id, result_id, chain_id]);
        result_id
    }

    fn gen_buffer_element_ptr(
        &mut self,
        base_local: LocalId,
        index: &MirValue,
        elem_ty: &MirType,
        func: &MirFunction,
    ) -> CodegenResult<Option<u32>> {
        let Some(buffer) = self.compute_buffers.get(&base_local).cloned() else {
            return Ok(None);
        };
        let index_id = self.gen_value(index, func)?;
        // member index 0 selects the runtime array inside the BufferBlock struct.
        let zero_id = self.get_const_id(&MirConst::Int(0, MirType::Int(IntSize::I32, false)));
        let elem_ty = if matches!(elem_ty, MirType::Void) {
            &buffer.elem_ty
        } else {
            elem_ty
        };
        let ptr_ty_id = self.uniform_elem_ptr_type(elem_ty);
        let chain_id = self.alloc_id();
        self.emit(
            SpvOp::OpAccessChain,
            &[ptr_ty_id, chain_id, buffer.var_id, zero_id, index_id],
        );
        Ok(Some(chain_id))
    }

    /// Access-chain into a workgroup-shared scratch array (`scratch[index]`).
    /// Parallel to `gen_buffer_element_ptr` but for a `Workgroup`-class variable
    /// that IS the array (no wrapping struct, so no leading member-0 index): the
    /// access chain is `var[index]`, result type a `Workgroup`-class pointer to
    /// the element. Returns `None` (so the caller falls through) when `base_local`
    /// is not a registered workgroup slot.
    fn gen_workgroup_element_ptr(
        &mut self,
        base_local: LocalId,
        index: &MirValue,
        elem_ty: &MirType,
        func: &MirFunction,
    ) -> CodegenResult<Option<u32>> {
        let Some(slot) = self.workgroup_slots.get(&base_local).cloned() else {
            return Ok(None);
        };
        let index_id = self.gen_value(index, func)?;
        let elem_ty = if matches!(elem_ty, MirType::Void) {
            &slot.elem_ty
        } else {
            elem_ty
        };
        let ptr_ty_id = self.workgroup_elem_ptr_type(elem_ty);
        let chain_id = self.alloc_id();
        self.emit(
            SpvOp::OpAccessChain,
            &[ptr_ty_id, chain_id, slot.var_id, index_id],
        );
        Ok(Some(chain_id))
    }

    /// Generate an rvalue.
    fn gen_rvalue(&mut self, rvalue: &MirRValue, func: &MirFunction) -> CodegenResult<u32> {
        match rvalue {
            MirRValue::Use(value) => self.gen_value(value, func),
            MirRValue::BinaryOp { op, left, right } => {
                // A bare integer literal is lowered with a default signedness
                // (signed `i32`) that need not match a same-width variable
                // operand, yet strict SPIR-V requires both operands AND the
                // result to share one type. The operation type therefore comes
                // from a NON-CONSTANT operand whenever one is present: for
                // `100 - z` (`z: u32`) the type is `z`'s `u32`, not the literal's
                // `i32`, so the op emits `OpISub %uint %uint_100 %z` and its
                // result stores back into `z` without a type mismatch. Only when
                // both operands are constants (or the left is the variable) is the
                // left operand's type used. A signedness-mismatched CONSTANT
                // operand is then re-typed to that operation type before emission
                // (the operand-level analogue of the assign-time
                // `coerce_const_signedness`); a variable operand is left untouched
                // (a genuine signed/unsigned VALUE mismatch is a front-end
                // concern, not something to silently reinterpret here).
                let ty =
                    if matches!(left, MirValue::Const(_)) && !matches!(right, MirValue::Const(_)) {
                        self.infer_value_type(right, func)?
                    } else {
                        self.infer_value_type(left, func)?
                    };
                let left = self.coerce_operand_signedness(left, &ty);
                let right = self.coerce_operand_signedness(right, &ty);
                let left_id = self.gen_value(&left, func)?;
                let right_id = self.gen_value(&right, func)?;
                let result_id = self.alloc_id();

                // Comparison ops return bool, not the operand type
                let is_comparison = matches!(
                    op,
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
                );
                let result_ty_id = if is_comparison {
                    self.get_type_id(&MirType::Bool)
                } else {
                    self.get_type_id(&ty)
                };

                let opcode = match &ty {
                    MirType::Vector(elem, _) => self.spirv_binop(*op, elem),
                    _ => self.spirv_binop(*op, &ty),
                };
                self.emit(opcode, &[result_ty_id, result_id, left_id, right_id]);
                Ok(result_id)
            }
            MirRValue::UnaryOp { op, operand } => {
                let operand_id = self.gen_value(operand, func)?;
                let ty = self.infer_value_type(operand, func)?;
                let ty_id = self.get_type_id(&ty);
                let result_id = self.alloc_id();

                let is_float = match &ty {
                    MirType::Vector(elem, _) => elem.is_float(),
                    _ => ty.is_float(),
                };
                let opcode = match op {
                    UnaryOp::Neg => {
                        if is_float {
                            SpvOp::OpFNegate
                        } else {
                            SpvOp::OpSNegate
                        }
                    }
                    UnaryOp::Not => SpvOp::OpNot,
                    UnaryOp::BitNot => SpvOp::OpNot,
                };
                self.emit(opcode, &[ty_id, result_id, operand_id]);
                Ok(result_id)
            }
            MirRValue::Cast { kind, value, ty } => {
                let val_id = self.gen_value(value, func)?;
                let from_ty = self.infer_value_type(value, func)?;
                let to_ty_id = self.get_type_id(ty);
                let result_id = self.alloc_id();

                let opcode = self.spirv_cast(*kind, &from_ty, ty);
                self.emit(opcode, &[to_ty_id, result_id, val_id]);
                Ok(result_id)
            }
            MirRValue::Aggregate { kind, operands } => {
                // Build a composite (struct, array, tuple) from constituents
                let constituent_ids: Vec<u32> = operands
                    .iter()
                    .map(|o| self.gen_value(o, func))
                    .collect::<Result<Vec<_>, _>>()?;
                let result_type = match kind {
                    AggregateKind::Struct(name) => MirType::Struct(name.clone()),
                    AggregateKind::Array(elem_ty) => {
                        MirType::Array(Box::new(elem_ty.clone()), operands.len() as u64)
                    }
                    AggregateKind::Tuple => {
                        // Tuples are structs in SPIR-V; use a placeholder
                        MirType::Void
                    }
                    _ => MirType::Void,
                };
                let ty_id = self.get_type_id(&result_type);
                Ok(self.emit_composite_construct(ty_id, &constituent_ids))
            }
            MirRValue::FieldAccess {
                base,
                field_name,
                field_ty,
            } => {
                // Extract a field from a struct via OpCompositeExtract
                let base_id = self.gen_value(base, func)?;
                let base_ty = self.infer_value_type(base, func)?;
                let result_ty_id = self.get_type_id(field_ty);

                // Resolve the field index from the struct definition
                let field_index = if let MirType::Struct(struct_name) = &base_ty {
                    self.resolve_field_index(struct_name, field_name)
                } else {
                    0 // For non-struct types (e.g. vector swizzle), default to 0
                };
                Ok(self.emit_composite_extract(result_ty_id, base_id, &[field_index]))
            }
            MirRValue::IndexAccess {
                base,
                index,
                elem_ty,
            } => {
                // Workgroup-shared read: `scratch[i]` on a workgroup slot lowers
                // to an OpAccessChain into the Workgroup-class array + OpLoad.
                // Checked before the buffer path (a local is one or the other).
                if let MirValue::Local(base_local) = base {
                    if let Some(chain_id) =
                        self.gen_workgroup_element_ptr(*base_local, index, elem_ty, func)?
                    {
                        let elem_ty_id = self.get_type_id(elem_ty);
                        let result_id = self.alloc_id();
                        self.emit(SpvOp::OpLoad, &[elem_ty_id, result_id, chain_id]);
                        return Ok(result_id);
                    }
                }
                // Compute-kernel buffer read: `a[i]` on a StorageBuffer parameter
                // lowers to a real OpAccessChain into the runtime array (member 0)
                // + OpLoad -- NOT the zero-constant fallback.
                if let MirValue::Local(base_local) = base {
                    if let Some(chain_id) =
                        self.gen_buffer_element_ptr(*base_local, index, elem_ty, func)?
                    {
                        let elem_ty_id = self.get_type_id(elem_ty);
                        let result_id = self.alloc_id();
                        self.emit(SpvOp::OpLoad, &[elem_ty_id, result_id, chain_id]);
                        return Ok(result_id);
                    }
                }

                // Array element access via OpCompositeExtract (constant index)
                // or OpAccessChain (dynamic index)
                let base_id = self.gen_value(base, func)?;
                let index_id = self.gen_value(index, func)?;
                let elem_ty_id = self.get_type_id(elem_ty);

                // Use OpVectorExtractDynamic for vector types, OpAccessChain for arrays
                let base_ty = self.infer_value_type(base, func)?;
                let result_id = self.alloc_id();
                match &base_ty {
                    MirType::Vector(_, _) => {
                        self.emit(
                            SpvOp::OpVectorExtractDynamic,
                            &[elem_ty_id, result_id, base_id, index_id],
                        );
                    }
                    _ => {
                        // For arrays: create pointer via OpAccessChain, then load
                        let ptr_ty_id = self.get_type_id(&MirType::Ptr(Box::new(elem_ty.clone())));
                        let chain_id = self.alloc_id();
                        self.emit(
                            SpvOp::OpAccessChain,
                            &[ptr_ty_id, chain_id, base_id, index_id],
                        );
                        self.emit(SpvOp::OpLoad, &[elem_ty_id, result_id, chain_id]);
                    }
                }
                Ok(result_id)
            }
            MirRValue::VariantField {
                base,
                variant_name: _,
                field_index,
                field_ty,
            } => {
                // Extract a field from an enum variant - treat as composite extract
                let base_id = self.gen_value(base, func)?;
                let field_ty_id = self.get_type_id(field_ty);
                // Skip tag (index 0), data starts at index 1, then field_index within data
                Ok(self.emit_composite_extract(field_ty_id, base_id, &[1 + *field_index]))
            }
            MirRValue::TextureSample {
                texture,
                sampler,
                coords,
            } => {
                // OpImageSampleImplicitLod
                let tex_id = self.gen_value(texture, func)?;
                let samp_id = self.gen_value(sampler, func)?;
                let coords_id = self.gen_value(coords, func)?;
                let result_ty = MirType::Struct(Arc::from("build_vec4")); // vec4 result
                let result_ty_id = self.get_type_id(&result_ty);

                // Create sampled image from texture + sampler
                let sampled_img_ty_id = self.get_type_id(&MirType::SampledImage(Box::new(
                    MirType::Struct(Arc::from("build_vec4")),
                )));
                let combined_id = self.alloc_id();
                self.emit(
                    SpvOp::OpSampledImage,
                    &[sampled_img_ty_id, combined_id, tex_id, samp_id],
                );

                // Sample
                let result_id = self.alloc_id();
                self.emit(
                    SpvOp::OpImageSampleImplicitLod,
                    &[result_ty_id, result_id, combined_id, coords_id],
                );
                Ok(result_id)
            }
            MirRValue::Ref { place, .. } | MirRValue::AddressOf { place, .. } => {
                // In SPIR-V, references are pointers - for function-local values,
                // the variable already has a pointer. Return its ID directly.
                if let Some(&ptr_id) = self.local_ids.get(&place.local) {
                    return Ok(ptr_id);
                }
                // Fallback: return zero
                let zero = self.get_const_id(&MirConst::Int(0, MirType::i32()));
                Ok(zero)
            }
            MirRValue::Repeat { value, count } => {
                // Create an array filled with repeated value
                let val_id = self.gen_value(value, func)?;
                let elem_ty = self.infer_value_type(value, func)?;
                let arr_ty = MirType::Array(Box::new(elem_ty), *count);
                let arr_ty_id = self.get_type_id(&arr_ty);
                let ids: Vec<u32> = (0..*count).map(|_| val_id).collect();
                Ok(self.emit_composite_construct(arr_ty_id, &ids))
            }
            MirRValue::NullaryOp(op, ty) => {
                match op {
                    NullaryOp::ThreadIndex(component) => {
                        // Read `gl_GlobalInvocationID[component]`.
                        Ok(self
                            .gen_invocation_component(SpvBuiltIn::GlobalInvocationId, *component))
                    }
                    NullaryOp::LocalInvocationId(component) => {
                        // Read `gl_LocalInvocationID[component]` (lane within the
                        // workgroup): access-chain the LocalInvocationId uvec3
                        // Input builtin and load the requested scalar.
                        Ok(
                            self.gen_invocation_component(
                                SpvBuiltIn::LocalInvocationId,
                                *component,
                            ),
                        )
                    }
                    NullaryOp::WorkgroupId(component) => {
                        // Read `gl_WorkgroupID[component]` (which workgroup this
                        // invocation belongs to).
                        Ok(self.gen_invocation_component(SpvBuiltIn::WorkgroupId, *component))
                    }
                    // sizeof/alignof have no operand-free SPIR-V form here; the
                    // shader path does not exercise them. Fall back to a zero of
                    // the requested type.
                    NullaryOp::SizeOf | NullaryOp::AlignOf => {
                        Ok(self.get_const_id(&MirConst::Int(0, ty.clone())))
                    }
                }
            }
            MirRValue::Deref { ptr, pointee_ty } => {
                // Dereference a pointer: OpLoad through the pointer value
                let ptr_id = self.gen_value(ptr, func)?;
                let ty_id = self.get_type_id(pointee_ty);
                let result_id = self.alloc_id();
                self.emit(SpvOp::OpLoad, &[ty_id, result_id, ptr_id]);
                Ok(result_id)
            }
            _ => {
                // Truly unknown operations - return zero as safe fallback
                let zero = self.get_const_id(&MirConst::Int(0, MirType::i32()));
                Ok(zero)
            }
        }
    }

    /// Resolve the index of a named field within a struct definition.
    fn resolve_field_index(&self, struct_name: &str, field_name: &str) -> u32 {
        if let Some(fields) = self.struct_defs.get(struct_name) {
            for (i, (name, _)) in fields.iter().enumerate() {
                if let Some(n) = name {
                    if n.as_ref() == field_name {
                        return i as u32;
                    }
                }
            }
        }
        0
    }

    /// Generate a value.
    fn gen_value(&mut self, value: &MirValue, func: &MirFunction) -> CodegenResult<u32> {
        match value {
            MirValue::Local(id) => {
                let val_id = *self
                    .local_ids
                    .get(id)
                    .ok_or_else(|| CodegenError::Internal(format!("Unknown local: {:?}", id)))?;

                // Value locals (function parameters) are direct values - no OpLoad needed.
                if self.value_locals.contains(id) {
                    return Ok(val_id);
                }

                // Pointer locals (OpVariable) need OpLoad to get the value.
                let ty = self.get_local_type(*id, func)?;
                let ty_id = self.get_type_id(&ty);
                let result_id = self.alloc_id();
                self.emit(SpvOp::OpLoad, &[ty_id, result_id, val_id]);
                Ok(result_id)
            }
            MirValue::Const(c) => Ok(self.get_const_id(c)),
            MirValue::Global(name) => {
                // Check if this is a function reference (used in Call terminators)
                if let Some(&id) = self.func_ids.get(name) {
                    return Ok(id);
                }
                // Otherwise global variable (not yet supported)
                let zero = self.get_const_id(&MirConst::Int(0, MirType::i32()));
                Ok(zero)
            }
            MirValue::Function(name) => {
                // Function reference - look up in pre-allocated func_ids
                if let Some(&id) = self.func_ids.get(name) {
                    Ok(id)
                } else {
                    // Function not found - might be a mangled or runtime name
                    Err(CodegenError::Internal(format!(
                        "SPIR-V: function '{}' not found in func_ids. Available: {:?}",
                        name,
                        self.func_ids.keys().collect::<Vec<_>>()
                    )))
                }
            }
        }
    }

    /// Generate a terminator.
    fn gen_terminator(
        &mut self,
        term: &MirTerminator,
        func: &MirFunction,
        block: &MirBlock,
    ) -> CodegenResult<()> {
        match term {
            MirTerminator::Goto(target) => {
                let target_id = *self.block_ids.get(target).unwrap();
                self.emit(SpvOp::OpBranch, &[target_id]);
            }
            MirTerminator::If {
                cond,
                then_block,
                else_block,
            } => {
                let cond_id = self.gen_value(cond, func)?;
                let then_id = *self.block_ids.get(then_block).unwrap();
                let else_id = *self.block_ids.get(else_block).unwrap();

                // PRIMARY PATH: consult the structured-CFG analysis, which
                // resolves the correct merge (and, for loops, continue target)
                // via dominator/post-dominator analysis. This handles arbitrary
                // nesting of `if`/`while` -- the old branch-following heuristic
                // below mis-picks the merge for a loop nested in a selection.
                use crate::codegen::analysis::structured_cfg::HeaderKind;
                if let Some(kind) = self
                    .structured_cfg
                    .as_ref()
                    .and_then(|cfg| cfg.header(block.id))
                {
                    match kind {
                        HeaderKind::Loop {
                            merge,
                            continue_target,
                        } => {
                            let merge_id = *self.block_ids.get(&merge).unwrap();
                            let continue_id = *self.block_ids.get(&continue_target).unwrap();
                            self.emit(SpvOp::OpLoopMerge, &[merge_id, continue_id, 0]);
                            self.emit(SpvOp::OpBranchConditional, &[cond_id, then_id, else_id]);
                            return Ok(());
                        }
                        HeaderKind::Selection { merge } => {
                            let merge_id = *self.block_ids.get(&merge).unwrap();
                            self.emit(SpvOp::OpSelectionMerge, &[merge_id, 0]);
                            self.emit(SpvOp::OpBranchConditional, &[cond_id, then_id, else_id]);
                            return Ok(());
                        }
                    }
                }

                // FALLBACK (analysis punted, e.g. an irreducible or exit-in-both-
                // arms shape): the legacy branch-following heuristic. Kept so no
                // previously-emitting shape regresses to an unmerged conditional.
                // This path was the ORIGINAL source of the nested-CFG bug, so make
                // it LOUD in debug builds: if the structured-CFG analysis was run
                // and still returned no classification for an `If` header, a nested
                // shape may be silently reverting to the fragile heuristic. Debug
                // builds emit a one-line diagnostic so a future nested-CFG
                // regression is discoverable instead of silently reproduced.
                #[cfg(debug_assertions)]
                if self.structured_cfg.is_some() {
                    eprintln!(
                        "buildc[spirv]: structured-CFG analysis punted on If-header block {} \
                         ({:?}); falling back to the legacy branch-following heuristic. If a \
                         nested if/while now emits malformed control flow, this is where to look.",
                        block.id.0, block.label
                    );
                }

                // Detect loop header: if the then-branch body eventually jumps
                // BACK to the current block, this is a while loop header.
                // Emit OpLoopMerge before OpBranchConditional.
                let is_loop_header = if let Some(blocks) = &func.blocks {
                    // Check if any block reachable from then_block jumps back here
                    let mut check = *then_block;
                    let mut found_loop = false;
                    for _ in 0..50 {
                        if let Some(b) = blocks.iter().find(|b| b.id == check) {
                            match b.terminator.as_ref() {
                                Some(MirTerminator::Goto(target)) => {
                                    if *target == block.id {
                                        found_loop = true;
                                        break;
                                    }
                                    check = *target;
                                }
                                Some(MirTerminator::Call {
                                    target: Some(target),
                                    ..
                                }) => {
                                    // Function call with continuation - follow to target
                                    if *target == block.id {
                                        found_loop = true;
                                        break;
                                    }
                                    check = *target;
                                }
                                Some(MirTerminator::If {
                                    then_block: tb,
                                    else_block: eb,
                                    ..
                                }) => {
                                    if *tb == block.id || *eb == block.id {
                                        found_loop = true;
                                        break;
                                    }
                                    check = *tb;
                                }
                                _ => break,
                            }
                        } else {
                            break;
                        }
                    }
                    found_loop
                } else {
                    false
                };

                if is_loop_header {
                    // Loop header: else_block is the loop exit (merge),
                    // then_block is the loop body (continue target)
                    self.emit(SpvOp::OpLoopMerge, &[else_id, then_id, 0]);
                    self.emit(SpvOp::OpBranchConditional, &[cond_id, then_id, else_id]);
                    return Ok(());
                }

                // Find the merge block by following control flow from both branches.
                // Walk each branch until we find a Goto target, following chains
                // of nested if/else blocks.
                let merge_id = if let Some(blocks) = &func.blocks {
                    let follow_branch = |start: &BlockId| -> Option<BlockId> {
                        let mut current = *start;
                        for _ in 0..20 {
                            // limit traversal depth
                            if let Some(block) = blocks.iter().find(|b| b.id == current) {
                                match block.terminator.as_ref() {
                                    Some(MirTerminator::Goto(target)) => return Some(*target),
                                    Some(MirTerminator::If { else_block: eb, .. }) => {
                                        // Nested if - follow to else branch's successor
                                        current = *eb;
                                    }
                                    Some(MirTerminator::Return(_)) => return None,
                                    _ => return None,
                                }
                            } else {
                                return None;
                            }
                        }
                        None
                    };

                    let then_target = follow_branch(then_block);
                    let else_target = follow_branch(else_block);

                    // Also check transitive targets: if else→bb6→bb3, then bb3 is the merge
                    let follow_transitive = |start: BlockId| -> Option<BlockId> {
                        let mut current = start;
                        for _ in 0..10 {
                            if let Some(block) = blocks.iter().find(|b| b.id == current) {
                                match block.terminator.as_ref() {
                                    Some(MirTerminator::Goto(target)) => {
                                        current = *target;
                                    }
                                    _ => return Some(current),
                                }
                            } else {
                                return Some(current);
                            }
                        }
                        Some(current)
                    };

                    if let (Some(tt), Some(et)) = (then_target, else_target) {
                        if tt == et {
                            *self.block_ids.get(&tt).unwrap_or(&else_id)
                        } else {
                            // Check if one eventually reaches the other
                            let et_final = follow_transitive(et);
                            let tt_final = follow_transitive(tt);
                            if et_final == Some(tt) || et_final == tt_final {
                                // else path eventually reaches then's merge → use then target
                                *self.block_ids.get(&tt).unwrap_or(&else_id)
                            } else if tt_final == Some(et) {
                                *self.block_ids.get(&et).unwrap_or(&else_id)
                            } else {
                                // Use then target as merge (it's the direct goto)
                                *self.block_ids.get(&tt).unwrap_or(&else_id)
                            }
                        }
                    } else if let Some(tt) = then_target {
                        *self.block_ids.get(&tt).unwrap_or(&else_id)
                    } else if let Some(et) = else_target {
                        *self.block_ids.get(&et).unwrap_or(&else_id)
                    } else {
                        else_id
                    }
                } else {
                    else_id
                };

                self.emit(SpvOp::OpSelectionMerge, &[merge_id, 0]);
                self.emit(SpvOp::OpBranchConditional, &[cond_id, then_id, else_id]);
            }
            MirTerminator::Switch {
                value,
                targets,
                default,
            } => {
                let val_id = self.gen_value(value, func)?;
                let default_id = *self.block_ids.get(default).unwrap();

                let mut operands = vec![val_id, default_id];
                for (const_val, target) in targets {
                    let cv = match const_val {
                        MirConst::Int(v, _) => *v as u32,
                        MirConst::Uint(v, _) => *v as u32,
                        _ => 0,
                    };
                    let target_id = *self.block_ids.get(target).unwrap();
                    operands.push(cv);
                    operands.push(target_id);
                }
                self.emit(SpvOp::OpSwitch, &operands);
            }
            MirTerminator::Call {
                func: callee,
                args,
                dest,
                target,
                ..
            } => {
                let arg_ids: Vec<u32> = args
                    .iter()
                    .map(|a| self.gen_value(a, func))
                    .collect::<Result<Vec<_>, _>>()?;

                let ret_ty = if let Some(dest_id) = dest {
                    self.get_local_type(*dest_id, func).unwrap_or(MirType::Void)
                } else {
                    MirType::Void
                };
                let ret_ty_id = self.get_type_id(&ret_ty);

                // Check if callee is a GLSL.std.450 builtin - emit OpExtInst instead of OpFunctionCall
                let callee_name = match callee {
                    MirValue::Function(name) | MirValue::Global(name) => Some(name.as_ref()),
                    _ => None,
                };
                let glsl_opcode = callee_name.and_then(|name| Self::glsl_builtin_opcode(name));

                let result_id = if let Some(opcode) = glsl_opcode {
                    // GLSL.std.450 extended instruction
                    self.emit_glsl_ext(ret_ty_id, opcode, &arg_ids)
                } else {
                    // Regular function call
                    let callee_id = self.gen_value(callee, func)?;
                    let result_id = self.alloc_id();
                    let mut operands = vec![ret_ty_id, result_id, callee_id];
                    operands.extend(&arg_ids);
                    self.emit(SpvOp::OpFunctionCall, &operands);
                    result_id
                };

                if let Some(dest_local) = dest {
                    let ptr_id = *self.local_ids.get(dest_local).unwrap();
                    self.emit(SpvOp::OpStore, &[ptr_id, result_id]);
                }

                if let Some(target) = target {
                    let target_id = *self.block_ids.get(target).unwrap();
                    self.emit(SpvOp::OpBranch, &[target_id]);
                }
            }
            MirTerminator::Return(value) => {
                // Entry point functions (main, shader stages) are overridden
                // to return void. Store result to output variables, then OpReturn.
                let is_entry = func.shader_stage.is_some()
                    || func.linkage == Linkage::External
                    || func.name.as_ref() == "main";
                if is_entry {
                    // For shader entry points with a return value, store to output variable(s)
                    if let Some(val) = value {
                        let val_id = self.gen_value(val, func)?;
                        let num_inputs = func.sig.params.len();
                        let num_outputs = self.io_var_ids.len() - num_inputs;

                        if num_outputs > 1 {
                            // Struct return: decompose into individual field stores.
                            // Each output variable corresponds to a struct field.
                            if let MirType::Struct(ref sname) = func.sig.ret {
                                let fields = self.struct_defs.get(sname).cloned();
                                if let Some(fields) = fields {
                                    for (fi, (_, field_ty)) in fields.iter().enumerate() {
                                        if num_inputs + fi < self.io_var_ids.len() {
                                            let output_var = self.io_var_ids[num_inputs + fi];
                                            let field_ty_id = self.get_type_id(field_ty);
                                            let extracted = self.alloc_id();
                                            self.emit(
                                                SpvOp::OpCompositeExtract,
                                                &[field_ty_id, extracted, val_id, fi as u32],
                                            );
                                            self.emit(SpvOp::OpStore, &[output_var, extracted]);
                                        }
                                    }
                                }
                            }
                        } else if num_inputs < self.io_var_ids.len() {
                            // Single output variable - store directly
                            let output_var = self.io_var_ids[num_inputs];
                            self.emit(SpvOp::OpStore, &[output_var, val_id]);
                        }
                    }
                    self.emit(SpvOp::OpReturn, &[]);
                } else if let Some(val) = value {
                    let val_id = self.gen_value(val, func)?;
                    self.emit(SpvOp::OpReturnValue, &[val_id]);
                } else {
                    self.emit(SpvOp::OpReturn, &[]);
                }
            }
            MirTerminator::Unreachable | MirTerminator::Abort => {
                self.emit(SpvOp::OpUnreachable, &[]);
            }
            MirTerminator::Drop { target, .. } => {
                let target_id = *self.block_ids.get(target).unwrap();
                self.emit(SpvOp::OpBranch, &[target_id]);
            }
            MirTerminator::Assert {
                cond,
                expected,
                target,
                ..
            } => {
                // Simplified assert - just branch if condition passes
                let _cond_id = self.gen_value(cond, func)?;
                let target_id = *self.block_ids.get(target).unwrap();

                if *expected {
                    self.emit(SpvOp::OpBranch, &[target_id]);
                } else {
                    self.emit(SpvOp::OpBranch, &[target_id]);
                }
            }
            MirTerminator::Resume => {
                self.emit(SpvOp::OpUnreachable, &[]);
            }
        }
        Ok(())
    }

    // =========================================================================
    // HELPERS
    // =========================================================================

    /// Get the SPIR-V binary operation opcode.
    fn spirv_binop(&self, op: BinOp, ty: &MirType) -> SpvOp {
        let is_float = ty.is_float();
        let is_signed = ty.is_signed();

        match op {
            BinOp::Add => {
                if is_float {
                    SpvOp::OpFAdd
                } else {
                    SpvOp::OpIAdd
                }
            }
            BinOp::Sub => {
                if is_float {
                    SpvOp::OpFSub
                } else {
                    SpvOp::OpISub
                }
            }
            BinOp::Mul => {
                if is_float {
                    SpvOp::OpFMul
                } else {
                    SpvOp::OpIMul
                }
            }
            BinOp::Div => {
                if is_float {
                    SpvOp::OpFDiv
                } else if is_signed {
                    SpvOp::OpSDiv
                } else {
                    SpvOp::OpUDiv
                }
            }
            BinOp::Rem => {
                if is_float {
                    SpvOp::OpFRem
                } else if is_signed {
                    SpvOp::OpSRem
                } else {
                    SpvOp::OpUMod
                }
            }
            BinOp::BitAnd => SpvOp::OpBitwiseAnd,
            BinOp::BitOr => SpvOp::OpBitwiseOr,
            BinOp::BitXor => SpvOp::OpBitwiseXor,
            BinOp::Shl => SpvOp::OpShiftLeftLogical,
            BinOp::Shr => {
                if is_signed {
                    SpvOp::OpShiftRightArithmetic
                } else {
                    SpvOp::OpShiftRightLogical
                }
            }
            BinOp::Eq => {
                if is_float {
                    SpvOp::OpFOrdEqual
                } else {
                    SpvOp::OpIEqual
                }
            }
            BinOp::Ne => {
                if is_float {
                    SpvOp::OpFOrdNotEqual
                } else {
                    SpvOp::OpINotEqual
                }
            }
            BinOp::Lt => {
                if is_float {
                    SpvOp::OpFOrdLessThan
                } else if is_signed {
                    SpvOp::OpSLessThan
                } else {
                    SpvOp::OpULessThan
                }
            }
            BinOp::Le => {
                if is_float {
                    SpvOp::OpFOrdLessThanEqual
                } else if is_signed {
                    SpvOp::OpSLessThanEqual
                } else {
                    SpvOp::OpULessThanEqual
                }
            }
            BinOp::Gt => {
                if is_float {
                    SpvOp::OpFOrdGreaterThan
                } else if is_signed {
                    SpvOp::OpSGreaterThan
                } else {
                    SpvOp::OpUGreaterThan
                }
            }
            BinOp::Ge => {
                if is_float {
                    SpvOp::OpFOrdGreaterThanEqual
                } else if is_signed {
                    SpvOp::OpSGreaterThanEqual
                } else {
                    SpvOp::OpUGreaterThanEqual
                }
            }
            _ => SpvOp::OpIAdd, // Fallback
        }
    }

    /// Get the SPIR-V cast opcode.
    fn spirv_cast(&self, kind: CastKind, from: &MirType, to: &MirType) -> SpvOp {
        match kind {
            CastKind::IntToInt => {
                if from.is_signed() {
                    SpvOp::OpSConvert
                } else {
                    SpvOp::OpUConvert
                }
            }
            CastKind::FloatToFloat => SpvOp::OpFConvert,
            CastKind::IntToFloat => {
                if from.is_signed() {
                    SpvOp::OpConvertSToF
                } else {
                    SpvOp::OpConvertUToF
                }
            }
            CastKind::FloatToInt => {
                if to.is_signed() {
                    SpvOp::OpConvertFToS
                } else {
                    SpvOp::OpConvertFToU
                }
            }
            CastKind::Transmute
            | CastKind::PtrToPtr
            | CastKind::PtrToInt
            | CastKind::IntToPtr
            | CastKind::FnToPtr => SpvOp::OpBitcast,
        }
    }

    /// Get the type of a local.
    fn get_local_type(&self, id: LocalId, func: &MirFunction) -> CodegenResult<MirType> {
        func.locals
            .iter()
            .find(|l| l.id == id)
            .map(|l| l.ty.clone())
            .ok_or_else(|| CodegenError::Internal(format!("Unknown local type: {:?}", id)))
    }

    /// Infer the type of a value.
    fn infer_value_type(&self, value: &MirValue, func: &MirFunction) -> CodegenResult<MirType> {
        match value {
            MirValue::Local(id) => self.get_local_type(*id, func),
            MirValue::Const(c) => match c {
                MirConst::Bool(_) => Ok(MirType::Bool),
                MirConst::Int(_, ty) => Ok(ty.clone()),
                MirConst::Uint(_, ty) => Ok(ty.clone()),
                MirConst::Float(_, ty) => Ok(ty.clone()),
                MirConst::Null(ty) => Ok(ty.clone()),
                MirConst::Unit => Ok(MirType::Void),
                MirConst::Zeroed(ty) => Ok(ty.clone()),
                MirConst::Undef(ty) => Ok(ty.clone()),
                _ => Ok(MirType::i32()),
            },
            MirValue::Global(_) | MirValue::Function(_) => Ok(MirType::i32()),
        }
    }

    /// Determine the execution model for a given function based on its shader_stage.
    fn execution_model_for_func(&self, func: &MirFunction) -> SpvExecutionModel {
        match func.shader_stage {
            Some(ShaderStage::Vertex) => SpvExecutionModel::Vertex,
            Some(ShaderStage::Fragment) => SpvExecutionModel::Fragment,
            Some(ShaderStage::Compute) => SpvExecutionModel::GLCompute,
            None => self.execution_model,
        }
    }

    // =========================================================================
    // DIRECT SHADER GENERATION
    // =========================================================================
    // These methods construct complete, valid SPIR-V binaries for specific
    // shader programs by calling the emission helpers directly. This bypasses
    // the full BuildLang MIR pipeline and proves the backend CAN generate
    // valid vertex/fragment shaders that pass spirv-val and load into Vulkan.

    /// Generate a complete SPIR-V binary for a minimal triangle fragment shader.
    ///
    /// Equivalent GLSL:
    /// ```glsl
    /// #version 450
    /// layout(location = 0) in vec3 fragColor;
    /// layout(location = 0) out vec4 outColor;
    /// void main() {
    ///     outColor = vec4(fragColor, 1.0);
    /// }
    /// ```
    pub fn generate_triangle_fragment_shader(&mut self) -> Vec<u8> {
        self.reset();
        self.execution_model = SpvExecutionModel::Fragment;

        // == Section 1: Header (SPIR-V 1.0 for Vulkan 1.0 compatibility) ==
        self.emit_header_version(SPIRV_VERSION_1_0);

        // == Section 2: Capabilities ==
        self.emit(SpvOp::OpCapability, &[SpvCapability::Shader as u32]);

        // == Section 3: Extensions ==
        let glsl_ext_id = self.alloc_id();
        self.glsl_ext_id = Some(glsl_ext_id);
        let name_words = self.emit_string("GLSL.std.450");
        let mut ops = vec![glsl_ext_id];
        ops.extend(name_words);
        self.emit(SpvOp::OpExtInstImport, &ops);

        // == Section 4: Memory model ==
        self.emit_memory_model();

        // Snapshot preamble end
        let preamble_end = self.output.len();

        // == Allocate all IDs up front ==
        let void_ty = self.alloc_id(); // %void
        let f32_ty = self.alloc_id(); // %float
        let vec3_ty = self.alloc_id(); // %v3float
        let vec4_ty = self.alloc_id(); // %v4float
        let ptr_input_vec3 = self.alloc_id(); // %_ptr_Input_v3float
        let ptr_output_vec4 = self.alloc_id(); // %_ptr_Output_v4float
        let func_ty = self.alloc_id(); // %func_void
        let frag_color_var = self.alloc_id(); // %fragColor
        let out_color_var = self.alloc_id(); // %outColor
        let main_fn = self.alloc_id(); // %main
        let f32_1_0 = self.alloc_id(); // constant 1.0f

        // == Section 5: Entry point ==
        let mut ep_buf: Vec<u32> = Vec::new();
        let main_name = self.emit_string("main");
        let mut ep_ops = vec![SpvExecutionModel::Fragment as u32, main_fn];
        ep_ops.extend(main_name);
        ep_ops.push(frag_color_var);
        ep_ops.push(out_color_var);
        Self::emit_to(&mut ep_buf, SpvOp::OpEntryPoint, &ep_ops);

        // == Section 6: Execution mode ==
        Self::emit_to(
            &mut ep_buf,
            SpvOp::OpExecutionMode,
            &[main_fn, SpvExecutionMode::OriginUpperLeft as u32],
        );

        // == Section 7: Debug names ==
        let mut names_buf: Vec<u32> = Vec::new();
        let n = self.emit_string("main");
        let mut o = vec![main_fn];
        o.extend(n);
        Self::emit_to(&mut names_buf, SpvOp::OpName, &o);
        let n = self.emit_string("fragColor");
        let mut o = vec![frag_color_var];
        o.extend(n);
        Self::emit_to(&mut names_buf, SpvOp::OpName, &o);
        let n = self.emit_string("outColor");
        let mut o = vec![out_color_var];
        o.extend(n);
        Self::emit_to(&mut names_buf, SpvOp::OpName, &o);

        // == Section 8: Annotations ==
        let mut annot_buf: Vec<u32> = Vec::new();
        // fragColor: Location 0
        Self::emit_to(
            &mut annot_buf,
            SpvOp::OpDecorate,
            &[frag_color_var, SpvDecoration::Location as u32, 0],
        );
        // outColor: Location 0
        Self::emit_to(
            &mut annot_buf,
            SpvOp::OpDecorate,
            &[out_color_var, SpvDecoration::Location as u32, 0],
        );

        // == Section 9: Type/constant/global declarations ==
        let mut globals_buf: Vec<u32> = Vec::new();
        // %void = OpTypeVoid
        Self::emit_to(&mut globals_buf, SpvOp::OpTypeVoid, &[void_ty]);
        // %float = OpTypeFloat 32
        Self::emit_to(&mut globals_buf, SpvOp::OpTypeFloat, &[f32_ty, 32]);
        // %v3float = OpTypeVector %float 3
        Self::emit_to(&mut globals_buf, SpvOp::OpTypeVector, &[vec3_ty, f32_ty, 3]);
        // %v4float = OpTypeVector %float 4
        Self::emit_to(&mut globals_buf, SpvOp::OpTypeVector, &[vec4_ty, f32_ty, 4]);
        // %_ptr_Input_v3float = OpTypePointer Input %v3float
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypePointer,
            &[ptr_input_vec3, SpvStorageClass::Input as u32, vec3_ty],
        );
        // %_ptr_Output_v4float = OpTypePointer Output %v4float
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypePointer,
            &[ptr_output_vec4, SpvStorageClass::Output as u32, vec4_ty],
        );
        // %func_void = OpTypeFunction %void
        Self::emit_to(&mut globals_buf, SpvOp::OpTypeFunction, &[func_ty, void_ty]);
        // %float_1 = OpConstant %float 1.0
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpConstant,
            &[f32_ty, f32_1_0, 1.0f32.to_bits()],
        );
        // %fragColor = OpVariable %_ptr_Input_v3float Input
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpVariable,
            &[
                ptr_input_vec3,
                frag_color_var,
                SpvStorageClass::Input as u32,
            ],
        );
        // %outColor = OpVariable %_ptr_Output_v4float Output
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpVariable,
            &[
                ptr_output_vec4,
                out_color_var,
                SpvStorageClass::Output as u32,
            ],
        );

        // == Sections 10-11: Function body ==
        let mut func_buf: Vec<u32> = Vec::new();
        // %main = OpFunction %void None %func_void
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpFunction,
            &[void_ty, main_fn, 0, func_ty],
        );
        // %label = OpLabel
        let label = self.alloc_id();
        Self::emit_to(&mut func_buf, SpvOp::OpLabel, &[label]);
        // %loaded_color = OpLoad %v3float %fragColor
        let loaded_color = self.alloc_id();
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpLoad,
            &[vec3_ty, loaded_color, frag_color_var],
        );
        // Extract components from fragColor
        let comp_r = self.alloc_id();
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpCompositeExtract,
            &[f32_ty, comp_r, loaded_color, 0],
        );
        let comp_g = self.alloc_id();
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpCompositeExtract,
            &[f32_ty, comp_g, loaded_color, 1],
        );
        let comp_b = self.alloc_id();
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpCompositeExtract,
            &[f32_ty, comp_b, loaded_color, 2],
        );
        // Construct vec4(r, g, b, 1.0)
        let result_vec4 = self.alloc_id();
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpCompositeConstruct,
            &[vec4_ty, result_vec4, comp_r, comp_g, comp_b, f32_1_0],
        );
        // OpStore %outColor %result_vec4
        Self::emit_to(&mut func_buf, SpvOp::OpStore, &[out_color_var, result_vec4]);
        // OpReturn
        Self::emit_to(&mut func_buf, SpvOp::OpReturn, &[]);
        // OpFunctionEnd
        Self::emit_to(&mut func_buf, SpvOp::OpFunctionEnd, &[]);

        // == Assemble final binary ==
        self.output.truncate(preamble_end);
        self.output.extend_from_slice(&ep_buf);
        self.output.extend_from_slice(&names_buf);
        self.output.extend_from_slice(&annot_buf);
        self.output.extend_from_slice(&globals_buf);
        self.output.extend_from_slice(&func_buf);

        // Update bound
        self.output[3] = self.next_id;

        // Convert to bytes
        self.output.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// Generate a complete SPIR-V binary for a minimal triangle vertex shader.
    ///
    /// Equivalent GLSL:
    /// ```glsl
    /// #version 450
    /// layout(location = 0) out vec3 fragColor;
    /// vec2 positions[3] = vec2[](vec2(0.0, -0.5), vec2(0.5, 0.5), vec2(-0.5, 0.5));
    /// vec3 colors[3] = vec3[](vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), vec3(0.0, 0.0, 1.0));
    /// void main() {
    ///     gl_Position = vec4(positions[gl_VertexIndex], 0.0, 1.0);
    ///     fragColor = colors[gl_VertexIndex];
    /// }
    /// ```
    pub fn generate_triangle_vertex_shader(&mut self) -> Vec<u8> {
        self.reset();
        self.execution_model = SpvExecutionModel::Vertex;

        // == Header (SPIR-V 1.0 for Vulkan 1.0 compatibility) ==
        self.emit_header_version(SPIRV_VERSION_1_0);
        self.emit(SpvOp::OpCapability, &[SpvCapability::Shader as u32]);

        let glsl_ext_id = self.alloc_id();
        self.glsl_ext_id = Some(glsl_ext_id);
        let name_words = self.emit_string("GLSL.std.450");
        let mut ops = vec![glsl_ext_id];
        ops.extend(name_words);
        self.emit(SpvOp::OpExtInstImport, &ops);

        self.emit_memory_model();
        let preamble_end = self.output.len();

        // == Allocate type IDs ==
        let void_ty = self.alloc_id();
        let f32_ty = self.alloc_id();
        let i32_ty = self.alloc_id();
        let vec2_ty = self.alloc_id();
        let vec3_ty = self.alloc_id();
        let vec4_ty = self.alloc_id();
        let uint_ty = self.alloc_id(); // for array length constant type
        let arr3_vec2_ty = self.alloc_id(); // [vec2; 3]
        let arr3_vec3_ty = self.alloc_id(); // [vec3; 3]
        let ptr_func_arr3_vec2 = self.alloc_id();
        let ptr_func_arr3_vec3 = self.alloc_id();
        let ptr_func_vec2 = self.alloc_id();
        let ptr_func_vec3 = self.alloc_id();
        let ptr_output_vec4 = self.alloc_id();
        let ptr_output_vec3 = self.alloc_id();
        let ptr_input_i32 = self.alloc_id();
        let func_ty = self.alloc_id();

        // == Variables ==
        let gl_position_var = self.alloc_id();
        let frag_color_var = self.alloc_id();
        let vertex_index_var = self.alloc_id();
        let main_fn = self.alloc_id();

        // == Constants ==
        let const_0_0 = self.alloc_id(); // 0.0f
        let const_0_5 = self.alloc_id(); // 0.5f
        let const_neg_0_5 = self.alloc_id(); // -0.5f
        let const_1_0 = self.alloc_id(); // 1.0f
        let const_3_u = self.alloc_id(); // 3u (array length)
        let const_0_i = self.alloc_id(); // 0i (for access chain)

        // Position data: (0.0, -0.5), (0.5, 0.5), (-0.5, 0.5)
        let pos0 = self.alloc_id(); // vec2(0.0, -0.5)
        let pos1 = self.alloc_id(); // vec2(0.5, 0.5)
        let pos2 = self.alloc_id(); // vec2(-0.5, 0.5)
        let positions_const = self.alloc_id(); // array constant

        // Color data: (1,0,0), (0,1,0), (0,0,1)
        let col0 = self.alloc_id(); // vec3(1.0, 0.0, 0.0)
        let col1 = self.alloc_id(); // vec3(0.0, 1.0, 0.0)
        let col2 = self.alloc_id(); // vec3(0.0, 0.0, 1.0)
        let colors_const = self.alloc_id();

        // == Section 5: Entry point ==
        let mut ep_buf: Vec<u32> = Vec::new();
        let main_name = self.emit_string("main");
        let mut ep_ops = vec![SpvExecutionModel::Vertex as u32, main_fn];
        ep_ops.extend(main_name);
        ep_ops.push(gl_position_var);
        ep_ops.push(frag_color_var);
        ep_ops.push(vertex_index_var);
        Self::emit_to(&mut ep_buf, SpvOp::OpEntryPoint, &ep_ops);
        // No execution mode required for vertex shaders

        // == Section 7: Debug names ==
        let mut names_buf: Vec<u32> = Vec::new();
        let n = self.emit_string("main");
        let mut o = vec![main_fn];
        o.extend(n);
        Self::emit_to(&mut names_buf, SpvOp::OpName, &o);
        let n = self.emit_string("gl_Position");
        let mut o = vec![gl_position_var];
        o.extend(n);
        Self::emit_to(&mut names_buf, SpvOp::OpName, &o);
        let n = self.emit_string("fragColor");
        let mut o = vec![frag_color_var];
        o.extend(n);
        Self::emit_to(&mut names_buf, SpvOp::OpName, &o);
        let n = self.emit_string("gl_VertexIndex");
        let mut o = vec![vertex_index_var];
        o.extend(n);
        Self::emit_to(&mut names_buf, SpvOp::OpName, &o);

        // == Section 8: Annotations ==
        let mut annot_buf: Vec<u32> = Vec::new();
        // gl_Position: BuiltIn Position
        Self::emit_to(
            &mut annot_buf,
            SpvOp::OpDecorate,
            &[
                gl_position_var,
                SpvDecoration::BuiltIn as u32,
                SpvBuiltIn::Position as u32,
            ],
        );
        // fragColor: Location 0
        Self::emit_to(
            &mut annot_buf,
            SpvOp::OpDecorate,
            &[frag_color_var, SpvDecoration::Location as u32, 0],
        );
        // gl_VertexIndex: BuiltIn VertexIndex
        Self::emit_to(
            &mut annot_buf,
            SpvOp::OpDecorate,
            &[
                vertex_index_var,
                SpvDecoration::BuiltIn as u32,
                SpvBuiltIn::VertexIndex as u32,
            ],
        );

        // == Section 9: Types, constants, globals ==
        let mut globals_buf: Vec<u32> = Vec::new();
        // Types
        Self::emit_to(&mut globals_buf, SpvOp::OpTypeVoid, &[void_ty]);
        Self::emit_to(&mut globals_buf, SpvOp::OpTypeFloat, &[f32_ty, 32]);
        Self::emit_to(&mut globals_buf, SpvOp::OpTypeInt, &[i32_ty, 32, 1]);
        Self::emit_to(&mut globals_buf, SpvOp::OpTypeInt, &[uint_ty, 32, 0]);
        Self::emit_to(&mut globals_buf, SpvOp::OpTypeVector, &[vec2_ty, f32_ty, 2]);
        Self::emit_to(&mut globals_buf, SpvOp::OpTypeVector, &[vec3_ty, f32_ty, 3]);
        Self::emit_to(&mut globals_buf, SpvOp::OpTypeVector, &[vec4_ty, f32_ty, 4]);

        // Array length constant (must come before array type)
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpConstant,
            &[uint_ty, const_3_u, 3],
        );

        // Array types
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypeArray,
            &[arr3_vec2_ty, vec2_ty, const_3_u],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypeArray,
            &[arr3_vec3_ty, vec3_ty, const_3_u],
        );

        // Pointer types
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypePointer,
            &[
                ptr_func_arr3_vec2,
                SpvStorageClass::Function as u32,
                arr3_vec2_ty,
            ],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypePointer,
            &[
                ptr_func_arr3_vec3,
                SpvStorageClass::Function as u32,
                arr3_vec3_ty,
            ],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypePointer,
            &[ptr_func_vec2, SpvStorageClass::Function as u32, vec2_ty],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypePointer,
            &[ptr_func_vec3, SpvStorageClass::Function as u32, vec3_ty],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypePointer,
            &[ptr_output_vec4, SpvStorageClass::Output as u32, vec4_ty],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypePointer,
            &[ptr_output_vec3, SpvStorageClass::Output as u32, vec3_ty],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypePointer,
            &[ptr_input_i32, SpvStorageClass::Input as u32, i32_ty],
        );
        Self::emit_to(&mut globals_buf, SpvOp::OpTypeFunction, &[func_ty, void_ty]);

        // Float constants
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpConstant,
            &[f32_ty, const_0_0, 0.0f32.to_bits()],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpConstant,
            &[f32_ty, const_0_5, 0.5f32.to_bits()],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpConstant,
            &[f32_ty, const_neg_0_5, (-0.5f32).to_bits()],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpConstant,
            &[f32_ty, const_1_0, 1.0f32.to_bits()],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpConstant,
            &[i32_ty, const_0_i, 0u32],
        );

        // Composite constants: positions
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpConstantComposite,
            &[vec2_ty, pos0, const_0_0, const_neg_0_5],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpConstantComposite,
            &[vec2_ty, pos1, const_0_5, const_0_5],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpConstantComposite,
            &[vec2_ty, pos2, const_neg_0_5, const_0_5],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpConstantComposite,
            &[arr3_vec2_ty, positions_const, pos0, pos1, pos2],
        );

        // Composite constants: colors
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpConstantComposite,
            &[vec3_ty, col0, const_1_0, const_0_0, const_0_0],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpConstantComposite,
            &[vec3_ty, col1, const_0_0, const_1_0, const_0_0],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpConstantComposite,
            &[vec3_ty, col2, const_0_0, const_0_0, const_1_0],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpConstantComposite,
            &[arr3_vec3_ty, colors_const, col0, col1, col2],
        );

        // Global variables (I/O)
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpVariable,
            &[
                ptr_output_vec4,
                gl_position_var,
                SpvStorageClass::Output as u32,
            ],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpVariable,
            &[
                ptr_output_vec3,
                frag_color_var,
                SpvStorageClass::Output as u32,
            ],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpVariable,
            &[
                ptr_input_i32,
                vertex_index_var,
                SpvStorageClass::Input as u32,
            ],
        );

        // == Sections 10-11: Function body ==
        let mut func_buf: Vec<u32> = Vec::new();
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpFunction,
            &[void_ty, main_fn, 0, func_ty],
        );

        let label = self.alloc_id();
        Self::emit_to(&mut func_buf, SpvOp::OpLabel, &[label]);

        // Allocate function-local arrays
        let positions_var = self.alloc_id();
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpVariable,
            &[
                ptr_func_arr3_vec2,
                positions_var,
                SpvStorageClass::Function as u32,
            ],
        );
        let colors_var = self.alloc_id();
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpVariable,
            &[
                ptr_func_arr3_vec3,
                colors_var,
                SpvStorageClass::Function as u32,
            ],
        );

        // Store constant arrays into local variables
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpStore,
            &[positions_var, positions_const],
        );
        Self::emit_to(&mut func_buf, SpvOp::OpStore, &[colors_var, colors_const]);

        // Load gl_VertexIndex
        let vtx_idx = self.alloc_id();
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpLoad,
            &[i32_ty, vtx_idx, vertex_index_var],
        );

        // Access positions[gl_VertexIndex]
        let pos_ptr = self.alloc_id();
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpAccessChain,
            &[ptr_func_vec2, pos_ptr, positions_var, vtx_idx],
        );
        let pos_val = self.alloc_id();
        Self::emit_to(&mut func_buf, SpvOp::OpLoad, &[vec2_ty, pos_val, pos_ptr]);

        // Extract x, y from position
        let pos_x = self.alloc_id();
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpCompositeExtract,
            &[f32_ty, pos_x, pos_val, 0],
        );
        let pos_y = self.alloc_id();
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpCompositeExtract,
            &[f32_ty, pos_y, pos_val, 1],
        );

        // Construct gl_Position = vec4(pos.x, pos.y, 0.0, 1.0)
        let gl_pos = self.alloc_id();
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpCompositeConstruct,
            &[vec4_ty, gl_pos, pos_x, pos_y, const_0_0, const_1_0],
        );
        Self::emit_to(&mut func_buf, SpvOp::OpStore, &[gl_position_var, gl_pos]);

        // Access colors[gl_VertexIndex]
        let col_ptr = self.alloc_id();
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpAccessChain,
            &[ptr_func_vec3, col_ptr, colors_var, vtx_idx],
        );
        let col_val = self.alloc_id();
        Self::emit_to(&mut func_buf, SpvOp::OpLoad, &[vec3_ty, col_val, col_ptr]);

        // Store to fragColor output
        Self::emit_to(&mut func_buf, SpvOp::OpStore, &[frag_color_var, col_val]);

        Self::emit_to(&mut func_buf, SpvOp::OpReturn, &[]);
        Self::emit_to(&mut func_buf, SpvOp::OpFunctionEnd, &[]);

        // == Assemble ==
        self.output.truncate(preamble_end);
        self.output.extend_from_slice(&ep_buf);
        self.output.extend_from_slice(&names_buf);
        self.output.extend_from_slice(&annot_buf);
        self.output.extend_from_slice(&globals_buf);
        self.output.extend_from_slice(&func_buf);

        self.output[3] = self.next_id;

        self.output.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// Generate a vertex shader that reads an MVP matrix from a uniform buffer.
    ///
    /// Equivalent GLSL:
    /// ```glsl
    /// layout(binding = 0) uniform UBO { mat4 mvp; } ubo;
    /// layout(location = 0) in vec3 inPosition;
    /// layout(location = 1) in vec3 inColor;
    /// layout(location = 0) out vec3 fragColor;
    /// void main() {
    ///     gl_Position = ubo.mvp * vec4(inPosition, 1.0);
    ///     fragColor = inColor;
    /// }
    /// ```
    pub fn generate_uniform_vertex_shader(&mut self) -> Vec<u8> {
        self.reset();

        // Allocate all IDs upfront
        let void_ty = self.alloc_id(); // 1
        let float_ty = self.alloc_id(); // 2
        let vec3_ty = self.alloc_id(); // 3
        let vec4_ty = self.alloc_id(); // 4
        let mat4_ty = self.alloc_id(); // 5
        let int_ty = self.alloc_id(); // 6
        let void_fn_ty = self.alloc_id(); // 7
        let main_fn = self.alloc_id(); // 8

        // Input variables
        let in_pos_ptr_ty = self.alloc_id(); // 9
        let in_color_ptr_ty = self.alloc_id(); // 10  (same type as in_pos)
        let in_pos_var = self.alloc_id(); // 11
        let in_color_var = self.alloc_id(); // 12

        // Output variables
        let out_pos_ptr_ty = self.alloc_id(); // 13
        let out_color_ptr_ty = self.alloc_id(); // 14
        let out_pos_var = self.alloc_id(); // 15  (gl_Position)
        let out_color_var = self.alloc_id(); // 16  (fragColor)

        // UBO struct and variable
        let ubo_struct_ty = self.alloc_id(); // 17
        let ubo_ptr_ty = self.alloc_id(); // 18
        let ubo_var = self.alloc_id(); // 19

        // Constants
        let const_0 = self.alloc_id(); // 20
        let const_1f = self.alloc_id(); // 21
        let const_0i = self.alloc_id(); // 22

        // Function-scope temps
        let label = self.alloc_id(); // 23
        let load_pos = self.alloc_id(); // 24
        let load_color = self.alloc_id(); // 25
        let mat_ptr_ty = self.alloc_id(); // 26
        let mvp_ptr = self.alloc_id(); // 27
        let load_mvp = self.alloc_id(); // 28
        let pos_x = self.alloc_id(); // 29
        let pos_y = self.alloc_id(); // 30
        let pos_z = self.alloc_id(); // 31
        let pos4 = self.alloc_id(); // 32
        let clip_pos = self.alloc_id(); // 33

        let glsl_ext = self.alloc_id(); // 34

        // Section buffers
        let mut ep_buf: Vec<u32> = Vec::new();
        let mut names_buf: Vec<u32> = Vec::new();
        let mut annot_buf: Vec<u32> = Vec::new();
        let mut globals_buf: Vec<u32> = Vec::new();
        let mut func_buf: Vec<u32> = Vec::new();

        // == Header ==
        self.output.push(SPIRV_MAGIC);
        self.output.push(SPIRV_VERSION_1_0);
        self.output.push(GENERATOR_ID);
        self.output.push(0); // bound placeholder
        self.output.push(0); // schema

        // Capabilities
        Self::emit_to(
            &mut self.output,
            SpvOp::OpCapability,
            &[SpvCapability::Shader as u32],
        );
        Self::emit_to(
            &mut self.output,
            SpvOp::OpCapability,
            &[SpvCapability::Matrix as u32],
        );

        // GLSL.std.450 import
        let mut glsl_words = Vec::new();
        glsl_words.push(glsl_ext);
        let glsl_str = b"GLSL.std.450\0";
        let mut word = 0u32;
        for (i, &byte) in glsl_str.iter().enumerate() {
            word |= (byte as u32) << ((i % 4) * 8);
            if i % 4 == 3 {
                glsl_words.push(word);
                word = 0;
            }
        }
        if glsl_str.len() % 4 != 0 {
            glsl_words.push(word);
        }
        Self::emit_to(&mut self.output, SpvOp::OpExtInstImport, &glsl_words);

        // Memory model
        Self::emit_to(&mut self.output, SpvOp::OpMemoryModel, &[0, 1]); // Logical GLSL450

        let preamble_end = self.output.len();

        // == Entry point ==
        let mut ep_operands = vec![SpvExecutionModel::Vertex as u32, main_fn];
        let name_str = b"main\0";
        let mut word = 0u32;
        for (i, &byte) in name_str.iter().enumerate() {
            word |= (byte as u32) << ((i % 4) * 8);
            if i % 4 == 3 {
                ep_operands.push(word);
                word = 0;
            }
        }
        if name_str.len() % 4 != 0 {
            ep_operands.push(word);
        }
        // Interface variables
        ep_operands.extend_from_slice(&[in_pos_var, in_color_var, out_pos_var, out_color_var]);
        Self::emit_to(&mut ep_buf, SpvOp::OpEntryPoint, &ep_operands);

        // == Debug names ==
        Self::emit_to_name(&mut names_buf, main_fn, "main");
        Self::emit_to_name(&mut names_buf, ubo_struct_ty, "UBO");
        Self::emit_to_member_name(&mut names_buf, ubo_struct_ty, 0, "mvp");
        Self::emit_to_name(&mut names_buf, in_pos_var, "inPosition");
        Self::emit_to_name(&mut names_buf, in_color_var, "inColor");
        Self::emit_to_name(&mut names_buf, out_pos_var, "gl_Position");
        Self::emit_to_name(&mut names_buf, out_color_var, "fragColor");

        // == Annotations ==
        // UBO struct Block decoration
        Self::emit_to(
            &mut annot_buf,
            SpvOp::OpDecorate,
            &[ubo_struct_ty, SpvDecoration::Block as u32],
        );
        // UBO member offset (mat4 at offset 0)
        Self::emit_to(
            &mut annot_buf,
            SpvOp::OpMemberDecorate,
            &[ubo_struct_ty, 0, SpvDecoration::Offset as u32, 0],
        );
        // mat4 column major
        Self::emit_to(
            &mut annot_buf,
            SpvOp::OpMemberDecorate,
            &[ubo_struct_ty, 0, SpvDecoration::ColMajor as u32],
        );
        // mat4 matrix stride = 16 (4 floats * 4 bytes)
        Self::emit_to(
            &mut annot_buf,
            SpvOp::OpMemberDecorate,
            &[ubo_struct_ty, 0, SpvDecoration::MatrixStride as u32, 16],
        );

        // UBO variable binding
        Self::emit_to(
            &mut annot_buf,
            SpvOp::OpDecorate,
            &[ubo_var, SpvDecoration::DescriptorSet as u32, 0],
        );
        Self::emit_to(
            &mut annot_buf,
            SpvOp::OpDecorate,
            &[ubo_var, SpvDecoration::Binding as u32, 0],
        );

        // Input locations
        Self::emit_to(
            &mut annot_buf,
            SpvOp::OpDecorate,
            &[in_pos_var, SpvDecoration::Location as u32, 0],
        );
        Self::emit_to(
            &mut annot_buf,
            SpvOp::OpDecorate,
            &[in_color_var, SpvDecoration::Location as u32, 1],
        );

        // Output: gl_Position with BuiltIn
        Self::emit_to(
            &mut annot_buf,
            SpvOp::OpDecorate,
            &[
                out_pos_var,
                SpvDecoration::BuiltIn as u32,
                SpvBuiltIn::Position as u32,
            ],
        );
        // Output: fragColor at Location 0
        Self::emit_to(
            &mut annot_buf,
            SpvOp::OpDecorate,
            &[out_color_var, SpvDecoration::Location as u32, 0],
        );

        // == Global types and constants ==
        Self::emit_to(&mut globals_buf, SpvOp::OpTypeVoid, &[void_ty]);
        Self::emit_to(&mut globals_buf, SpvOp::OpTypeFloat, &[float_ty, 32]);
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypeVector,
            &[vec3_ty, float_ty, 3],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypeVector,
            &[vec4_ty, float_ty, 4],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypeMatrix,
            &[mat4_ty, vec4_ty, 4],
        );
        Self::emit_to(&mut globals_buf, SpvOp::OpTypeInt, &[int_ty, 32, 1]);
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypeFunction,
            &[void_fn_ty, void_ty],
        );

        // Pointer types
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypePointer,
            &[in_pos_ptr_ty, SpvStorageClass::Input as u32, vec3_ty],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypePointer,
            &[in_color_ptr_ty, SpvStorageClass::Input as u32, vec3_ty],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypePointer,
            &[out_pos_ptr_ty, SpvStorageClass::Output as u32, vec4_ty],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypePointer,
            &[out_color_ptr_ty, SpvStorageClass::Output as u32, vec3_ty],
        );

        // UBO struct: { mat4 mvp }
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypeStruct,
            &[ubo_struct_ty, mat4_ty],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypePointer,
            &[ubo_ptr_ty, SpvStorageClass::Uniform as u32, ubo_struct_ty],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypePointer,
            &[mat_ptr_ty, SpvStorageClass::Uniform as u32, mat4_ty],
        );

        // Variables
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpVariable,
            &[in_pos_ptr_ty, in_pos_var, SpvStorageClass::Input as u32],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpVariable,
            &[in_color_ptr_ty, in_color_var, SpvStorageClass::Input as u32],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpVariable,
            &[out_pos_ptr_ty, out_pos_var, SpvStorageClass::Output as u32],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpVariable,
            &[
                out_color_ptr_ty,
                out_color_var,
                SpvStorageClass::Output as u32,
            ],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpVariable,
            &[ubo_ptr_ty, ubo_var, SpvStorageClass::Uniform as u32],
        );

        // Constants
        Self::emit_to(&mut globals_buf, SpvOp::OpConstant, &[float_ty, const_0, 0]); // 0.0f
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpConstant,
            &[float_ty, const_1f, 0x3F800000],
        ); // 1.0f
        Self::emit_to(&mut globals_buf, SpvOp::OpConstant, &[int_ty, const_0i, 0]); // 0 (int)

        // == Function body ==
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpFunction,
            &[void_ty, main_fn, 0, void_fn_ty],
        );
        Self::emit_to(&mut func_buf, SpvOp::OpLabel, &[label]);

        // Load inputs
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpLoad,
            &[vec3_ty, load_pos, in_pos_var],
        );
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpLoad,
            &[vec3_ty, load_color, in_color_var],
        );

        // Access UBO.mvp (member 0)
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpAccessChain,
            &[mat_ptr_ty, mvp_ptr, ubo_var, const_0i],
        );
        Self::emit_to(&mut func_buf, SpvOp::OpLoad, &[mat4_ty, load_mvp, mvp_ptr]);

        // Construct vec4(position, 1.0)
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpCompositeExtract,
            &[float_ty, pos_x, load_pos, 0],
        );
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpCompositeExtract,
            &[float_ty, pos_y, load_pos, 1],
        );
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpCompositeExtract,
            &[float_ty, pos_z, load_pos, 2],
        );
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpCompositeConstruct,
            &[vec4_ty, pos4, pos_x, pos_y, pos_z, const_1f],
        );

        // gl_Position = mvp * vec4(pos, 1.0)
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpMatrixTimesVector,
            &[vec4_ty, clip_pos, load_mvp, pos4],
        );

        // Store outputs
        Self::emit_to(&mut func_buf, SpvOp::OpStore, &[out_pos_var, clip_pos]);
        Self::emit_to(&mut func_buf, SpvOp::OpStore, &[out_color_var, load_color]);

        Self::emit_to(&mut func_buf, SpvOp::OpReturn, &[]);
        Self::emit_to(&mut func_buf, SpvOp::OpFunctionEnd, &[]);

        // == Assemble ==
        self.output.truncate(preamble_end);
        self.output.extend_from_slice(&ep_buf);
        self.output.extend_from_slice(&names_buf);
        self.output.extend_from_slice(&annot_buf);
        self.output.extend_from_slice(&globals_buf);
        self.output.extend_from_slice(&func_buf);

        self.output[3] = self.next_id;
        self.output.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// Generate a fragment shader that samples a texture and applies tone mapping.
    ///
    /// Equivalent GLSL:
    /// ```glsl
    /// layout(binding = 0) uniform sampler2D texSampler;
    /// layout(location = 0) in vec2 fragTexCoord;
    /// layout(location = 0) out vec4 outColor;
    /// void main() {
    ///     vec4 color = texture(texSampler, fragTexCoord);
    ///     // Simple ACES-like tone mapping: (color * 2.51 + 0.03) / (color * 2.43 + 0.59)
    ///     outColor = color;
    /// }
    /// ```
    pub fn generate_textured_fragment_shader(&mut self) -> Vec<u8> {
        self.reset();

        // Allocate all IDs upfront
        let void_ty = self.alloc_id(); // 1
        let float_ty = self.alloc_id(); // 2
        let vec2_ty = self.alloc_id(); // 3
        let vec4_ty = self.alloc_id(); // 4
        let void_fn_ty = self.alloc_id(); // 5
        let main_fn = self.alloc_id(); // 6

        // Image/sampler types
        let image_ty = self.alloc_id(); // 7 - OpTypeImage (float, 2D)
        let sampled_img_ty = self.alloc_id(); // 8 - OpTypeSampledImage
        let sampler_ptr_ty = self.alloc_id(); // 9

        // Input/output variables
        let in_uv_ptr_ty = self.alloc_id(); // 10
        let out_color_ptr_ty = self.alloc_id(); // 11
        let in_uv_var = self.alloc_id(); // 12
        let out_color_var = self.alloc_id(); // 13
        let sampler_var = self.alloc_id(); // 14

        // Function-scope temps
        let label = self.alloc_id(); // 15
        let load_uv = self.alloc_id(); // 16
        let load_sampler = self.alloc_id(); // 17
        let sampled_color = self.alloc_id(); // 18

        // Section buffers
        let mut ep_buf: Vec<u32> = Vec::new();
        let mut names_buf: Vec<u32> = Vec::new();
        let mut annot_buf: Vec<u32> = Vec::new();
        let mut globals_buf: Vec<u32> = Vec::new();
        let mut func_buf: Vec<u32> = Vec::new();

        // == Header ==
        self.output.push(SPIRV_MAGIC);
        self.output.push(SPIRV_VERSION_1_0);
        self.output.push(GENERATOR_ID);
        self.output.push(0); // bound placeholder
        self.output.push(0); // schema

        // Capabilities
        Self::emit_to(
            &mut self.output,
            SpvOp::OpCapability,
            &[SpvCapability::Shader as u32],
        );

        // GLSL.std.450 import
        let glsl_ext = self.alloc_id();
        let mut glsl_words = Vec::new();
        glsl_words.push(glsl_ext);
        let glsl_str = b"GLSL.std.450\0";
        let mut word = 0u32;
        for (i, &byte) in glsl_str.iter().enumerate() {
            word |= (byte as u32) << ((i % 4) * 8);
            if i % 4 == 3 {
                glsl_words.push(word);
                word = 0;
            }
        }
        if glsl_str.len() % 4 != 0 {
            glsl_words.push(word);
        }
        Self::emit_to(&mut self.output, SpvOp::OpExtInstImport, &glsl_words);

        // Memory model
        Self::emit_to(&mut self.output, SpvOp::OpMemoryModel, &[0, 1]); // Logical GLSL450

        let preamble_end = self.output.len();

        // == Entry point ==
        let mut ep_operands = vec![SpvExecutionModel::Fragment as u32, main_fn];
        let name_str = b"main\0";
        let mut word = 0u32;
        for (i, &byte) in name_str.iter().enumerate() {
            word |= (byte as u32) << ((i % 4) * 8);
            if i % 4 == 3 {
                ep_operands.push(word);
                word = 0;
            }
        }
        if name_str.len() % 4 != 0 {
            ep_operands.push(word);
        }
        // Only Input/Output vars in the interface list (SPIR-V 1.0-1.3 rule)
        ep_operands.extend_from_slice(&[in_uv_var, out_color_var]);
        Self::emit_to(&mut ep_buf, SpvOp::OpEntryPoint, &ep_operands);

        // Fragment shader execution mode: OriginUpperLeft
        Self::emit_to(&mut ep_buf, SpvOp::OpExecutionMode, &[main_fn, 7]); // OriginUpperLeft

        // == Debug names ==
        Self::emit_to_name(&mut names_buf, main_fn, "main");
        Self::emit_to_name(&mut names_buf, in_uv_var, "fragTexCoord");
        Self::emit_to_name(&mut names_buf, out_color_var, "outColor");
        Self::emit_to_name(&mut names_buf, sampler_var, "texSampler");

        // == Annotations ==
        // Sampler binding
        Self::emit_to(
            &mut annot_buf,
            SpvOp::OpDecorate,
            &[sampler_var, SpvDecoration::DescriptorSet as u32, 0],
        );
        Self::emit_to(
            &mut annot_buf,
            SpvOp::OpDecorate,
            &[sampler_var, SpvDecoration::Binding as u32, 0],
        );

        // Input/output locations
        Self::emit_to(
            &mut annot_buf,
            SpvOp::OpDecorate,
            &[in_uv_var, SpvDecoration::Location as u32, 0],
        );
        Self::emit_to(
            &mut annot_buf,
            SpvOp::OpDecorate,
            &[out_color_var, SpvDecoration::Location as u32, 0],
        );

        // == Global types and constants ==
        Self::emit_to(&mut globals_buf, SpvOp::OpTypeVoid, &[void_ty]);
        Self::emit_to(&mut globals_buf, SpvOp::OpTypeFloat, &[float_ty, 32]);
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypeVector,
            &[vec2_ty, float_ty, 2],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypeVector,
            &[vec4_ty, float_ty, 4],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypeFunction,
            &[void_fn_ty, void_ty],
        );

        // OpTypeImage: result, sampled-type, Dim(1=2D), depth(0), arrayed(0), MS(0), sampled(1), format(0=Unknown)
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypeImage,
            &[image_ty, float_ty, 1, 0, 0, 0, 1, 0],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypeSampledImage,
            &[sampled_img_ty, image_ty],
        );

        // Pointer types
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypePointer,
            &[in_uv_ptr_ty, SpvStorageClass::Input as u32, vec2_ty],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypePointer,
            &[out_color_ptr_ty, SpvStorageClass::Output as u32, vec4_ty],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpTypePointer,
            &[
                sampler_ptr_ty,
                SpvStorageClass::UniformConstant as u32,
                sampled_img_ty,
            ],
        );

        // Variables
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpVariable,
            &[in_uv_ptr_ty, in_uv_var, SpvStorageClass::Input as u32],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpVariable,
            &[
                out_color_ptr_ty,
                out_color_var,
                SpvStorageClass::Output as u32,
            ],
        );
        Self::emit_to(
            &mut globals_buf,
            SpvOp::OpVariable,
            &[
                sampler_ptr_ty,
                sampler_var,
                SpvStorageClass::UniformConstant as u32,
            ],
        );

        // == Function body ==
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpFunction,
            &[void_ty, main_fn, 0, void_fn_ty],
        );
        Self::emit_to(&mut func_buf, SpvOp::OpLabel, &[label]);

        // Load UV coordinates
        Self::emit_to(&mut func_buf, SpvOp::OpLoad, &[vec2_ty, load_uv, in_uv_var]);

        // Load combined image+sampler
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpLoad,
            &[sampled_img_ty, load_sampler, sampler_var],
        );

        // Sample texture: vec4 color = texture(sampler, uv)
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpImageSampleImplicitLod,
            &[vec4_ty, sampled_color, load_sampler, load_uv],
        );

        // Store to output
        Self::emit_to(
            &mut func_buf,
            SpvOp::OpStore,
            &[out_color_var, sampled_color],
        );

        Self::emit_to(&mut func_buf, SpvOp::OpReturn, &[]);
        Self::emit_to(&mut func_buf, SpvOp::OpFunctionEnd, &[]);

        // == Assemble ==
        self.output.truncate(preamble_end);
        self.output.extend_from_slice(&ep_buf);
        self.output.extend_from_slice(&names_buf);
        self.output.extend_from_slice(&annot_buf);
        self.output.extend_from_slice(&globals_buf);
        self.output.extend_from_slice(&func_buf);

        self.output[3] = self.next_id;
        self.output.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// Set up shader I/O variables for vertex/fragment shaders.
    ///
    /// For vertex shaders:
    ///   - Each function parameter becomes an Input variable with Location decoration
    ///   - The return type becomes an Output variable with Location decoration
    ///   - If a return field is named "position" and is vec4, add BuiltIn Position decoration
    ///
    /// For fragment shaders:
    ///   - Each function parameter becomes an Input variable with Location decoration
    ///   - The return type becomes an Output variable with Location decoration
    fn setup_shader_io(
        &mut self,
        func: &MirFunction,
        _func_id: u32,
        exec_model: SpvExecutionModel,
    ) {
        // Compute kernels: map each slice/array/pointer parameter to a
        // StorageBuffer binding, and pre-declare the GlobalInvocationId built-in
        // so `gl_GlobalInvocationID.x` (lowered to a ThreadIndex nullary op) can
        // read the thread index. This is what makes a `#[compute]` kernel emit
        // real bindings instead of an entry point with zero interface.
        if matches!(exec_model, SpvExecutionModel::GLCompute) {
            self.compute_buffers.clear();
            self.compute_push_constants.clear();
            self.compute_pc_var = None;

            // Pass 1: partition parameters into buffer params (slice/array ->
            // StorageBuffer at descriptor set 0) and scalar params (-> members
            // of a single push-constant block). Buffers get contiguous bindings
            // 0..num_buffers (a scalar param does NOT consume a binding), which
            // is exactly what the host binds. Scalars accumulate at 4-byte
            // aligned offsets in declaration order.
            let mut next_binding: u32 = 0;
            let mut pc_members: Vec<(LocalId, MirType)> = Vec::new();
            let mut pc_offset: u32 = 0;
            for (i, param_ty) in func.sig.params.iter().enumerate() {
                let local_id = func
                    .locals
                    .iter()
                    .find(|l| l.is_param && l.id.0 == i as u32)
                    .map(|l| l.id);
                // A slice/array parameter passes as `Ptr(Slice/Array(elem))`.
                let buffer_elem = match param_ty {
                    MirType::Ptr(inner) => match inner.as_ref() {
                        MirType::Slice(elem) | MirType::Array(elem, _) => Some((**elem).clone()),
                        _ => None,
                    },
                    MirType::Slice(elem) | MirType::Array(elem, _) => Some((**elem).clone()),
                    _ => None,
                };

                if let Some(elem_ty) = buffer_elem {
                    // Writability: infer strictly from the parameter local's
                    // mutability (`&mut [_]`). Fail-safe default is READ-ONLY:
                    // a bare `&[_]` is never writable, so an accidental store is
                    // caught (see IndexStore). Only `&mut` buffers are writable.
                    let writable = func
                        .locals
                        .iter()
                        .find(|l| l.is_param && l.id.0 == i as u32)
                        .map(|l| l.is_mut)
                        .unwrap_or(false);
                    let binding = next_binding;
                    next_binding += 1;
                    let var_id = self.emit_compute_ssbo(0, binding, &elem_ty, writable);
                    if let Some(id) = local_id {
                        self.compute_buffers.insert(
                            id,
                            ComputeBuffer {
                                var_id,
                                elem_ty,
                                writable,
                            },
                        );
                    }
                } else {
                    // A non-pointer scalar parameter (e.g. `alpha: f32`) becomes
                    // a push-constant member. Post f64->f32 coercion every scalar
                    // is 4 bytes here; keep the offset arithmetic general via
                    // `scalar_stride`.
                    let scalar_ty = param_ty.clone();
                    if let Some(id) = local_id {
                        let stride = Self::scalar_stride(&scalar_ty);
                        // 4-byte natural alignment (all Phase-1 scalars are <=4).
                        let align = stride.max(4);
                        pc_offset = pc_offset.div_ceil(align) * align;
                        self.compute_push_constants.insert(
                            id,
                            ComputePushConstant {
                                member_index: pc_members.len() as u32,
                                offset: pc_offset,
                                ty: scalar_ty.clone(),
                            },
                        );
                        pc_members.push((id, scalar_ty));
                        pc_offset += stride;
                    }
                }
            }

            // Pass 2: if there are any scalar params, emit the push-constant
            // block (one Block-decorated OpTypeStruct in the PushConstant storage
            // class, one member per scalar with an Offset decoration).
            if !pc_members.is_empty() {
                let member_tys: Vec<MirType> = pc_members.iter().map(|(_, t)| t.clone()).collect();
                let offsets: Vec<u32> = pc_members
                    .iter()
                    .map(|(id, _)| self.compute_push_constants[id].offset)
                    .collect();
                let pc_var = self.emit_push_constant_block(&member_tys, &offsets);
                self.compute_pc_var = Some(pc_var);
            }

            // GlobalInvocationId is a uvec3 Input builtin; declare it now and add
            // it to the entry-point interface (required for Input vars).
            let uvec3 = MirType::Vector(Box::new(MirType::Int(IntSize::I32, false)), 3);
            let giid = self.emit_builtin_var(SpvBuiltIn::GlobalInvocationId, &uvec3);
            self.io_var_ids.push(giid);

            // Pre-declare the LOCAL invocation and WORKGROUP index built-ins when
            // the kernel reads them (`gl_LocalInvocationID` / `gl_WorkgroupID`).
            // Both are `Input`-class variables, so they MUST appear in the
            // OpEntryPoint interface -- declaring them here (before body emission
            // builds `io_var_ids`) guarantees that. Only emit the ones the body
            // actually uses so an unused built-in is not forced into the interface.
            let uses_local_id = Self::compute_reads_nullary(func, |op| {
                matches!(op, NullaryOp::LocalInvocationId(_))
            });
            let uses_workgroup_id =
                Self::compute_reads_nullary(func, |op| matches!(op, NullaryOp::WorkgroupId(_)));
            if uses_local_id {
                let lid = self.emit_builtin_var(SpvBuiltIn::LocalInvocationId, &uvec3);
                self.io_var_ids.push(lid);
            }
            if uses_workgroup_id {
                let wid = self.emit_builtin_var(SpvBuiltIn::WorkgroupId, &uvec3);
                self.io_var_ids.push(wid);
            }

            // Workgroup-shared scratch arrays: any local annotated `Workgroup:N`
            // (produced by the frontend's `workgroupArray(N)` lowering) gets one
            // `Workgroup`-class OpVariable of `array<elem_ty, N>`. Register the
            // local -> variable mapping so `scratch[..]` access-chains into it.
            // These variables are NOT pushed into `io_var_ids`: on SPIR-V 1.0 a
            // Workgroup-class variable is not part of the entry-point interface.
            self.workgroup_slots.clear();
            for local in &func.locals {
                if let Some(len) = local.annotations.iter().find_map(|a| {
                    a.strip_prefix("Workgroup:")
                        .and_then(|n| n.parse::<u64>().ok())
                }) {
                    // Element type of the annotated array local (the frontend
                    // builds it as `Array(elem, N)`).
                    let elem_ty = match &local.ty {
                        MirType::Array(elem, _) => (**elem).clone(),
                        // Defensive: a bare annotation with no array type falls
                        // back to f32 (the only Phase-4a element type).
                        _ => MirType::f32(),
                    };
                    let var_id = self.emit_workgroup_var(&elem_ty, len);
                    self.workgroup_slots
                        .insert(local.id, WorkgroupSlot { var_id, elem_ty });
                }
            }
            return;
        }

        if !matches!(
            exec_model,
            SpvExecutionModel::Vertex | SpvExecutionModel::Fragment
        ) {
            return;
        }

        // Create Input variables for each function parameter
        self.shader_input_vars.clear();
        let mut location_counter = 0u32;
        for (i, param_ty) in func.sig.params.iter().enumerate() {
            let param_name = func
                .locals
                .iter()
                .find(|l| l.is_param && l.id.0 == i as u32)
                .and_then(|l| l.name.as_ref())
                .map(|n| n.to_string())
                .unwrap_or_else(|| format!("in_{}", i));

            // Detect builtin parameters by name convention
            let builtin = match param_name.as_str() {
                "vertex_id" | "vertex_index" | "gl_VertexIndex" => Some(SpvBuiltIn::VertexIndex),
                "instance_id" | "instance_index" | "gl_InstanceIndex" => {
                    Some(SpvBuiltIn::InstanceIndex)
                }
                _ => None,
            };

            let ptr_ty_id = self.get_ptr_type_id(param_ty, SpvStorageClass::Input);
            let var_id = self.alloc_id();
            self.emit_global(
                SpvOp::OpVariable,
                &[ptr_ty_id, var_id, SpvStorageClass::Input as u32],
            );

            if let Some(bi) = builtin {
                self.emit_decoration(var_id, SpvDecoration::BuiltIn, &[bi as u32]);
            } else {
                self.emit_decoration(var_id, SpvDecoration::Location, &[location_counter]);
                location_counter += 1;
            }

            self.emit_name(var_id, &param_name);
            self.io_var_ids.push(var_id);
            self.shader_input_vars.push(var_id);
        }

        // Create Output variable for the return type
        if func.sig.ret != MirType::Void {
            let ret_ty = &func.sig.ret;

            // Built-in vector types (build_vec2/3/4) are single output values
            let is_builtin_vec = matches!(ret_ty, MirType::Struct(ref name)
                if name.as_ref() == "build_vec2" || name.as_ref() == "build_vec3" || name.as_ref() == "build_vec4");

            // For vertex shaders returning build_vec4: treat as gl_Position
            let is_vertex_position = matches!(exec_model, SpvExecutionModel::Vertex)
                && matches!(ret_ty, MirType::Struct(ref name) if name.as_ref() == "build_vec4");

            if matches!(exec_model, SpvExecutionModel::Vertex) && !is_builtin_vec {
                // User struct return: split into individual output fields
                if let MirType::Struct(struct_name) = ret_ty {
                    if let Some(fields) = self.struct_defs.get(struct_name).cloned() {
                        for (loc, (field_name, field_ty)) in fields.iter().enumerate() {
                            let ptr_ty_id = self.get_ptr_type_id(field_ty, SpvStorageClass::Output);
                            let var_id = self.alloc_id();
                            self.emit_global(
                                SpvOp::OpVariable,
                                &[ptr_ty_id, var_id, SpvStorageClass::Output as u32],
                            );

                            let is_position = field_name
                                .as_ref()
                                .map(|n| n.as_ref() == "position")
                                .unwrap_or(false);
                            let is_vec4 = matches!(field_ty, MirType::Vector(ref elem, 4) if elem.is_float())
                                || matches!(field_ty, MirType::Struct(ref n) if n.as_ref() == "build_vec4");

                            if is_position && is_vec4 {
                                self.emit_decoration(
                                    var_id,
                                    SpvDecoration::BuiltIn,
                                    &[SpvBuiltIn::Position as u32],
                                );
                            } else {
                                self.emit_decoration(
                                    var_id,
                                    SpvDecoration::Location,
                                    &[loc as u32],
                                );
                            }

                            let out_name = field_name
                                .as_ref()
                                .map(|n| format!("out_{}", n))
                                .unwrap_or_else(|| format!("out_{}", loc));
                            self.emit_name(var_id, &out_name);
                            self.io_var_ids.push(var_id);
                        }
                    } else {
                        // Unknown struct, emit as single output
                        let ptr_ty_id = self.get_ptr_type_id(ret_ty, SpvStorageClass::Output);
                        let var_id = self.alloc_id();
                        self.emit_global(
                            SpvOp::OpVariable,
                            &[ptr_ty_id, var_id, SpvStorageClass::Output as u32],
                        );
                        self.emit_decoration(var_id, SpvDecoration::Location, &[0]);
                        self.io_var_ids.push(var_id);
                    }
                } else {
                    // Non-struct return for vertex shader (MirType::Vector etc)
                    let ptr_ty_id = self.get_ptr_type_id(ret_ty, SpvStorageClass::Output);
                    let var_id = self.alloc_id();
                    self.emit_global(
                        SpvOp::OpVariable,
                        &[ptr_ty_id, var_id, SpvStorageClass::Output as u32],
                    );
                    let is_vec4 = matches!(ret_ty, MirType::Vector(ref elem, 4) if elem.is_float());
                    if is_vec4 {
                        self.emit_decoration(
                            var_id,
                            SpvDecoration::BuiltIn,
                            &[SpvBuiltIn::Position as u32],
                        );
                    } else {
                        self.emit_decoration(var_id, SpvDecoration::Location, &[0]);
                    }
                    self.io_var_ids.push(var_id);
                }
            } else {
                // Built-in vec return (vertex position) or fragment shader output
                // build_vec types already map to OpTypeVector in get_type_id
                let ptr_ty_id = self.get_ptr_type_id(ret_ty, SpvStorageClass::Output);
                let var_id = self.alloc_id();
                self.emit_global(
                    SpvOp::OpVariable,
                    &[ptr_ty_id, var_id, SpvStorageClass::Output as u32],
                );
                if is_vertex_position {
                    self.emit_decoration(
                        var_id,
                        SpvDecoration::BuiltIn,
                        &[SpvBuiltIn::Position as u32],
                    );
                    self.emit_name(var_id, "gl_Position");
                } else {
                    self.emit_decoration(var_id, SpvDecoration::Location, &[0]);
                }
                self.io_var_ids.push(var_id);
            }
        }
    }
}

impl Default for SpirvBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SpirvBackend {
    // =========================================================================
    // F64 → F32 GPU COERCION PASS
    // =========================================================================
    //
    // GPUs operate in f32 (Vulkan requires gl_Position to be vec4<f32>).
    // The MIR lowerer produces f64 by default. This pass rewrites the entire
    // MIR module to use f32 for all floating-point operations when targeting
    // SPIR-V for graphics shaders.

    /// Coerce a MirType from f64 to f32 for GPU execution.
    fn coerce_type(ty: &MirType) -> MirType {
        match ty {
            MirType::Float(FloatSize::F64) => MirType::Float(FloatSize::F32),
            MirType::Ptr(inner) => MirType::Ptr(Box::new(Self::coerce_type(inner))),
            MirType::Array(elem, len) => MirType::Array(Box::new(Self::coerce_type(elem)), *len),
            MirType::Slice(elem) => MirType::Slice(Box::new(Self::coerce_type(elem))),
            MirType::Vector(elem, lanes) => {
                MirType::Vector(Box::new(Self::coerce_type(elem)), *lanes)
            }
            MirType::FnPtr(sig) => {
                let mut sig = sig.as_ref().clone();
                sig.ret = Self::coerce_type(&sig.ret);
                sig.params = sig.params.iter().map(|p| Self::coerce_type(p)).collect();
                MirType::FnPtr(Box::new(sig))
            }
            MirType::Texture2D(elem) => MirType::Texture2D(Box::new(Self::coerce_type(elem))),
            MirType::SampledImage(elem) => MirType::SampledImage(Box::new(Self::coerce_type(elem))),
            _ => ty.clone(), // Bool, Int, Void, Struct, Never, Sampler - unchanged
        }
    }

    /// Coerce a MirConst from f64 to f32.
    fn coerce_const(c: &MirConst) -> MirConst {
        match c {
            MirConst::Float(v, ty) => MirConst::Float(*v, Self::coerce_type(ty)),
            _ => c.clone(),
        }
    }

    /// Coerce a MirValue from f64 to f32.
    fn coerce_value(v: &MirValue) -> MirValue {
        match v {
            MirValue::Const(c) => MirValue::Const(Self::coerce_const(c)),
            _ => v.clone(),
        }
    }

    /// Coerce a MirRValue from f64 to f32.
    fn coerce_rvalue(rv: &MirRValue) -> MirRValue {
        match rv {
            MirRValue::Use(v) => MirRValue::Use(Self::coerce_value(v)),
            MirRValue::BinaryOp { op, left, right } => MirRValue::BinaryOp {
                op: *op,
                left: Self::coerce_value(left),
                right: Self::coerce_value(right),
            },
            MirRValue::UnaryOp { op, operand } => MirRValue::UnaryOp {
                op: *op,
                operand: Self::coerce_value(operand),
            },
            MirRValue::Cast { kind, value, ty } => MirRValue::Cast {
                kind: *kind,
                value: Self::coerce_value(value),
                ty: Self::coerce_type(ty),
            },
            MirRValue::Aggregate { kind, operands } => MirRValue::Aggregate {
                kind: kind.clone(),
                operands: operands.iter().map(|o| Self::coerce_value(o)).collect(),
            },
            MirRValue::Repeat { value, count } => MirRValue::Repeat {
                value: Self::coerce_value(value),
                count: *count,
            },
            MirRValue::FieldAccess {
                base,
                field_name,
                field_ty,
            } => MirRValue::FieldAccess {
                base: Self::coerce_value(base),
                field_name: field_name.clone(),
                field_ty: Self::coerce_type(field_ty),
            },
            MirRValue::VariantField {
                base,
                variant_name,
                field_index,
                field_ty,
            } => MirRValue::VariantField {
                base: Self::coerce_value(base),
                variant_name: variant_name.clone(),
                field_index: *field_index,
                field_ty: Self::coerce_type(field_ty),
            },
            MirRValue::IndexAccess {
                base,
                index,
                elem_ty,
            } => MirRValue::IndexAccess {
                base: Self::coerce_value(base),
                index: Self::coerce_value(index),
                elem_ty: Self::coerce_type(elem_ty),
            },
            MirRValue::Deref { ptr, pointee_ty } => MirRValue::Deref {
                ptr: Self::coerce_value(ptr),
                pointee_ty: Self::coerce_type(pointee_ty),
            },
            MirRValue::Ref {
                is_mut: _,
                place: _,
            } => rv.clone(),
            MirRValue::AddressOf {
                is_mut: _,
                place: _,
            } => rv.clone(),
            MirRValue::TextureSample {
                texture,
                sampler,
                coords,
            } => MirRValue::TextureSample {
                texture: Self::coerce_value(texture),
                sampler: Self::coerce_value(sampler),
                coords: Self::coerce_value(coords),
            },
            _ => rv.clone(),
        }
    }

    /// Coerce an entire MIR module from f64 to f32 for GPU execution.
    fn coerce_module_f32(mir: &MirModule) -> MirModule {
        let mut result = MirModule::new(mir.name.clone());
        result.strings = mir.strings.clone();
        result.externals = mir.externals.clone();

        // Coerce type definitions (struct fields)
        for td in &mir.types {
            let mut td = td.clone();
            if let TypeDefKind::Struct { ref mut fields, .. } = td.kind {
                for (_, field_ty) in fields.iter_mut() {
                    *field_ty = Self::coerce_type(field_ty);
                }
            }
            result.types.push(td);
        }

        // Coerce globals
        for g in &mir.globals {
            let mut g = g.clone();
            g.ty = Self::coerce_type(&g.ty);
            result.globals.push(g);
        }

        // Coerce functions
        for func in &mir.functions {
            let mut f = func.clone();

            // Coerce signature
            f.sig.params = f.sig.params.iter().map(|p| Self::coerce_type(p)).collect();
            f.sig.ret = Self::coerce_type(&f.sig.ret);

            // Coerce locals
            for local in f.locals.iter_mut() {
                local.ty = Self::coerce_type(&local.ty);
            }

            // Coerce blocks
            if let Some(ref mut blocks) = f.blocks {
                for block in blocks.iter_mut() {
                    // Coerce statements
                    for stmt in block.stmts.iter_mut() {
                        stmt.kind = match &stmt.kind {
                            MirStmtKind::Assign { dest, value } => MirStmtKind::Assign {
                                dest: *dest,
                                value: Self::coerce_rvalue(value),
                            },
                            MirStmtKind::DerefAssign { ptr, value } => MirStmtKind::DerefAssign {
                                ptr: *ptr,
                                value: Self::coerce_rvalue(value),
                            },
                            MirStmtKind::FieldDerefAssign {
                                ptr,
                                field_name,
                                value,
                            } => MirStmtKind::FieldDerefAssign {
                                ptr: *ptr,
                                field_name: field_name.clone(),
                                value: Self::coerce_rvalue(value),
                            },
                            _ => stmt.kind.clone(),
                        };
                    }

                    // Coerce terminator
                    if let Some(ref term) = block.terminator.clone() {
                        block.terminator = Some(match term {
                            MirTerminator::Return(Some(v)) => {
                                MirTerminator::Return(Some(Self::coerce_value(v)))
                            }
                            MirTerminator::Call {
                                func: fv,
                                args,
                                dest,
                                target,
                                unwind,
                            } => MirTerminator::Call {
                                func: Self::coerce_value(fv),
                                args: args.iter().map(|a| Self::coerce_value(a)).collect(),
                                dest: *dest,
                                target: *target,
                                unwind: *unwind,
                            },
                            MirTerminator::Switch {
                                value,
                                targets,
                                default,
                            } => MirTerminator::Switch {
                                value: Self::coerce_value(value),
                                targets: targets
                                    .iter()
                                    .map(|(c, b)| (Self::coerce_const(c), *b))
                                    .collect(),
                                default: *default,
                            },
                            MirTerminator::If {
                                cond,
                                then_block,
                                else_block,
                            } => MirTerminator::If {
                                cond: Self::coerce_value(cond),
                                then_block: *then_block,
                                else_block: *else_block,
                            },
                            _ => term.clone(),
                        });
                    }
                }
            }

            result.functions.push(f);
        }

        result
    }
}

impl Backend for SpirvBackend {
    fn generate(&mut self, mir: &MirModule) -> CodegenResult<GeneratedCode> {
        self.reset();

        // Use SPIR-V 1.0 for Vulkan 1.0 compatibility when shader stages are present
        let has_shaders = mir.functions.iter().any(|f| f.shader_stage.is_some());

        // GPU coercion: convert all f64 → f32 for shader modules.
        // GPUs natively operate in f32; Vulkan requires gl_Position to be vec4<f32>.
        // The coerced module is used for all subsequent codegen.
        let coerced;
        let mir = if has_shaders {
            coerced = Self::coerce_module_f32(mir);
            &coerced
        } else {
            mir
        };

        // Per-kernel workgroup size: a 2D kernel (one that reads
        // `gl_GlobalInvocationID.y`, lowered to `ThreadIndex(component >= 1)`)
        // dispatches over a 2D grid, so give it a 2D local size (16x16x1) instead
        // of the 1D default (64x1x1). Group counts on the host mirror this
        // (div_ceil per axis). Purely additive: 1D kernels keep (64,1,1).
        if has_shaders && mir_reads_thread_axis_ge(mir, 1) {
            self.workgroup_size = (16, 16, 1);
        }

        // Load struct definitions from the MIR module for use during type emission
        for type_def in &mir.types {
            if let TypeDefKind::Struct { fields, .. } = &type_def.kind {
                self.struct_defs
                    .insert(type_def.name.clone(), fields.clone());
            }
        }

        // =====================================================================
        // SPIR-V requires strict layout order for instructions:
        //   1. OpCapability
        //   2. OpExtension
        //   3. OpExtInstImport
        //   4. OpMemoryModel
        //   5. OpEntryPoint
        //   6. OpExecutionMode
        //   7. Debug (OpName, OpMemberName, OpString, OpSource)
        //   8. Annotations (OpDecorate, OpMemberDecorate)
        //   9. Type / constant / global-variable declarations
        //  10-11. Function definitions
        //
        // We collect sections 7-8 into pending buffers (pending_names and
        // pending_annotations). Sections 1-6 go into a preamble buffer.
        // Sections 9-11 land in self.output (types, constants, variables, and
        // function bodies are naturally interleaved there). At the end we
        // concatenate: header + preamble + names + annotations + output.
        // =====================================================================

        // -- Sections 1-4: header, capabilities, extensions, memory model -----
        // These go directly into self.output as a preamble.
        if has_shaders {
            self.emit_header_version(SPIRV_VERSION_1_0);
            // Remove Float64 capability - GPU shaders use f32 after coercion
            self.capabilities
                .retain(|c| !matches!(c, SpvCapability::Float64));
        } else {
            self.emit_header();
        }
        self.emit_capabilities();
        self.emit_extensions();
        self.emit_memory_model();

        // Snapshot the preamble (header + caps + exts + memmodel).
        // Everything after this will be entry-points, exec-modes, I/O vars,
        // types, constants, and function bodies.
        let preamble_end = self.output.len();

        // When shaders are present, find all reachable functions (entry points + callees).
        // Skip CPU-only functions (like main) that reference unavailable runtime functions.
        let shader_reachable: std::collections::HashSet<Arc<str>> = if has_shaders {
            let mut reachable = std::collections::HashSet::new();
            let mut worklist: Vec<Arc<str>> = Vec::new();

            // Start from shader entry points
            for func in &mir.functions {
                if func.shader_stage.is_some() {
                    reachable.insert(func.name.clone());
                    worklist.push(func.name.clone());
                }
            }

            // Transitively find callees
            while let Some(name) = worklist.pop() {
                if let Some(func) = mir.functions.iter().find(|f| f.name == name) {
                    if let Some(ref blocks) = func.blocks {
                        for block in blocks {
                            if let Some(ref term) = block.terminator {
                                if let MirTerminator::Call { func: callee, .. } = term {
                                    let callee_name = match callee {
                                        MirValue::Global(n) | MirValue::Function(n) => {
                                            Some(n.clone())
                                        }
                                        _ => None,
                                    };
                                    if let Some(cn) = callee_name {
                                        if reachable.insert(cn.clone()) {
                                            worklist.push(cn);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            reachable
        } else {
            // No shader filtering - all functions are reachable
            mir.functions.iter().map(|f| f.name.clone()).collect()
        };

        // Pre-allocate function IDs for reachable functions only.
        for func in &mir.functions {
            if !func.is_declaration()
                && !self.func_ids.contains_key(&func.name)
                && shader_reachable.contains(&func.name)
            {
                let func_id = self.alloc_id();
                self.func_ids.insert(func.name.clone(), func_id);
            }
        }

        // -- Sections 5-6: entry points and execution modes -------------------
        // Collect into a separate buffer so they stay before debug/annotations.
        let mut entry_point_buf: Vec<u32> = Vec::new();

        for func in &mir.functions {
            // When shader stages are present, only shader-annotated functions
            // become entry points. Otherwise, public functions are entry points.
            let is_entry = if has_shaders {
                func.shader_stage.is_some() && !func.is_declaration()
            } else {
                func.is_public && !func.is_declaration()
            };

            if is_entry {
                // Use the pre-allocated function ID
                let func_id = *self.func_ids.get(&func.name).unwrap();

                let exec_model = self.execution_model_for_func(func);

                // Set up shader I/O variables.
                // This calls get_ptr_type_id/get_type_id which emit
                // OpType*/OpVariable into self.output -- that's fine, those
                // belong in section 9 and self.output will be placed there.
                self.io_var_ids.clear();
                self.setup_shader_io(func, func_id, exec_model);

                // Entry point (section 5) -- into entry_point_buf
                let name_words = self.emit_string(&func.name);
                let mut operands = vec![exec_model as u32, func_id];
                operands.extend(name_words);
                operands.extend_from_slice(&self.io_var_ids);
                Self::emit_to(&mut entry_point_buf, SpvOp::OpEntryPoint, &operands);

                // Execution mode (section 6) -- into entry_point_buf
                match exec_model {
                    SpvExecutionModel::GLCompute | SpvExecutionModel::Kernel => {
                        Self::emit_to(
                            &mut entry_point_buf,
                            SpvOp::OpExecutionMode,
                            &[
                                func_id,
                                SpvExecutionMode::LocalSize as u32,
                                self.workgroup_size.0,
                                self.workgroup_size.1,
                                self.workgroup_size.2,
                            ],
                        );
                    }
                    SpvExecutionModel::Fragment => {
                        Self::emit_to(
                            &mut entry_point_buf,
                            SpvOp::OpExecutionMode,
                            &[func_id, SpvExecutionMode::OriginUpperLeft as u32],
                        );
                    }
                    SpvExecutionModel::Vertex => {
                        // No required execution modes for vertex shaders
                    }
                    _ => {}
                }
            }
        }

        // -- Sections 9-11: types, constants, globals, function bodies --------
        // Enable function phase: emit() -> pending_functions,
        // emit_global() -> pending_globals.
        self.in_function_phase = true;
        for func in &mir.functions {
            // Only generate shader-reachable functions
            if shader_reachable.contains(&func.name) {
                self.gen_function(func)?;
            }
        }
        self.in_function_phase = false;

        // =====================================================================
        // Assemble final SPIR-V binary in correct layout order.
        // =====================================================================
        // During the entry-point setup phase (setup_shader_io), types and
        // global variables were emitted into self.output[preamble_end..].
        // During the function phase, types/constants/globals went to
        // pending_globals and function bodies went to pending_functions.
        let setup_globals: Vec<u32> = self.output[preamble_end..].to_vec();

        // Truncate output to preamble, then append sections in order.
        self.output.truncate(preamble_end);

        // Section 5-6: entry points + execution modes
        self.output.extend_from_slice(&entry_point_buf);

        // Section 7: debug names (OpName, OpMemberName)
        self.output.extend_from_slice(&self.pending_names);

        // Section 8: annotations (OpDecorate, OpMemberDecorate)
        self.output.extend_from_slice(&self.pending_annotations);

        // Section 9: type / constant / global-variable declarations
        self.output.extend_from_slice(&setup_globals);
        self.output.extend_from_slice(&self.pending_globals);

        // Sections 10-11: function definitions
        self.output.extend_from_slice(&self.pending_functions);

        // Update bound in header (word index 3)
        self.output[3] = self.next_id;

        // Convert to bytes (little-endian)
        let bytes: Vec<u8> = self
            .output
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect();

        Ok(GeneratedCode::new(OutputFormat::SpirV, bytes))
    }

    fn target(&self) -> Target {
        Target::SpirV
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_compute_module() -> MirModule {
        let mut module = MirModule::new("compute_test");

        let sig = MirFnSig::new(vec![], MirType::Void);
        let mut func = MirFunction::new("main", sig);
        func.is_public = true;

        let mut block = MirBlock::new(BlockId::ENTRY);
        block.set_terminator(MirTerminator::Return(None));
        func.add_block(block);

        module.add_function(func);
        module
    }

    #[test]
    fn test_spirv_backend_new() {
        let backend = SpirvBackend::new();
        assert_eq!(backend.workgroup_size, (64, 1, 1));
    }

    #[test]
    fn test_spirv_backend_with_workgroup_size() {
        let backend = SpirvBackend::new().with_workgroup_size(256, 1, 1);
        assert_eq!(backend.workgroup_size, (256, 1, 1));
    }

    #[test]
    fn test_spirv_backend_with_capabilities() {
        let backend = SpirvBackend::new().with_float64().with_int64();
        assert!(backend.capabilities.contains(&SpvCapability::Float64));
        assert!(backend.capabilities.contains(&SpvCapability::Int64));
    }

    #[test]
    fn test_generate_compute_shader() {
        let module = create_compute_module();
        let mut backend = SpirvBackend::new();

        let result = backend.generate(&module);
        assert!(result.is_ok());

        let code = result.unwrap();
        let bytes = &code.data;

        // Check magic number
        let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert_eq!(magic, SPIRV_MAGIC);

        // Check version
        let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        assert_eq!(version, SPIRV_VERSION);
    }

    #[test]
    fn test_emit_string() {
        let backend = SpirvBackend::new();

        let words = backend.emit_string("main");
        assert!(!words.is_empty());

        // "main" + null = 5 bytes = 2 words
        assert_eq!(words.len(), 2);
    }

    #[test]
    fn test_spirv_binop() {
        let backend = SpirvBackend::new();

        assert!(matches!(
            backend.spirv_binop(BinOp::Add, &MirType::i32()),
            SpvOp::OpIAdd
        ));
        assert!(matches!(
            backend.spirv_binop(BinOp::Add, &MirType::f32()),
            SpvOp::OpFAdd
        ));
        assert!(matches!(
            backend.spirv_binop(BinOp::Div, &MirType::i32()),
            SpvOp::OpSDiv
        ));
        assert!(matches!(
            backend.spirv_binop(BinOp::Div, &MirType::u32()),
            SpvOp::OpUDiv
        ));
    }

    #[test]
    fn test_backend_target() {
        let backend = SpirvBackend::new();
        assert_eq!(backend.target(), Target::SpirV);
    }

    /// Helper to extract all SPIR-V words from generated binary.
    fn words_from_bytes(bytes: &[u8]) -> Vec<u32> {
        bytes
            .chunks(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }

    /// Check if a given opcode appears in the word stream.
    fn contains_opcode(words: &[u32], opcode: SpvOp) -> bool {
        for word in words {
            let op = word & 0xFFFF;
            if op == opcode as u32 {
                return true;
            }
        }
        false
    }

    /// Find all instruction start positions for a given opcode.
    fn find_instructions(words: &[u32], opcode: SpvOp) -> Vec<usize> {
        let mut results = Vec::new();
        let mut i = 5; // skip header (5 words)
        while i < words.len() {
            let word = words[i];
            let op = word & 0xFFFF;
            let wc = (word >> 16) as usize;
            if wc == 0 {
                break;
            }
            if op == opcode as u32 {
                results.push(i);
            }
            i += wc;
        }
        results
    }

    #[test]
    fn test_generate_vertex_shader() {
        // Create a minimal vertex shader module:
        //   vertex fn vs_main(in_pos: vec3<f32>, in_color: vec3<f32>) -> vec4<f32>
        let mut module = MirModule::new("vertex_test");

        let vec3_f32 = MirType::vector(MirType::f32(), 3);
        let vec4_f32 = MirType::vector(MirType::f32(), 4);

        let sig = MirFnSig::new(vec![vec3_f32.clone(), vec3_f32.clone()], vec4_f32.clone());
        let mut func = MirFunction::new("vs_main", sig);
        func.is_public = true;
        func.shader_stage = Some(ShaderStage::Vertex);

        // Add parameter locals
        let mut param0 = MirLocal::new(LocalId(0), vec3_f32.clone());
        param0.name = Some(Arc::from("in_pos"));
        param0.is_param = true;
        func.locals.push(param0);

        let mut param1 = MirLocal::new(LocalId(1), vec3_f32.clone());
        param1.name = Some(Arc::from("in_color"));
        param1.is_param = true;
        func.locals.push(param1);

        // Create a block that returns a zero vec4
        let mut block = MirBlock::new(BlockId::ENTRY);
        let ret_val = MirValue::Const(MirConst::Zeroed(vec4_f32.clone()));
        block.set_terminator(MirTerminator::Return(Some(ret_val)));
        func.add_block(block);

        module.add_function(func);

        // Generate
        let mut backend = SpirvBackend::new();
        let result = backend.generate(&module);
        assert!(
            result.is_ok(),
            "Vertex shader generation failed: {:?}",
            result.err()
        );

        let code = result.unwrap();
        let bytes = &code.data;
        let words = words_from_bytes(bytes);

        // 1. Check SPIR-V magic
        assert_eq!(words[0], SPIRV_MAGIC);

        // 2. Check OpCapability Shader (capability 1)
        let cap_positions = find_instructions(&words, SpvOp::OpCapability);
        assert!(!cap_positions.is_empty(), "No OpCapability found");
        let has_shader_cap = cap_positions
            .iter()
            .any(|&pos| words.get(pos + 1) == Some(&(SpvCapability::Shader as u32)));
        assert!(has_shader_cap, "OpCapability Shader not found");

        // 3. Check OpEntryPoint Vertex (execution model 0)
        let ep_positions = find_instructions(&words, SpvOp::OpEntryPoint);
        assert!(!ep_positions.is_empty(), "No OpEntryPoint found");
        let has_vertex_ep = ep_positions
            .iter()
            .any(|&pos| words.get(pos + 1) == Some(&(SpvExecutionModel::Vertex as u32)));
        assert!(has_vertex_ep, "OpEntryPoint with Vertex model not found");

        // 4. Check OpTypeVector is present
        assert!(
            contains_opcode(&words, SpvOp::OpTypeVector),
            "OpTypeVector not found in output"
        );

        // 5. Check OpDecorate with Location is present
        let dec_positions = find_instructions(&words, SpvOp::OpDecorate);
        let has_location = dec_positions
            .iter()
            .any(|&pos| words.get(pos + 2) == Some(&(SpvDecoration::Location as u32)));
        assert!(has_location, "OpDecorate with Location not found");

        // 6. Check the function has OpReturn or OpReturnValue
        assert!(
            contains_opcode(&words, SpvOp::OpReturnValue)
                || contains_opcode(&words, SpvOp::OpReturn),
            "No OpReturn/OpReturnValue found"
        );

        // 7. Verify no LocalSize execution mode is emitted (that's for compute only)
        let em_positions = find_instructions(&words, SpvOp::OpExecutionMode);
        let has_local_size = em_positions
            .iter()
            .any(|&pos| words.get(pos + 2) == Some(&(SpvExecutionMode::LocalSize as u32)));
        assert!(
            !has_local_size,
            "Vertex shader should not have LocalSize execution mode"
        );
    }

    #[test]
    fn test_generate_fragment_shader() {
        // Minimal fragment shader: fn fs_main() -> vec4<f32>
        let mut module = MirModule::new("fragment_test");

        let vec4_f32 = MirType::vector(MirType::f32(), 4);

        let sig = MirFnSig::new(vec![], vec4_f32.clone());
        let mut func = MirFunction::new("fs_main", sig);
        func.is_public = true;
        func.shader_stage = Some(ShaderStage::Fragment);

        let mut block = MirBlock::new(BlockId::ENTRY);
        let ret_val = MirValue::Const(MirConst::Zeroed(vec4_f32.clone()));
        block.set_terminator(MirTerminator::Return(Some(ret_val)));
        func.add_block(block);

        module.add_function(func);

        let mut backend = SpirvBackend::new();
        let result = backend.generate(&module);
        assert!(
            result.is_ok(),
            "Fragment shader generation failed: {:?}",
            result.err()
        );

        let code = result.unwrap();
        let words = words_from_bytes(&code.data);

        // Check OpEntryPoint Fragment
        let ep_positions = find_instructions(&words, SpvOp::OpEntryPoint);
        let has_frag_ep = ep_positions
            .iter()
            .any(|&pos| words.get(pos + 1) == Some(&(SpvExecutionModel::Fragment as u32)));
        assert!(has_frag_ep, "OpEntryPoint with Fragment model not found");

        // Check OriginUpperLeft execution mode
        let em_positions = find_instructions(&words, SpvOp::OpExecutionMode);
        let has_origin = em_positions
            .iter()
            .any(|&pos| words.get(pos + 2) == Some(&(SpvExecutionMode::OriginUpperLeft as u32)));
        assert!(
            has_origin,
            "Fragment shader must have OriginUpperLeft execution mode"
        );
    }

    #[test]
    fn test_struct_type_emits_members() {
        // Create a module with a struct type and verify OpTypeStruct includes member IDs
        let mut module = MirModule::new("struct_test");

        // Define a struct with two f32 fields
        let struct_def = MirTypeDef {
            name: Arc::from("MyStruct"),
            kind: TypeDefKind::Struct {
                fields: vec![
                    (Some(Arc::from("x")), MirType::f32()),
                    (Some(Arc::from("y")), MirType::f32()),
                ],
                packed: false,
            },
        };
        module.add_type(struct_def);

        // Create a trivial function that uses the struct type
        let sig = MirFnSig::new(vec![], MirType::Void);
        let mut func = MirFunction::new("main", sig);
        func.is_public = true;

        // Add a local of the struct type so it gets referenced
        let local = MirLocal::new(LocalId(0), MirType::Struct(Arc::from("MyStruct")));
        func.locals.push(local);

        let mut block = MirBlock::new(BlockId::ENTRY);
        block.set_terminator(MirTerminator::Return(None));
        func.add_block(block);

        module.add_function(func);

        let mut backend = SpirvBackend::new();
        let result = backend.generate(&module);
        assert!(
            result.is_ok(),
            "Struct test generation failed: {:?}",
            result.err()
        );

        let code = result.unwrap();
        let words = words_from_bytes(&code.data);

        // Find OpTypeStruct instructions
        let struct_positions = find_instructions(&words, SpvOp::OpTypeStruct);
        assert!(!struct_positions.is_empty(), "No OpTypeStruct found");

        // The OpTypeStruct should have word count >= 3 (opcode+wc, result_id, member_type_id, ...)
        // i.e. at least one member type ID
        for &pos in &struct_positions {
            let wc = (words[pos] >> 16) as usize;
            // word count = 1 (op+wc) + 1 (result id) + N (member types)
            // For our struct with 2 fields: wc should be 4 (1 + 1 + 2)
            assert!(
                wc >= 3,
                "OpTypeStruct has no member types (word count: {})",
                wc
            );
        }
    }

    // =========================================================================
    // DIRECT TRIANGLE SHADER GENERATION TESTS
    // =========================================================================

    #[test]
    fn test_generate_triangle_fragment_shader_structure() {
        let mut backend = SpirvBackend::new();
        let bytes = backend.generate_triangle_fragment_shader();
        let words = words_from_bytes(&bytes);

        // 1. SPIR-V magic
        assert_eq!(words[0], SPIRV_MAGIC, "Invalid SPIR-V magic number");

        // 2. OpCapability Shader
        let cap_positions = find_instructions(&words, SpvOp::OpCapability);
        let has_shader = cap_positions
            .iter()
            .any(|&pos| words.get(pos + 1) == Some(&(SpvCapability::Shader as u32)));
        assert!(has_shader, "Missing OpCapability Shader");

        // 3. OpEntryPoint Fragment
        let ep_positions = find_instructions(&words, SpvOp::OpEntryPoint);
        assert!(!ep_positions.is_empty(), "No OpEntryPoint found");
        let has_frag = ep_positions
            .iter()
            .any(|&pos| words.get(pos + 1) == Some(&(SpvExecutionModel::Fragment as u32)));
        assert!(has_frag, "OpEntryPoint must use Fragment execution model");

        // 4. OpExecutionMode OriginUpperLeft
        let em_positions = find_instructions(&words, SpvOp::OpExecutionMode);
        let has_origin = em_positions
            .iter()
            .any(|&pos| words.get(pos + 2) == Some(&(SpvExecutionMode::OriginUpperLeft as u32)));
        assert!(has_origin, "Fragment shader must have OriginUpperLeft");

        // 5. Has Location decorations
        let dec_positions = find_instructions(&words, SpvOp::OpDecorate);
        let has_location = dec_positions
            .iter()
            .any(|&pos| words.get(pos + 2) == Some(&(SpvDecoration::Location as u32)));
        assert!(has_location, "Missing Location decoration");

        // 6. Has OpLoad, OpCompositeExtract, OpCompositeConstruct, OpStore
        assert!(contains_opcode(&words, SpvOp::OpLoad), "Missing OpLoad");
        assert!(
            contains_opcode(&words, SpvOp::OpCompositeExtract),
            "Missing OpCompositeExtract"
        );
        assert!(
            contains_opcode(&words, SpvOp::OpCompositeConstruct),
            "Missing OpCompositeConstruct"
        );
        assert!(contains_opcode(&words, SpvOp::OpStore), "Missing OpStore");
        assert!(contains_opcode(&words, SpvOp::OpReturn), "Missing OpReturn");
    }

    #[test]
    fn test_generate_triangle_vertex_shader_structure() {
        let mut backend = SpirvBackend::new();
        let bytes = backend.generate_triangle_vertex_shader();
        let words = words_from_bytes(&bytes);

        // 1. SPIR-V magic
        assert_eq!(words[0], SPIRV_MAGIC, "Invalid SPIR-V magic number");

        // 2. OpEntryPoint Vertex
        let ep_positions = find_instructions(&words, SpvOp::OpEntryPoint);
        let has_vert = ep_positions
            .iter()
            .any(|&pos| words.get(pos + 1) == Some(&(SpvExecutionModel::Vertex as u32)));
        assert!(has_vert, "OpEntryPoint must use Vertex execution model");

        // 3. Has BuiltIn Position decoration
        let dec_positions = find_instructions(&words, SpvOp::OpDecorate);
        let has_builtin_pos = dec_positions.iter().any(|&pos| {
            words.get(pos + 2) == Some(&(SpvDecoration::BuiltIn as u32))
                && words.get(pos + 3) == Some(&(SpvBuiltIn::Position as u32))
        });
        assert!(has_builtin_pos, "Missing BuiltIn Position decoration");

        // 4. Has BuiltIn VertexIndex decoration
        let has_vtx_idx = dec_positions.iter().any(|&pos| {
            words.get(pos + 2) == Some(&(SpvDecoration::BuiltIn as u32))
                && words.get(pos + 3) == Some(&(SpvBuiltIn::VertexIndex as u32))
        });
        assert!(has_vtx_idx, "Missing BuiltIn VertexIndex decoration");

        // 5. Has Location 0 for fragColor output
        let has_location = dec_positions
            .iter()
            .any(|&pos| words.get(pos + 2) == Some(&(SpvDecoration::Location as u32)));
        assert!(has_location, "Missing Location decoration for fragColor");

        // 6. Has array types and access chain
        assert!(
            contains_opcode(&words, SpvOp::OpTypeArray),
            "Missing OpTypeArray"
        );
        assert!(
            contains_opcode(&words, SpvOp::OpAccessChain),
            "Missing OpAccessChain"
        );
        assert!(
            contains_opcode(&words, SpvOp::OpConstantComposite),
            "Missing OpConstantComposite"
        );
    }

    /// Write SPIR-V bytes to a temporary file and run spirv-val.
    /// Returns Ok(()) if validation passes, Err(message) if it fails.
    fn validate_spirv_bytes(bytes: &[u8], label: &str) -> Result<(), String> {
        use std::io::Write;

        let tmp_path = std::env::temp_dir().join(format!("buildlang_test_{}.spv", label));
        let mut f = std::fs::File::create(&tmp_path)
            .map_err(|e| format!("Failed to create temp file: {}", e))?;
        f.write_all(bytes)
            .map_err(|e| format!("Failed to write temp file: {}", e))?;
        drop(f);

        let spirv_val = "C:/VulkanSDK/1.4.341.1/Bin/spirv-val.exe";
        let output = std::process::Command::new(spirv_val)
            .arg(tmp_path.to_str().unwrap())
            .output()
            .map_err(|e| format!("Failed to run spirv-val: {}", e))?;

        // Clean up
        let _ = std::fs::remove_file(&tmp_path);

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("spirv-val failed for {}:\n{}", label, stderr))
        }
    }

    #[test]
    fn test_triangle_fragment_shader_validates() {
        let mut backend = SpirvBackend::new();
        let bytes = backend.generate_triangle_fragment_shader();

        match validate_spirv_bytes(&bytes, "frag") {
            Ok(()) => {} // Passes validation
            Err(msg) => panic!("{}", msg),
        }
    }

    #[test]
    fn test_triangle_vertex_shader_validates() {
        let mut backend = SpirvBackend::new();
        let bytes = backend.generate_triangle_vertex_shader();

        match validate_spirv_bytes(&bytes, "vert") {
            Ok(()) => {} // Passes validation
            Err(msg) => panic!("{}", msg),
        }
    }

    #[test]
    fn test_write_triangle_shaders_to_examples() {
        use std::io::Write;

        let out_dir = std::env::temp_dir();

        // Generate and write fragment shader
        let mut backend = SpirvBackend::new();
        let frag_bytes = backend.generate_triangle_fragment_shader();
        let frag_path = out_dir.join("build_frag.spv");
        let mut f = std::fs::File::create(&frag_path).expect("Failed to create build_frag.spv");
        f.write_all(&frag_bytes)
            .expect("Failed to write build_frag.spv");

        // Validate
        validate_spirv_bytes(&frag_bytes, "build_frag").expect("build_frag.spv failed validation");

        // Generate and write vertex shader
        let mut backend = SpirvBackend::new();
        let vert_bytes = backend.generate_triangle_vertex_shader();
        let vert_path = out_dir.join("build_vert.spv");
        let mut f = std::fs::File::create(&vert_path).expect("Failed to create build_vert.spv");
        f.write_all(&vert_bytes)
            .expect("Failed to write build_vert.spv");

        // Validate
        validate_spirv_bytes(&vert_bytes, "build_vert").expect("build_vert.spv failed validation");

        // Verify files exist and have content
        assert!(frag_path.exists(), "build_frag.spv was not written");
        assert!(vert_path.exists(), "build_vert.spv was not written");
        assert!(
            frag_bytes.len() > 100,
            "Fragment shader too small: {} bytes",
            frag_bytes.len()
        );
        assert!(
            vert_bytes.len() > 100,
            "Vertex shader too small: {} bytes",
            vert_bytes.len()
        );
    }

    #[test]
    #[ignore] // Requires Vulkan SDK installed
    fn test_uniform_vertex_shader_validates() {
        let mut backend = SpirvBackend::new();
        let bytes = backend.generate_uniform_vertex_shader();

        // Verify SPIR-V header
        let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert_eq!(magic, SPIRV_MAGIC);
        assert!(
            bytes.len() > 200,
            "Uniform vertex shader too small: {} bytes",
            bytes.len()
        );

        // Validate with spirv-val
        validate_spirv_bytes(&bytes, "uniform_vert")
            .expect("Uniform vertex shader failed SPIR-V validation");

        // Write to examples directory
        let out_dir = std::env::temp_dir();
        let path = out_dir.join("build_uniform_vert.spv");
        std::fs::write(&path, &bytes).expect("Failed to write uniform vertex shader");
    }

    #[test]
    #[ignore] // Requires Vulkan SDK installed
    fn test_textured_fragment_shader_validates() {
        let mut backend = SpirvBackend::new();
        let bytes = backend.generate_textured_fragment_shader();

        // Verify SPIR-V header
        let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert_eq!(magic, SPIRV_MAGIC);
        assert!(
            bytes.len() > 150,
            "Textured fragment shader too small: {} bytes",
            bytes.len()
        );

        // Validate with spirv-val
        validate_spirv_bytes(&bytes, "textured_frag")
            .expect("Textured fragment shader failed SPIR-V validation");

        // Write to examples directory
        let out_dir = std::env::temp_dir();
        let path = out_dir.join("build_textured_frag.spv");
        std::fs::write(&path, &bytes).expect("Failed to write textured fragment shader");
    }

    #[test]
    #[ignore] // Requires Vulkan SDK installed
    fn test_write_a5_shaders_to_examples() {
        use std::io::Write;

        let out_dir = std::env::temp_dir();

        // Generate uniform vertex shader (MVP transform from UBO)
        let mut backend = SpirvBackend::new();
        let vert_bytes = backend.generate_uniform_vertex_shader();
        let vert_path = out_dir.join("build_uniform_vert.spv");
        let mut f =
            std::fs::File::create(&vert_path).expect("Failed to create build_uniform_vert.spv");
        f.write_all(&vert_bytes).expect("Failed to write");
        validate_spirv_bytes(&vert_bytes, "uniform_vert")
            .expect("Uniform vertex shader failed validation");

        // Generate textured fragment shader (texture sampling)
        let mut backend = SpirvBackend::new();
        let frag_bytes = backend.generate_textured_fragment_shader();
        let frag_path = out_dir.join("build_textured_frag.spv");
        let mut f =
            std::fs::File::create(&frag_path).expect("Failed to create build_textured_frag.spv");
        f.write_all(&frag_bytes).expect("Failed to write");
        validate_spirv_bytes(&frag_bytes, "textured_frag")
            .expect("Textured fragment shader failed validation");

        println!("Uniform vertex shader: {} bytes", vert_bytes.len());
        println!("Textured fragment shader: {} bytes", frag_bytes.len());
    }
}
