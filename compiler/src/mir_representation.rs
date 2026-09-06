#![allow(dead_code)]

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use buildlang::ast::Module;
use buildlang::codegen::{
    lower::MirLowerer, AggregateKind, BinOp, BindingKind, CallingConv, CastKind, ExternalKind,
    FloatSize, IntSize, Linkage, MirConst, MirEnumVariant, MirFnSig, MirGlobal, MirModule,
    MirPlace, MirRValue, MirStmtKind, MirTerminator, MirType, MirTypeDef, MirUniform, MirValue,
    NullaryOp, PlaceProjection, ShaderBinding, ShaderStage, TypeDefKind, UnaryOp,
};
use buildlang::lexer::{Lexer, SourceFile};
use buildlang::parser::Parser;
use buildlang::types::{FunctionEffectSummary, TypeChecker, TypeContext};

use super::{
    input_graph_digest, preprocess_includes_recording_inputs, resolve_imports_recording_inputs,
    resolve_modules_recording_inputs, source_digest_hex, source_text_digest_hex, InputDigestLedger,
    SemanticCorpusManifest,
};

pub(crate) const MIR_REPRESENTATION_RECEIPT: &str = "mir-representation-2026-06-18.json";

const MIR_REPRESENTATION_SCHEMA: &str = "buildlang-mir-representation-receipt/v0";

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MirRepresentationReceipt {
    pub schema: String,
    pub receipt_id: String,
    pub created_at: String,
    pub compiler: String,
    pub language: String,
    pub source_set: MirRepresentationSourceSet,
    pub ir: MirRepresentationIr,
    pub programs: Vec<MirRepresentationProgram>,
    pub summary: MirRepresentationSummary,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MirRepresentationSourceSet {
    pub kind: String,
    pub manifest: String,
    pub program_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MirRepresentationIr {
    pub name: String,
    pub version: String,
    pub lowering_pipeline: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MirRepresentationDigest {
    pub algorithm: String,
    pub hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MirRepresentationProgram {
    pub id: String,
    pub path: String,
    pub source_digest: MirRepresentationDigest,
    pub input_graph_digest: MirRepresentationDigest,
    pub mir_digest: MirRepresentationDigest,
    pub module: MirRepresentationModuleCounts,
    pub symbols: MirRepresentationSymbols,
    pub operations: MirRepresentationOperations,
    pub memory_surfaces: MirRepresentationMemorySurfaces,
    pub control_flow: MirRepresentationControlFlow,
}

pub(crate) struct LoweredMirProgram {
    pub(crate) source_digest: MirRepresentationDigest,
    pub(crate) input_graph_digest: MirRepresentationDigest,
    pub(crate) ast: Module,
    pub(crate) function_effect_summaries: Vec<FunctionEffectSummary>,
    pub(crate) module: MirModule,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MirRepresentationModuleCounts {
    pub function_count: usize,
    pub defined_function_count: usize,
    pub declaration_count: usize,
    pub type_count: usize,
    pub global_count: usize,
    pub string_count: usize,
    pub external_count: usize,
    pub vtable_count: usize,
    pub uniform_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MirRepresentationSymbols {
    pub functions: Vec<String>,
    pub types: Vec<String>,
    pub globals: Vec<String>,
    pub externals: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MirRepresentationOperations {
    pub statements: Vec<String>,
    pub rvalues: Vec<String>,
    pub terminators: Vec<String>,
    pub binary_ops: Vec<String>,
    pub unary_ops: Vec<String>,
    pub casts: Vec<String>,
    pub aggregate_kinds: Vec<String>,
    pub place_projections: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MirRepresentationMemorySurfaces {
    pub references: bool,
    pub mutable_references: bool,
    pub deref_reads: bool,
    pub deref_writes: bool,
    pub field_reads: bool,
    pub field_writes: bool,
    pub index_reads: bool,
    pub aggregate_values: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MirRepresentationControlFlow {
    pub block_count: usize,
    pub branching: bool,
    pub switching: bool,
    pub calls: bool,
    pub loops: bool,
    pub unreachable: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MirRepresentationSummary {
    pub program_count: usize,
    pub statement_families: Vec<String>,
    pub rvalue_families: Vec<String>,
    pub terminator_families: Vec<String>,
    pub memory_surfaces: Vec<String>,
}

#[derive(Default)]
struct InventorySets {
    statements: BTreeSet<String>,
    rvalues: BTreeSet<String>,
    terminators: BTreeSet<String>,
    binary_ops: BTreeSet<String>,
    unary_ops: BTreeSet<String>,
    casts: BTreeSet<String>,
    aggregate_kinds: BTreeSet<String>,
    place_projections: BTreeSet<String>,
}

impl InventorySets {
    fn operations(self) -> MirRepresentationOperations {
        MirRepresentationOperations {
            statements: sorted(self.statements),
            rvalues: sorted(self.rvalues),
            terminators: sorted(self.terminators),
            binary_ops: sorted(self.binary_ops),
            unary_ops: sorted(self.unary_ops),
            casts: sorted(self.casts),
            aggregate_kinds: sorted(self.aggregate_kinds),
            place_projections: sorted(self.place_projections),
        }
    }
}

fn sorted(values: BTreeSet<String>) -> Vec<String> {
    values.into_iter().collect()
}

fn summarize_programs(programs: &[MirRepresentationProgram]) -> MirRepresentationSummary {
    let mut statement_families = BTreeSet::new();
    let mut rvalue_families = BTreeSet::new();
    let mut terminator_families = BTreeSet::new();
    let mut memory_surfaces = BTreeSet::new();

    for program in programs {
        statement_families.extend(program.operations.statements.iter().cloned());
        rvalue_families.extend(program.operations.rvalues.iter().cloned());
        terminator_families.extend(program.operations.terminators.iter().cloned());
        push_memory_surface_names(&mut memory_surfaces, &program.memory_surfaces);
    }

    MirRepresentationSummary {
        program_count: programs.len(),
        statement_families: sorted(statement_families),
        rvalue_families: sorted(rvalue_families),
        terminator_families: sorted(terminator_families),
        memory_surfaces: sorted(memory_surfaces),
    }
}

fn push_memory_surface_names(
    output: &mut BTreeSet<String>,
    surfaces: &MirRepresentationMemorySurfaces,
) {
    for (name, active) in [
        ("aggregate_values", surfaces.aggregate_values),
        ("deref_reads", surfaces.deref_reads),
        ("deref_writes", surfaces.deref_writes),
        ("field_reads", surfaces.field_reads),
        ("field_writes", surfaces.field_writes),
        ("index_reads", surfaces.index_reads),
        ("mutable_references", surfaces.mutable_references),
        ("references", surfaces.references),
    ] {
        if active {
            output.insert(name.to_string());
        }
    }
}

fn summarize_mir_program(
    id: &str,
    path: &str,
    source_digest: MirRepresentationDigest,
    input_graph_digest: MirRepresentationDigest,
    mir_digest: MirRepresentationDigest,
    module: &MirModule,
) -> MirRepresentationProgram {
    let module_counts = MirRepresentationModuleCounts {
        function_count: module.functions.len(),
        defined_function_count: module
            .functions
            .iter()
            .filter(|function| !function.is_declaration())
            .count(),
        declaration_count: module
            .functions
            .iter()
            .filter(|function| function.is_declaration())
            .count(),
        type_count: module.types.len(),
        global_count: module.globals.len(),
        string_count: module.strings.len(),
        external_count: module.externals.len(),
        vtable_count: module.vtables.len(),
        uniform_count: module.uniforms.len(),
    };
    let symbols = collect_mir_symbols(module);

    let mut sets = InventorySets::default();
    let mut memory_surfaces = MirRepresentationMemorySurfaces::default();
    let mut control_flow = MirRepresentationControlFlow::default();

    for function in &module.functions {
        let Some(blocks) = &function.blocks else {
            continue;
        };
        control_flow.block_count += blocks.len();
        for block in blocks {
            for stmt in &block.stmts {
                collect_stmt(&stmt.kind, &mut sets, &mut memory_surfaces);
            }
            if let Some(terminator) = &block.terminator {
                collect_loop_edges(terminator, block.id.0, &mut control_flow);
                collect_terminator(
                    terminator,
                    &mut sets,
                    &mut memory_surfaces,
                    &mut control_flow,
                );
            }
        }
    }

    MirRepresentationProgram {
        id: id.to_string(),
        path: path.to_string(),
        source_digest,
        input_graph_digest,
        mir_digest,
        module: module_counts,
        symbols,
        operations: sets.operations(),
        memory_surfaces: collect_mir_memory_surfaces(module),
        control_flow,
    }
}

pub(crate) fn collect_mir_symbols(module: &MirModule) -> MirRepresentationSymbols {
    MirRepresentationSymbols {
        functions: sorted(
            module
                .functions
                .iter()
                .map(|function| function.name.to_string())
                .collect(),
        ),
        types: sorted(module.types.iter().map(|ty| ty.name.to_string()).collect()),
        globals: sorted(
            module
                .globals
                .iter()
                .map(|global| global.name.to_string())
                .collect(),
        ),
        externals: sorted(
            module
                .externals
                .iter()
                .map(|external| external.name.to_string())
                .collect(),
        ),
    }
}

pub(crate) fn collect_mir_memory_surfaces(module: &MirModule) -> MirRepresentationMemorySurfaces {
    let mut sets = InventorySets::default();
    let mut memory_surfaces = MirRepresentationMemorySurfaces::default();
    let mut control_flow = MirRepresentationControlFlow::default();

    for function in &module.functions {
        let Some(blocks) = &function.blocks else {
            continue;
        };
        for block in blocks {
            for stmt in &block.stmts {
                collect_stmt(&stmt.kind, &mut sets, &mut memory_surfaces);
            }
            if let Some(terminator) = &block.terminator {
                collect_terminator(
                    terminator,
                    &mut sets,
                    &mut memory_surfaces,
                    &mut control_flow,
                );
            }
        }
    }

    memory_surfaces
}

fn collect_stmt(
    stmt: &MirStmtKind,
    sets: &mut InventorySets,
    memory: &mut MirRepresentationMemorySurfaces,
) {
    sets.statements.insert(statement_name(stmt).to_string());
    match stmt {
        MirStmtKind::Assign { value, .. } => collect_rvalue(value, sets, memory),
        MirStmtKind::DerefAssign { value, .. } => {
            memory.deref_writes = true;
            collect_rvalue(value, sets, memory);
        }
        MirStmtKind::FieldDerefAssign { value, .. } => {
            memory.deref_writes = true;
            memory.field_writes = true;
            collect_rvalue(value, sets, memory);
        }
        MirStmtKind::FieldAssign { value, .. } => {
            memory.field_writes = true;
            collect_rvalue(value, sets, memory);
        }
        MirStmtKind::GlobalStore { value, .. } => collect_rvalue(value, sets, memory),
        MirStmtKind::IndexStore { value, .. } => {
            // An indexed store is a memory write; fold into the existing
            // field-write surface rather than introducing a new surface (the
            // doctor surface inventory is asserted by exact count in tests).
            memory.field_writes = true;
            collect_rvalue(value, sets, memory);
        }
        // A workgroup barrier is a bare synchronization point with no operands
        // and no memory surface of its own (the shared writes it orders are the
        // index stores, already accounted for above).
        MirStmtKind::StorageLive(_)
        | MirStmtKind::StorageDead(_)
        | MirStmtKind::Nop
        | MirStmtKind::WorkgroupBarrier => {}
    }
}

fn collect_rvalue(
    rvalue: &MirRValue,
    sets: &mut InventorySets,
    memory: &mut MirRepresentationMemorySurfaces,
) {
    sets.rvalues.insert(rvalue_name(rvalue).to_string());
    match rvalue {
        MirRValue::Use(_) => {}
        MirRValue::BinaryOp { op, .. } => {
            sets.binary_ops.insert(bin_op_name(*op).to_string());
        }
        MirRValue::UnaryOp { op, .. } => {
            sets.unary_ops.insert(unary_op_name(*op).to_string());
        }
        MirRValue::Ref { is_mut, place } | MirRValue::AddressOf { is_mut, place } => {
            memory.references = true;
            memory.mutable_references |= *is_mut;
            collect_place(place, sets, memory);
        }
        MirRValue::Cast { kind, .. } => {
            sets.casts.insert(cast_kind_name(*kind).to_string());
        }
        MirRValue::Aggregate { kind, .. } => {
            memory.aggregate_values = true;
            sets.aggregate_kinds
                .insert(aggregate_kind_name(kind).to_string());
        }
        MirRValue::Repeat { .. } => {}
        MirRValue::Discriminant(place) | MirRValue::Len(place) => {
            collect_place(place, sets, memory);
        }
        MirRValue::NullaryOp(_, _) => {}
        MirRValue::FieldAccess { .. } | MirRValue::VariantField { .. } => {
            memory.field_reads = true;
        }
        MirRValue::IndexAccess { .. } => {
            memory.index_reads = true;
        }
        MirRValue::Deref { .. } => {
            memory.deref_reads = true;
        }
        MirRValue::TextureSample { .. } => {}
    }
}

fn collect_place(
    place: &MirPlace,
    sets: &mut InventorySets,
    memory: &mut MirRepresentationMemorySurfaces,
) {
    for projection in &place.projections {
        sets.place_projections
            .insert(projection_name(projection).to_string());
        match projection {
            PlaceProjection::Deref => memory.deref_reads = true,
            PlaceProjection::Field(..) => memory.field_reads = true,
            PlaceProjection::Index(_)
            | PlaceProjection::ConstantIndex { .. }
            | PlaceProjection::Subslice { .. } => memory.index_reads = true,
            PlaceProjection::Downcast(_) => {}
        }
    }
}

fn collect_terminator(
    terminator: &MirTerminator,
    sets: &mut InventorySets,
    memory: &mut MirRepresentationMemorySurfaces,
    control_flow: &mut MirRepresentationControlFlow,
) {
    sets.terminators
        .insert(terminator_name(terminator).to_string());
    match terminator {
        MirTerminator::Goto(_) => {}
        MirTerminator::If { .. } => {
            control_flow.branching = true;
        }
        MirTerminator::Switch { .. } => {
            control_flow.switching = true;
        }
        MirTerminator::Call { .. } => {
            control_flow.calls = true;
        }
        MirTerminator::Return(_) => {}
        MirTerminator::Unreachable => {
            control_flow.unreachable = true;
        }
        MirTerminator::Drop { place, .. } => {
            collect_place(place, sets, memory);
        }
        MirTerminator::Assert { .. } => {
            control_flow.branching = true;
        }
        MirTerminator::Resume | MirTerminator::Abort => {}
    }
}

fn collect_loop_edges(
    terminator: &MirTerminator,
    current_block: u32,
    control_flow: &mut MirRepresentationControlFlow,
) {
    let mut note_target = |target: u32| {
        if target <= current_block {
            control_flow.loops = true;
        }
    };

    match terminator {
        MirTerminator::Goto(target) => note_target(target.0),
        MirTerminator::If {
            then_block,
            else_block,
            ..
        } => {
            note_target(then_block.0);
            note_target(else_block.0);
        }
        MirTerminator::Switch {
            targets, default, ..
        } => {
            for (_, target) in targets {
                note_target(target.0);
            }
            note_target(default.0);
        }
        MirTerminator::Call { target, unwind, .. } => {
            if let Some(target) = target {
                note_target(target.0);
            }
            if let Some(unwind) = unwind {
                note_target(unwind.0);
            }
        }
        MirTerminator::Drop { target, unwind, .. }
        | MirTerminator::Assert { target, unwind, .. } => {
            note_target(target.0);
            if let Some(unwind) = unwind {
                note_target(unwind.0);
            }
        }
        MirTerminator::Return(_)
        | MirTerminator::Unreachable
        | MirTerminator::Resume
        | MirTerminator::Abort => {}
    }
}

fn statement_name(stmt: &MirStmtKind) -> &'static str {
    match stmt {
        MirStmtKind::Assign { .. } => "Assign",
        MirStmtKind::DerefAssign { .. } => "DerefAssign",
        MirStmtKind::FieldDerefAssign { .. } => "FieldDerefAssign",
        MirStmtKind::FieldAssign { .. } => "FieldAssign",
        MirStmtKind::GlobalStore { .. } => "GlobalStore",
        MirStmtKind::IndexStore { .. } => "IndexStore",
        MirStmtKind::StorageLive(_) => "StorageLive",
        MirStmtKind::StorageDead(_) => "StorageDead",
        MirStmtKind::Nop => "Nop",
        MirStmtKind::WorkgroupBarrier => "WorkgroupBarrier",
    }
}

fn rvalue_name(rvalue: &MirRValue) -> &'static str {
    match rvalue {
        MirRValue::Use(_) => "Use",
        MirRValue::BinaryOp { .. } => "BinaryOp",
        MirRValue::UnaryOp { .. } => "UnaryOp",
        MirRValue::Ref { .. } => "Ref",
        MirRValue::AddressOf { .. } => "AddressOf",
        MirRValue::Cast { .. } => "Cast",
        MirRValue::Aggregate { .. } => "Aggregate",
        MirRValue::Repeat { .. } => "Repeat",
        MirRValue::Discriminant(_) => "Discriminant",
        MirRValue::Len(_) => "Len",
        MirRValue::NullaryOp(_, _) => "NullaryOp",
        MirRValue::FieldAccess { .. } => "FieldAccess",
        MirRValue::VariantField { .. } => "VariantField",
        MirRValue::IndexAccess { .. } => "IndexAccess",
        MirRValue::Deref { .. } => "Deref",
        MirRValue::TextureSample { .. } => "TextureSample",
    }
}

fn terminator_name(terminator: &MirTerminator) -> &'static str {
    match terminator {
        MirTerminator::Goto(_) => "Goto",
        MirTerminator::If { .. } => "If",
        MirTerminator::Switch { .. } => "Switch",
        MirTerminator::Call { .. } => "Call",
        MirTerminator::Return(_) => "Return",
        MirTerminator::Unreachable => "Unreachable",
        MirTerminator::Drop { .. } => "Drop",
        MirTerminator::Assert { .. } => "Assert",
        MirTerminator::Resume => "Resume",
        MirTerminator::Abort => "Abort",
    }
}

fn bin_op_name(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "Add",
        BinOp::Sub => "Sub",
        BinOp::Mul => "Mul",
        BinOp::Div => "Div",
        BinOp::Rem => "Rem",
        BinOp::Pow => "Pow",
        BinOp::BitAnd => "BitAnd",
        BinOp::BitOr => "BitOr",
        BinOp::BitXor => "BitXor",
        BinOp::Shl => "Shl",
        BinOp::Shr => "Shr",
        BinOp::Eq => "Eq",
        BinOp::Ne => "Ne",
        BinOp::Lt => "Lt",
        BinOp::Le => "Le",
        BinOp::Gt => "Gt",
        BinOp::Ge => "Ge",
        BinOp::AddChecked => "AddChecked",
        BinOp::SubChecked => "SubChecked",
        BinOp::MulChecked => "MulChecked",
        BinOp::AddWrapping => "AddWrapping",
        BinOp::SubWrapping => "SubWrapping",
        BinOp::MulWrapping => "MulWrapping",
        BinOp::AddSaturating => "AddSaturating",
        BinOp::SubSaturating => "SubSaturating",
    }
}

fn unary_op_name(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Not => "Not",
        UnaryOp::BitNot => "BitNot",
        UnaryOp::Neg => "Neg",
    }
}

fn cast_kind_name(kind: CastKind) -> &'static str {
    match kind {
        CastKind::IntToInt => "IntToInt",
        CastKind::FloatToFloat => "FloatToFloat",
        CastKind::IntToFloat => "IntToFloat",
        CastKind::FloatToInt => "FloatToInt",
        CastKind::PtrToInt => "PtrToInt",
        CastKind::IntToPtr => "IntToPtr",
        CastKind::PtrToPtr => "PtrToPtr",
        CastKind::FnToPtr => "FnToPtr",
        CastKind::Transmute => "Transmute",
    }
}

fn aggregate_kind_name(kind: &AggregateKind) -> &'static str {
    match kind {
        AggregateKind::Array(_) => "Array",
        AggregateKind::Tuple => "Tuple",
        AggregateKind::Struct(_) => "Struct",
        AggregateKind::Variant(_, _, _) => "Variant",
        AggregateKind::Closure(_) => "Closure",
    }
}

fn projection_name(projection: &PlaceProjection) -> &'static str {
    match projection {
        PlaceProjection::Deref => "Deref",
        PlaceProjection::Field(..) => "Field",
        PlaceProjection::Index(_) => "Index",
        PlaceProjection::ConstantIndex { .. } => "ConstantIndex",
        PlaceProjection::Subslice { .. } => "Subslice",
        PlaceProjection::Downcast(_) => "Downcast",
    }
}

fn sha256_digest(hex: String) -> MirRepresentationDigest {
    MirRepresentationDigest {
        algorithm: "sha256".to_string(),
        hex,
    }
}

fn mir_source_digest(bytes: &[u8]) -> MirRepresentationDigest {
    sha256_digest(source_text_digest_hex(bytes))
}

fn mir_representation_digest(algorithm: &'static str, hex: String) -> MirRepresentationDigest {
    MirRepresentationDigest {
        algorithm: algorithm.to_string(),
        hex,
    }
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).expect("serialize string")
}

fn push_line(output: &mut String, line: impl AsRef<str>) {
    output.push_str(line.as_ref());
    output.push('\n');
}

fn format_optional_string(value: Option<&str>) -> String {
    value.map(json_string).unwrap_or_else(|| "null".to_string())
}

fn format_optional_local(value: Option<u32>) -> String {
    value
        .map(|local| local.to_string())
        .unwrap_or_else(|| "null".to_string())
}

fn format_optional_block(value: Option<u32>) -> String {
    value
        .map(|block| block.to_string())
        .unwrap_or_else(|| "null".to_string())
}

fn linkage_name(linkage: Linkage) -> &'static str {
    match linkage {
        Linkage::Internal => "Internal",
        Linkage::External => "External",
        Linkage::Weak => "Weak",
        Linkage::LinkOnce => "LinkOnce",
    }
}

fn shader_stage_name(stage: ShaderStage) -> &'static str {
    match stage {
        ShaderStage::Vertex => "Vertex",
        ShaderStage::Fragment => "Fragment",
        ShaderStage::Compute => "Compute",
    }
}

fn calling_conv_name(calling_conv: CallingConv) -> &'static str {
    match calling_conv {
        CallingConv::Build => "Build",
        CallingConv::C => "C",
        CallingConv::Fast => "Fast",
        CallingConv::Cold => "Cold",
    }
}

fn binding_kind_name(kind: &BindingKind) -> &'static str {
    match kind {
        BindingKind::UniformBuffer(_) => "UniformBuffer",
        BindingKind::Texture2D => "Texture2D",
        BindingKind::Sampler => "Sampler",
        BindingKind::StorageBuffer(_) => "StorageBuffer",
    }
}

fn nullary_op_name(op: NullaryOp) -> &'static str {
    match op {
        NullaryOp::SizeOf => "SizeOf",
        NullaryOp::AlignOf => "AlignOf",
        NullaryOp::ThreadIndex(_) => "ThreadIndex",
        NullaryOp::LocalInvocationId(_) => "LocalInvocationId",
        NullaryOp::WorkgroupId(_) => "WorkgroupId",
    }
}

fn int_size_name(size: IntSize) -> &'static str {
    match size {
        IntSize::I8 => "I8",
        IntSize::I16 => "I16",
        IntSize::I32 => "I32",
        IntSize::I64 => "I64",
        IntSize::I128 => "I128",
        IntSize::ISize => "ISize",
    }
}

fn float_size_name(size: FloatSize) -> &'static str {
    match size {
        FloatSize::F32 => "F32",
        FloatSize::F64 => "F64",
    }
}

fn write_mir_fn_sig(output: &mut String, signature: &MirFnSig) {
    push_line(
        output,
        format!(
            "sig variadic={} calling_conv={}",
            signature.is_variadic,
            calling_conv_name(signature.calling_conv)
        ),
    );
    push_line(output, format!("sig.params {}", signature.params.len()));
    for (index, parameter) in signature.params.iter().enumerate() {
        write_mir_type(output, &format!("sig.param[{index}]"), parameter);
    }
    write_mir_type(output, "sig.ret", &signature.ret);
}

fn write_mir_type(output: &mut String, label: &str, ty: &MirType) {
    match ty {
        MirType::Void => push_line(output, format!("{label} Void")),
        MirType::Bool => push_line(output, format!("{label} Bool")),
        MirType::Int(size, signed) => push_line(
            output,
            format!("{label} Int size={} signed={signed}", int_size_name(*size)),
        ),
        MirType::Float(size) => push_line(
            output,
            format!("{label} Float size={}", float_size_name(*size)),
        ),
        MirType::Ptr(inner) => {
            push_line(output, format!("{label} Ptr"));
            write_mir_type(output, &format!("{label}.inner"), inner);
        }
        MirType::Array(element, count) => {
            push_line(output, format!("{label} Array count={count}"));
            write_mir_type(output, &format!("{label}.element"), element);
        }
        MirType::Slice(element) => {
            push_line(output, format!("{label} Slice"));
            write_mir_type(output, &format!("{label}.element"), element);
        }
        MirType::Struct(name) => push_line(
            output,
            format!("{label} Struct {}", json_string(name.as_ref())),
        ),
        MirType::FnPtr(signature) => {
            push_line(output, format!("{label} FnPtr"));
            write_mir_fn_sig(output, signature);
        }
        MirType::Never => push_line(output, format!("{label} Never")),
        MirType::Vector(element, lanes) => {
            push_line(output, format!("{label} Vector lanes={lanes}"));
            write_mir_type(output, &format!("{label}.element"), element);
        }
        MirType::Texture2D(element) => {
            push_line(output, format!("{label} Texture2D"));
            write_mir_type(output, &format!("{label}.element"), element);
        }
        MirType::Sampler => push_line(output, format!("{label} Sampler")),
        MirType::SampledImage(element) => {
            push_line(output, format!("{label} SampledImage"));
            write_mir_type(output, &format!("{label}.element"), element);
        }
        MirType::TraitObject(name) => push_line(
            output,
            format!("{label} TraitObject {}", json_string(name.as_ref())),
        ),
        MirType::Vec(element) => {
            push_line(output, format!("{label} Vec"));
            write_mir_type(output, &format!("{label}.element"), element);
        }
        MirType::Map(key, value) => {
            push_line(output, format!("{label} Map"));
            write_mir_type(output, &format!("{label}.key"), key);
            write_mir_type(output, &format!("{label}.value"), value);
        }
        MirType::Tuple(elements) => {
            push_line(output, format!("{label} Tuple {}", elements.len()));
            for (index, element) in elements.iter().enumerate() {
                write_mir_type(output, &format!("{label}.element[{index}]"), element);
            }
        }
    }
}

fn write_mir_const(output: &mut String, label: &str, value: &MirConst) {
    match value {
        MirConst::Bool(flag) => push_line(output, format!("{label} Bool {flag}")),
        MirConst::Int(number, ty) => {
            push_line(output, format!("{label} Int {number}"));
            write_mir_type(output, &format!("{label}.type"), ty);
        }
        MirConst::Uint(number, ty) => {
            push_line(output, format!("{label} Uint {number}"));
            write_mir_type(output, &format!("{label}.type"), ty);
        }
        MirConst::Float(number, ty) => {
            push_line(output, format!("{label} Float bits={}", number.to_bits()));
            write_mir_type(output, &format!("{label}.type"), ty);
        }
        MirConst::Str(index) => push_line(output, format!("{label} Str index={index}")),
        MirConst::ByteStr(bytes) => {
            let mut encoded = String::with_capacity(bytes.len() * 2);
            for byte in bytes {
                write!(&mut encoded, "{byte:02x}").expect("write to string");
            }
            push_line(output, format!("{label} ByteStr {encoded}"));
        }
        MirConst::Null(ty) => {
            push_line(output, format!("{label} Null"));
            write_mir_type(output, &format!("{label}.type"), ty);
        }
        MirConst::Unit => push_line(output, format!("{label} Unit")),
        MirConst::Zeroed(ty) => {
            push_line(output, format!("{label} Zeroed"));
            write_mir_type(output, &format!("{label}.type"), ty);
        }
        MirConst::Undef(ty) => {
            push_line(output, format!("{label} Undef"));
            write_mir_type(output, &format!("{label}.type"), ty);
        }
        MirConst::Struct(name, fields) => {
            push_line(
                output,
                format!(
                    "{label} Struct {} fields={}",
                    json_string(name.as_ref()),
                    fields.len()
                ),
            );
            for (index, field) in fields.iter().enumerate() {
                write_mir_const(output, &format!("{label}.field[{index}]"), field);
            }
        }
    }
}

fn write_mir_value(output: &mut String, label: &str, value: &MirValue) {
    match value {
        MirValue::Local(local) => push_line(output, format!("{label} Local {}", local.0)),
        MirValue::Const(constant) => {
            push_line(output, format!("{label} Const"));
            write_mir_const(output, &format!("{label}.const"), constant);
        }
        MirValue::Global(name) => push_line(
            output,
            format!("{label} Global {}", json_string(name.as_ref())),
        ),
        MirValue::Function(name) => push_line(
            output,
            format!("{label} Function {}", json_string(name.as_ref())),
        ),
    }
}

fn write_mir_place(output: &mut String, label: &str, place: &MirPlace) {
    push_line(
        output,
        format!(
            "{label} local={} projections={}",
            place.local.0,
            place.projections.len()
        ),
    );
    for (index, projection) in place.projections.iter().enumerate() {
        write_mir_projection(output, &format!("{label}.projection[{index}]"), projection);
    }
}

fn write_mir_projection(output: &mut String, label: &str, projection: &PlaceProjection) {
    match projection {
        PlaceProjection::Deref => push_line(output, format!("{label} Deref")),
        PlaceProjection::Field(index, name, ty) => {
            push_line(output, format!("{label} Field index={index} name={name}"));
            write_mir_type(output, &format!("{label}.type"), ty);
        }
        PlaceProjection::Index(local) => push_line(output, format!("{label} Index {}", local.0)),
        PlaceProjection::ConstantIndex { offset, from_end } => push_line(
            output,
            format!("{label} ConstantIndex offset={offset} from_end={from_end}"),
        ),
        PlaceProjection::Subslice { from, to, from_end } => push_line(
            output,
            format!("{label} Subslice from={from} to={to} from_end={from_end}"),
        ),
        PlaceProjection::Downcast(index) => push_line(output, format!("{label} Downcast {index}")),
    }
}

fn write_mir_rvalue(output: &mut String, label: &str, rvalue: &MirRValue) {
    push_line(output, format!("{label} {}", rvalue_name(rvalue)));
    match rvalue {
        MirRValue::Use(value) => write_mir_value(output, &format!("{label}.value"), value),
        MirRValue::BinaryOp { op, left, right } => {
            push_line(output, format!("{label}.op {}", bin_op_name(*op)));
            write_mir_value(output, &format!("{label}.left"), left);
            write_mir_value(output, &format!("{label}.right"), right);
        }
        MirRValue::UnaryOp { op, operand } => {
            push_line(output, format!("{label}.op {}", unary_op_name(*op)));
            write_mir_value(output, &format!("{label}.operand"), operand);
        }
        MirRValue::Ref { is_mut, place } | MirRValue::AddressOf { is_mut, place } => {
            push_line(output, format!("{label}.is_mut {is_mut}"));
            write_mir_place(output, &format!("{label}.place"), place);
        }
        MirRValue::Cast { kind, value, ty } => {
            push_line(output, format!("{label}.kind {}", cast_kind_name(*kind)));
            write_mir_value(output, &format!("{label}.value"), value);
            write_mir_type(output, &format!("{label}.type"), ty);
        }
        MirRValue::Aggregate { kind, operands } => {
            write_mir_aggregate_kind(output, &format!("{label}.kind"), kind);
            push_line(output, format!("{label}.operands {}", operands.len()));
            for (index, operand) in operands.iter().enumerate() {
                write_mir_value(output, &format!("{label}.operand[{index}]"), operand);
            }
        }
        MirRValue::Repeat { value, count } => {
            push_line(output, format!("{label}.count {count}"));
            write_mir_value(output, &format!("{label}.value"), value);
        }
        MirRValue::Discriminant(place) | MirRValue::Len(place) => {
            write_mir_place(output, &format!("{label}.place"), place);
        }
        MirRValue::NullaryOp(op, ty) => {
            push_line(output, format!("{label}.op {}", nullary_op_name(*op)));
            if let NullaryOp::ThreadIndex(component) = op {
                push_line(output, format!("{label}.component {component}"));
            }
            write_mir_type(output, &format!("{label}.type"), ty);
        }
        MirRValue::FieldAccess {
            base,
            field_name,
            field_ty,
        } => {
            write_mir_value(output, &format!("{label}.base"), base);
            push_line(
                output,
                format!("{label}.field {}", json_string(field_name.as_ref())),
            );
            write_mir_type(output, &format!("{label}.field_type"), field_ty);
        }
        MirRValue::VariantField {
            base,
            variant_name,
            field_index,
            field_ty,
        } => {
            write_mir_value(output, &format!("{label}.base"), base);
            push_line(
                output,
                format!(
                    "{label}.variant {} field_index={field_index}",
                    json_string(variant_name.as_ref())
                ),
            );
            write_mir_type(output, &format!("{label}.field_type"), field_ty);
        }
        MirRValue::IndexAccess {
            base,
            index,
            elem_ty,
        } => {
            write_mir_value(output, &format!("{label}.base"), base);
            write_mir_value(output, &format!("{label}.index"), index);
            write_mir_type(output, &format!("{label}.element_type"), elem_ty);
        }
        MirRValue::Deref { ptr, pointee_ty } => {
            write_mir_value(output, &format!("{label}.ptr"), ptr);
            write_mir_type(output, &format!("{label}.pointee_type"), pointee_ty);
        }
        MirRValue::TextureSample {
            texture,
            sampler,
            coords,
        } => {
            write_mir_value(output, &format!("{label}.texture"), texture);
            write_mir_value(output, &format!("{label}.sampler"), sampler);
            write_mir_value(output, &format!("{label}.coords"), coords);
        }
    }
}

fn write_mir_aggregate_kind(output: &mut String, label: &str, kind: &AggregateKind) {
    match kind {
        AggregateKind::Array(element) => {
            push_line(output, format!("{label} Array"));
            write_mir_type(output, &format!("{label}.element"), element);
        }
        AggregateKind::Tuple => push_line(output, format!("{label} Tuple")),
        AggregateKind::Struct(name) => push_line(
            output,
            format!("{label} Struct {}", json_string(name.as_ref())),
        ),
        AggregateKind::Variant(name, discriminant, variant) => push_line(
            output,
            format!(
                "{label} Variant enum={} discriminant={discriminant} variant={}",
                json_string(name.as_ref()),
                json_string(variant.as_ref())
            ),
        ),
        AggregateKind::Closure(name) => push_line(
            output,
            format!("{label} Closure {}", json_string(name.as_ref())),
        ),
    }
}

fn write_mir_stmt(output: &mut String, label: &str, stmt: &MirStmtKind) {
    push_line(output, format!("{label} {}", statement_name(stmt)));
    match stmt {
        MirStmtKind::Assign { dest, value } => {
            push_line(output, format!("{label}.dest {}", dest.0));
            write_mir_rvalue(output, &format!("{label}.value"), value);
        }
        MirStmtKind::DerefAssign { ptr, value } => {
            push_line(output, format!("{label}.ptr {}", ptr.0));
            write_mir_rvalue(output, &format!("{label}.value"), value);
        }
        MirStmtKind::FieldDerefAssign {
            ptr,
            field_name,
            value,
        } => {
            push_line(output, format!("{label}.ptr {}", ptr.0));
            push_line(
                output,
                format!("{label}.field {}", json_string(field_name.as_ref())),
            );
            write_mir_rvalue(output, &format!("{label}.value"), value);
        }
        MirStmtKind::FieldAssign {
            base,
            field_name,
            value,
        } => {
            push_line(output, format!("{label}.base {}", base.0));
            push_line(
                output,
                format!("{label}.field {}", json_string(field_name.as_ref())),
            );
            write_mir_rvalue(output, &format!("{label}.value"), value);
        }
        MirStmtKind::GlobalStore { name, value } => {
            push_line(
                output,
                format!("{label}.global {}", json_string(name.as_ref())),
            );
            write_mir_rvalue(output, &format!("{label}.value"), value);
        }
        MirStmtKind::IndexStore {
            base,
            index,
            elem_ty,
            value,
        } => {
            write_mir_value(output, &format!("{label}.base"), base);
            write_mir_value(output, &format!("{label}.index"), index);
            write_mir_type(output, &format!("{label}.elem_ty"), elem_ty);
            write_mir_rvalue(output, &format!("{label}.value"), value);
        }
        MirStmtKind::StorageLive(local) | MirStmtKind::StorageDead(local) => {
            push_line(output, format!("{label}.local {}", local.0));
        }
        // A workgroup barrier carries no operands; the statement name emitted at
        // the top of this function fully describes it.
        MirStmtKind::Nop | MirStmtKind::WorkgroupBarrier => {}
    }
}

fn write_mir_terminator(output: &mut String, label: &str, terminator: &MirTerminator) {
    push_line(output, format!("{label} {}", terminator_name(terminator)));
    match terminator {
        MirTerminator::Goto(target) => push_line(output, format!("{label}.target {}", target.0)),
        MirTerminator::If {
            cond,
            then_block,
            else_block,
        } => {
            write_mir_value(output, &format!("{label}.cond"), cond);
            push_line(output, format!("{label}.then {}", then_block.0));
            push_line(output, format!("{label}.else {}", else_block.0));
        }
        MirTerminator::Switch {
            value,
            targets,
            default,
        } => {
            write_mir_value(output, &format!("{label}.value"), value);
            push_line(output, format!("{label}.targets {}", targets.len()));
            for (index, (constant, block)) in targets.iter().enumerate() {
                write_mir_const(output, &format!("{label}.target[{index}].value"), constant);
                push_line(output, format!("{label}.target[{index}].block {}", block.0));
            }
            push_line(output, format!("{label}.default {}", default.0));
        }
        MirTerminator::Call {
            func,
            args,
            dest,
            target,
            unwind,
        } => {
            write_mir_value(output, &format!("{label}.func"), func);
            push_line(output, format!("{label}.args {}", args.len()));
            for (index, argument) in args.iter().enumerate() {
                write_mir_value(output, &format!("{label}.arg[{index}]"), argument);
            }
            push_line(
                output,
                format!(
                    "{label}.dest {}",
                    format_optional_local(dest.map(|value| value.0))
                ),
            );
            push_line(
                output,
                format!(
                    "{label}.target {}",
                    format_optional_block(target.map(|value| value.0))
                ),
            );
            push_line(
                output,
                format!(
                    "{label}.unwind {}",
                    format_optional_block(unwind.map(|value| value.0))
                ),
            );
        }
        MirTerminator::Return(value) => {
            if let Some(value) = value {
                write_mir_value(output, &format!("{label}.value"), value);
            } else {
                push_line(output, format!("{label}.value null"));
            }
        }
        MirTerminator::Unreachable | MirTerminator::Resume | MirTerminator::Abort => {}
        MirTerminator::Drop {
            place,
            target,
            unwind,
        } => {
            write_mir_place(output, &format!("{label}.place"), place);
            push_line(output, format!("{label}.target {}", target.0));
            push_line(
                output,
                format!(
                    "{label}.unwind {}",
                    format_optional_block(unwind.map(|value| value.0))
                ),
            );
        }
        MirTerminator::Assert {
            cond,
            expected,
            msg,
            target,
            unwind,
        } => {
            write_mir_value(output, &format!("{label}.cond"), cond);
            push_line(output, format!("{label}.expected {expected}"));
            push_line(output, format!("{label}.msg {}", json_string(msg.as_ref())));
            push_line(output, format!("{label}.target {}", target.0));
            push_line(
                output,
                format!(
                    "{label}.unwind {}",
                    format_optional_block(unwind.map(|value| value.0))
                ),
            );
        }
    }
}

fn write_shader_binding(output: &mut String, label: &str, binding: &ShaderBinding) {
    push_line(
        output,
        format!(
            "{label} set={} binding={} kind={}",
            binding.set,
            binding.binding,
            binding_kind_name(&binding.kind)
        ),
    );
    match &binding.kind {
        BindingKind::UniformBuffer(name) | BindingKind::StorageBuffer(name) => {
            push_line(
                output,
                format!("{label}.resource {}", json_string(name.as_ref())),
            );
        }
        BindingKind::Texture2D | BindingKind::Sampler => {}
    }
    write_mir_type(output, &format!("{label}.type"), &binding.ty);
}

fn write_mir_global(output: &mut String, index: usize, global: &MirGlobal) {
    push_line(
        output,
        format!(
            "global[{index}] name={} mutable={} linkage={}",
            json_string(global.name.as_ref()),
            global.is_mut,
            linkage_name(global.linkage)
        ),
    );
    write_mir_type(output, &format!("global[{index}].type"), &global.ty);
    if let Some(init) = &global.init {
        write_mir_const(output, &format!("global[{index}].init"), init);
    } else {
        push_line(output, format!("global[{index}].init null"));
    }
}

fn write_mir_external(
    output: &mut String,
    index: usize,
    external: &buildlang::codegen::MirExternal,
) {
    push_line(
        output,
        format!(
            "external[{index}] name={}",
            json_string(external.name.as_ref())
        ),
    );
    match &external.kind {
        ExternalKind::Function(signature) => {
            push_line(output, format!("external[{index}].kind Function"));
            write_mir_fn_sig(output, signature);
        }
        ExternalKind::Global(ty) => {
            push_line(output, format!("external[{index}].kind Global"));
            write_mir_type(output, &format!("external[{index}].type"), ty);
        }
    }
}

fn write_mir_enum_variant(output: &mut String, label: &str, variant: &MirEnumVariant) {
    push_line(
        output,
        format!(
            "{label} name={} discriminant={}",
            json_string(variant.name.as_ref()),
            variant.discriminant
        ),
    );
    push_line(output, format!("{label}.fields {}", variant.fields.len()));
    for (index, (name, ty)) in variant.fields.iter().enumerate() {
        push_line(
            output,
            format!(
                "{label}.field[{index}].name {}",
                format_optional_string(name.as_ref().map(|value| value.as_ref()))
            ),
        );
        write_mir_type(output, &format!("{label}.field[{index}].type"), ty);
    }
}

fn write_mir_type_def(output: &mut String, index: usize, ty: &MirTypeDef) {
    push_line(
        output,
        format!("type[{index}] name={}", json_string(ty.name.as_ref())),
    );
    match &ty.kind {
        TypeDefKind::Struct { fields, packed } => {
            push_line(output, format!("type[{index}].kind Struct packed={packed}"));
            push_line(output, format!("type[{index}].fields {}", fields.len()));
            for (field_index, (name, field_ty)) in fields.iter().enumerate() {
                push_line(
                    output,
                    format!(
                        "type[{index}].field[{field_index}].name {}",
                        format_optional_string(name.as_ref().map(|value| value.as_ref()))
                    ),
                );
                write_mir_type(
                    output,
                    &format!("type[{index}].field[{field_index}].type"),
                    field_ty,
                );
            }
        }
        TypeDefKind::Union { variants } => {
            push_line(output, format!("type[{index}].kind Union"));
            push_line(output, format!("type[{index}].variants {}", variants.len()));
            for (variant_index, (name, variant_ty)) in variants.iter().enumerate() {
                push_line(
                    output,
                    format!(
                        "type[{index}].variant[{variant_index}].name {}",
                        json_string(name.as_ref())
                    ),
                );
                write_mir_type(
                    output,
                    &format!("type[{index}].variant[{variant_index}].type"),
                    variant_ty,
                );
            }
        }
        TypeDefKind::Enum {
            discriminant_ty,
            variants,
        } => {
            push_line(output, format!("type[{index}].kind Enum"));
            write_mir_type(
                output,
                &format!("type[{index}].discriminant"),
                discriminant_ty,
            );
            push_line(output, format!("type[{index}].variants {}", variants.len()));
            for (variant_index, variant) in variants.iter().enumerate() {
                write_mir_enum_variant(
                    output,
                    &format!("type[{index}].variant[{variant_index}]"),
                    variant,
                );
            }
        }
    }
}

fn write_mir_uniform(output: &mut String, index: usize, uniform: &MirUniform) {
    push_line(
        output,
        format!(
            "uniform[{index}] name={}",
            json_string(uniform.name.as_ref())
        ),
    );
    write_mir_type(output, &format!("uniform[{index}].type"), &uniform.ty);
    if let Some(default) = &uniform.default {
        write_mir_const(output, &format!("uniform[{index}].default"), default);
    } else {
        push_line(output, format!("uniform[{index}].default null"));
    }
}

fn write_mir_module(module: &MirModule) -> String {
    let mut output = String::new();
    push_line(
        &mut output,
        format!("module {}", json_string(module.name.as_ref())),
    );
    push_line(
        &mut output,
        format!(
            "counts functions={} globals={} types={} strings={} externals={} vtables={} uniforms={}",
            module.functions.len(),
            module.globals.len(),
            module.types.len(),
            module.strings.len(),
            module.externals.len(),
            module.vtables.len(),
            module.uniforms.len()
        ),
    );

    for (index, function) in module.functions.iter().enumerate() {
        push_line(
            &mut output,
            format!(
                "function[{index}] name={} public={} linkage={} declaration={} shader_stage={}",
                json_string(function.name.as_ref()),
                function.is_public,
                linkage_name(function.linkage),
                function.is_declaration(),
                function
                    .shader_stage
                    .map(shader_stage_name)
                    .unwrap_or("null")
            ),
        );
        write_mir_fn_sig(&mut output, &function.sig);
        push_line(
            &mut output,
            format!("function[{index}].bindings {}", function.bindings.len()),
        );
        for (binding_index, binding) in function.bindings.iter().enumerate() {
            write_shader_binding(
                &mut output,
                &format!("function[{index}].binding[{binding_index}]"),
                binding,
            );
        }
        push_line(
            &mut output,
            format!("function[{index}].locals {}", function.locals.len()),
        );
        for (local_index, local) in function.locals.iter().enumerate() {
            push_line(
                &mut output,
                format!(
                    "function[{index}].local[{local_index}] id={} name={} mutable={} param={} annotations={}",
                    local.id.0,
                    format_optional_string(local.name.as_ref().map(|value| value.as_ref())),
                    local.is_mut,
                    local.is_param,
                    local.annotations.len()
                ),
            );
            write_mir_type(
                &mut output,
                &format!("function[{index}].local[{local_index}].type"),
                &local.ty,
            );
            for (annotation_index, annotation) in local.annotations.iter().enumerate() {
                push_line(
                    &mut output,
                    format!(
                        "function[{index}].local[{local_index}].annotation[{annotation_index}] {}",
                        json_string(annotation.as_ref())
                    ),
                );
            }
        }
        match &function.blocks {
            Some(blocks) => {
                push_line(
                    &mut output,
                    format!("function[{index}].blocks {}", blocks.len()),
                );
                for (block_index, block) in blocks.iter().enumerate() {
                    push_line(
                        &mut output,
                        format!(
                            "function[{index}].block[{block_index}] id={} label={}",
                            block.id.0,
                            format_optional_string(
                                block.label.as_ref().map(|value| value.as_ref())
                            )
                        ),
                    );
                    push_line(
                        &mut output,
                        format!(
                            "function[{index}].block[{block_index}].stmts {}",
                            block.stmts.len()
                        ),
                    );
                    for (stmt_index, stmt) in block.stmts.iter().enumerate() {
                        write_mir_stmt(
                            &mut output,
                            &format!("function[{index}].block[{block_index}].stmt[{stmt_index}]"),
                            &stmt.kind,
                        );
                    }
                    if let Some(terminator) = &block.terminator {
                        write_mir_terminator(
                            &mut output,
                            &format!("function[{index}].block[{block_index}].terminator"),
                            terminator,
                        );
                    } else {
                        push_line(
                            &mut output,
                            format!("function[{index}].block[{block_index}].terminator null"),
                        );
                    }
                }
            }
            None => push_line(&mut output, format!("function[{index}].blocks null")),
        }
    }

    for (index, global) in module.globals.iter().enumerate() {
        write_mir_global(&mut output, index, global);
    }

    for (index, ty) in module.types.iter().enumerate() {
        write_mir_type_def(&mut output, index, ty);
    }

    for (index, string) in module.strings.iter().enumerate() {
        push_line(
            &mut output,
            format!("string[{index}] {}", json_string(string.as_ref())),
        );
    }

    for (index, external) in module.externals.iter().enumerate() {
        write_mir_external(&mut output, index, external);
    }

    let mut trait_methods = module.trait_methods.iter().collect::<Vec<_>>();
    trait_methods.sort_by(|left, right| left.0.as_ref().cmp(right.0.as_ref()));
    push_line(
        &mut output,
        format!("trait_methods {}", trait_methods.len()),
    );
    for (trait_index, (trait_name, methods)) in trait_methods.into_iter().enumerate() {
        push_line(
            &mut output,
            format!(
                "trait_method[{trait_index}] trait={} methods={}",
                json_string(trait_name.as_ref()),
                methods.len()
            ),
        );
        let mut methods = methods.iter().collect::<Vec<_>>();
        methods.sort_by(|left, right| left.0.as_ref().cmp(right.0.as_ref()));
        for (method_index, (method_name, signature)) in methods.into_iter().enumerate() {
            push_line(
                &mut output,
                format!(
                    "trait_method[{trait_index}].method[{method_index}] name={}",
                    json_string(method_name.as_ref())
                ),
            );
            write_mir_fn_sig(&mut output, signature);
        }
    }

    let mut vtables = module.vtables.iter().collect::<Vec<_>>();
    vtables.sort_by(|left, right| {
        left.trait_name
            .as_ref()
            .cmp(right.trait_name.as_ref())
            .then_with(|| left.type_name.as_ref().cmp(right.type_name.as_ref()))
    });
    for (index, vtable) in vtables.into_iter().enumerate() {
        push_line(
            &mut output,
            format!(
                "vtable[{index}] trait={} type={} methods={}",
                json_string(vtable.trait_name.as_ref()),
                json_string(vtable.type_name.as_ref()),
                vtable.methods.len()
            ),
        );
        let mut methods = vtable.methods.iter().collect::<Vec<_>>();
        methods.sort_by(|left, right| {
            left.0
                .as_ref()
                .cmp(right.0.as_ref())
                .then_with(|| left.1.as_ref().cmp(right.1.as_ref()))
        });
        for (method_index, (method_name, function_name, signature)) in
            methods.into_iter().enumerate()
        {
            push_line(
                &mut output,
                format!(
                    "vtable[{index}].method[{method_index}] name={} function={}",
                    json_string(method_name.as_ref()),
                    json_string(function_name.as_ref())
                ),
            );
            write_mir_fn_sig(&mut output, signature);
        }
    }

    for (index, uniform) in module.uniforms.iter().enumerate() {
        write_mir_uniform(&mut output, index, uniform);
    }

    output
}

pub(crate) fn digest_mir_module(module: &MirModule) -> MirRepresentationDigest {
    sha256_digest(source_digest_hex(write_mir_module(module).as_bytes()))
}

pub(crate) fn build_mir_representation_receipt(
    corpus_root: &Path,
    manifest: &SemanticCorpusManifest,
) -> Result<MirRepresentationReceipt, String> {
    let mut programs = Vec::new();
    for program in &manifest.programs {
        let program_path =
            validate_corpus_relative_path(corpus_root, &program.path, "program.path")?;
        let lowered = lower_program_to_mir(&program_path)?;
        let mir_digest = digest_mir_module(&lowered.module);
        programs.push(summarize_mir_program(
            &program.id,
            &program.path,
            lowered.source_digest,
            lowered.input_graph_digest,
            mir_digest,
            &lowered.module,
        ));
    }

    let summary = summarize_programs(&programs);
    Ok(MirRepresentationReceipt {
        schema: MIR_REPRESENTATION_SCHEMA.to_string(),
        receipt_id: "mir-representation-semantic-corpus-2026-06-18".to_string(),
        created_at: "2026-06-18".to_string(),
        compiler: "buildc".to_string(),
        language: "buildlang".to_string(),
        source_set: MirRepresentationSourceSet {
            kind: "semantic-corpus".to_string(),
            manifest: "manifest.json".to_string(),
            program_count: manifest.programs.len(),
        },
        ir: MirRepresentationIr {
            name: "MIR".to_string(),
            version: "v0".to_string(),
            lowering_pipeline: "parse -> type-check -> ast-to-mir".to_string(),
        },
        programs,
        summary,
    })
}

pub(crate) fn validate_mir_representation_receipt(
    corpus_root: &Path,
    receipt: &MirRepresentationReceipt,
    manifest: &SemanticCorpusManifest,
) -> Result<(), String> {
    if receipt.schema != MIR_REPRESENTATION_SCHEMA {
        return Err(format!(
            "mir representation receipt has unsupported schema '{}'",
            receipt.schema
        ));
    }
    if receipt.compiler != "buildc" {
        return Err(format!(
            "mir representation compiler mismatch: expected 'buildc', found '{}'",
            receipt.compiler
        ));
    }
    if receipt.language != "buildlang" {
        return Err(format!(
            "mir representation language mismatch: expected 'buildlang', found '{}'",
            receipt.language
        ));
    }
    if receipt.source_set.kind != "semantic-corpus" {
        return Err(format!(
            "mir representation source_set.kind mismatch: expected 'semantic-corpus', found '{}'",
            receipt.source_set.kind
        ));
    }
    let manifest_path = validate_corpus_relative_path(
        corpus_root,
        &receipt.source_set.manifest,
        "source_set.manifest",
    )?;
    let expected_manifest = corpus_root
        .join("manifest.json")
        .canonicalize()
        .map_err(|err| {
            format!(
                "mir representation failed to canonicalize expected manifest {}: {err}",
                corpus_root.join("manifest.json").display()
            )
        })?;
    if manifest_path != expected_manifest {
        return Err(format!(
            "mir representation source_set.manifest must point at manifest.json, found {}",
            receipt.source_set.manifest
        ));
    }
    if receipt.source_set.program_count != manifest.programs.len() {
        return Err(format!(
            "mir representation source_set.program_count mismatch: expected {}, found {}",
            manifest.programs.len(),
            receipt.source_set.program_count
        ));
    }
    for program in &receipt.programs {
        validate_corpus_relative_path(corpus_root, &program.path, "program.path")?;
    }

    let expected = build_mir_representation_receipt(corpus_root, manifest)?;
    compare_receipts(receipt, &expected)
}

pub(crate) fn verify_mir_representation_receipt(
    corpus_root: &Path,
    receipt: &MirRepresentationReceipt,
    manifest: &SemanticCorpusManifest,
) -> Result<(), i32> {
    validate_mir_representation_receipt(corpus_root, receipt, manifest).map_err(|message| {
        eprintln!("{message}");
        1
    })
}

fn validate_corpus_relative_path(
    corpus_root: &Path,
    relative: &str,
    field: &str,
) -> Result<PathBuf, String> {
    if relative.trim().is_empty() {
        return Err(format!("mir representation {field} must not be empty"));
    }
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path.has_root()
        || relative_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "mir representation {field} must stay within corpus root: {relative}"
        ));
    }
    let canonical_root = corpus_root.canonicalize().map_err(|err| {
        format!(
            "mir representation {field} failed to canonicalize corpus root {}: {err}",
            corpus_root.display()
        )
    })?;
    let path = corpus_root.join(relative_path);
    if !path.is_file() {
        return Err(format!(
            "mir representation {field} path not found: {}",
            path.display()
        ));
    }
    let canonical_path = path.canonicalize().map_err(|err| {
        format!(
            "mir representation {field} failed to canonicalize path {}: {err}",
            path.display()
        )
    })?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(format!(
            "mir representation {field} must stay within corpus root: {relative}"
        ));
    }
    Ok(canonical_path)
}

pub(crate) fn lower_program_to_mir(program_path: &Path) -> Result<LoweredMirProgram, String> {
    let mut input_digest_ledger = InputDigestLedger::text_normalized();
    let source_bytes = std::fs::read(program_path).map_err(|err| {
        format!(
            "mir representation failed to read {}: {err}",
            program_path.display()
        )
    })?;
    input_digest_ledger.record("entry", program_path, &source_bytes);
    let source_digest = mir_source_digest(&source_bytes);
    let source = String::from_utf8(source_bytes).map_err(|err| {
        format!(
            "mir representation failed to decode {} as UTF-8: {err}",
            program_path.display()
        )
    })?;
    let source = resolve_imports_recording_inputs(&source, program_path, &mut input_digest_ledger)
        .map_err(|_| {
            format!(
                "mir representation failed to resolve imports for {}",
                program_path.display()
            )
        })?;
    let base_dir = program_path.parent().unwrap_or_else(|| Path::new("."));
    let source = preprocess_includes_recording_inputs(&source, base_dir, &mut input_digest_ledger)
        .map_err(|_| {
            format!(
                "mir representation failed to preprocess includes for {}",
                program_path.display()
            )
        })?;
    let source_file = SourceFile::new(program_path.to_string_lossy(), source);
    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer.tokenize().map_err(|err| {
        format!(
            "mir representation lexer error in {}: {err}",
            program_path.display()
        )
    })?;
    let mut parser = Parser::new(&source_file, tokens);
    let mut ast = parser.parse().map_err(|err| {
        format!(
            "mir representation parse error in {}: {err}",
            program_path.display()
        )
    })?;
    if !parser.errors().is_empty() {
        let errors = parser
            .errors()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!(
            "mir representation parse errors in {}: {errors}",
            program_path.display()
        ));
    }
    resolve_modules_recording_inputs(&mut ast, base_dir, &mut input_digest_ledger).map_err(
        |_| {
            format!(
                "mir representation failed to resolve modules for {}",
                program_path.display()
            )
        },
    )?;

    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.set_source_file(&source_file);
    checker.set_source_dir(base_dir.to_path_buf());
    checker.check_module(&ast);
    if checker.has_errors() {
        let errors = checker
            .errors()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!(
            "mir representation type errors in {}: {errors}",
            program_path.display()
        ));
    }
    let function_effect_summaries = checker.function_effect_summaries().to_vec();

    let input_graph_digest = input_graph_digest(&input_digest_ledger.into_sorted_records());
    let module = MirLowerer::with_source(&ctx, Arc::from(source_file.source()))
        .lower_module(&ast)
        .map_err(|err| {
            format!(
                "mir representation code generation error in {}: {err}",
                program_path.display()
            )
        })?;

    Ok(LoweredMirProgram {
        source_digest,
        input_graph_digest: mir_representation_digest(
            input_graph_digest.algorithm,
            input_graph_digest.hex,
        ),
        ast,
        function_effect_summaries,
        module,
    })
}

fn compare_receipts(
    receipt: &MirRepresentationReceipt,
    expected: &MirRepresentationReceipt,
) -> Result<(), String> {
    if receipt.receipt_id != expected.receipt_id {
        return Err(format!(
            "mir representation receipt_id mismatch: expected '{}', found '{}'",
            expected.receipt_id, receipt.receipt_id
        ));
    }
    if receipt.created_at != expected.created_at {
        return Err(format!(
            "mir representation created_at mismatch: expected '{}', found '{}'",
            expected.created_at, receipt.created_at
        ));
    }
    if receipt.ir != expected.ir {
        return Err("mir representation ir drift".to_string());
    }
    if receipt.programs.len() != expected.programs.len() {
        return Err(format!(
            "mir representation program count drift: expected {}, found {}",
            expected.programs.len(),
            receipt.programs.len()
        ));
    }

    for (actual_program, expected_program) in receipt.programs.iter().zip(&expected.programs) {
        if actual_program.id != expected_program.id || actual_program.path != expected_program.path
        {
            return Err(format!(
                "mir representation program order drift: expected {} at {}, found {} at {}",
                expected_program.id, expected_program.path, actual_program.id, actual_program.path
            ));
        }
        if actual_program.source_digest != expected_program.source_digest {
            return Err(format!(
                "mir representation program {} source_digest mismatch",
                actual_program.id
            ));
        }
        if actual_program.input_graph_digest != expected_program.input_graph_digest {
            return Err(format!(
                "mir representation program {} input_graph_digest mismatch",
                actual_program.id
            ));
        }
        if actual_program.mir_digest != expected_program.mir_digest {
            return Err(format!(
                "mir representation program {} mir_digest mismatch",
                actual_program.id
            ));
        }
        if actual_program.operations.statements != expected_program.operations.statements {
            return Err(format!(
                "mir representation program {} operations.statements drift",
                actual_program.id
            ));
        }
        if actual_program.operations.rvalues != expected_program.operations.rvalues {
            return Err(format!(
                "mir representation program {} operations.rvalues drift",
                actual_program.id
            ));
        }
        if actual_program.operations.terminators != expected_program.operations.terminators {
            return Err(format!(
                "mir representation program {} operations.terminators drift",
                actual_program.id
            ));
        }
        if actual_program.operations.binary_ops != expected_program.operations.binary_ops {
            return Err(format!(
                "mir representation program {} operations.binary_ops drift",
                actual_program.id
            ));
        }
        if actual_program.operations.unary_ops != expected_program.operations.unary_ops {
            return Err(format!(
                "mir representation program {} operations.unary_ops drift",
                actual_program.id
            ));
        }
        if actual_program.operations.casts != expected_program.operations.casts {
            return Err(format!(
                "mir representation program {} operations.casts drift",
                actual_program.id
            ));
        }
        if actual_program.operations.aggregate_kinds != expected_program.operations.aggregate_kinds
        {
            return Err(format!(
                "mir representation program {} operations.aggregate_kinds drift",
                actual_program.id
            ));
        }
        if actual_program.operations.place_projections
            != expected_program.operations.place_projections
        {
            return Err(format!(
                "mir representation program {} operations.place_projections drift",
                actual_program.id
            ));
        }
        if actual_program.module != expected_program.module {
            return Err(format!(
                "mir representation program {} module counts drift",
                actual_program.id
            ));
        }
        if actual_program.symbols != expected_program.symbols {
            return Err(format!(
                "mir representation program {} symbols drift",
                actual_program.id
            ));
        }
        if actual_program.memory_surfaces != expected_program.memory_surfaces {
            return Err(format!(
                "mir representation program {} memory_surfaces drift",
                actual_program.id
            ));
        }
        if actual_program.control_flow != expected_program.control_flow {
            return Err(format!(
                "mir representation program {} control_flow drift",
                actual_program.id
            ));
        }
    }

    if receipt.summary != expected.summary {
        return Err("mir representation summary drift".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use buildlang::codegen::{
        AggregateKind, BlockId, LocalId, MirBlock, MirConst, MirFnSig, MirFunction, MirLocal,
        MirStmt, MirType, MirTypeDef, MirValue, MirVtable, TypeDefKind,
    };

    #[test]
    fn mir_representation_builds_receipt_for_semantic_corpus() {
        let corpus_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("compiler manifest has repository parent")
            .join("semantic-corpus");
        let manifest: SemanticCorpusManifest =
            serde_json::from_slice(&std::fs::read(corpus_root.join("manifest.json")).unwrap())
                .unwrap();

        let receipt = build_mir_representation_receipt(&corpus_root, &manifest)
            .expect("build MIR representation receipt");

        assert_eq!(receipt.schema, MIR_REPRESENTATION_SCHEMA);
        assert_eq!(receipt.source_set.program_count, manifest.programs.len());
        assert_eq!(receipt.programs.len(), manifest.programs.len());
        assert_eq!(receipt.summary.program_count, manifest.programs.len());
        assert_eq!(receipt.programs[0].id, manifest.programs[0].id);
        assert_eq!(receipt.programs[0].path, manifest.programs[0].path);
        assert_eq!(receipt.programs[0].source_digest.algorithm, "sha256");
        assert_eq!(receipt.programs[0].source_digest.hex.len(), 64);
        assert_eq!(receipt.programs[0].input_graph_digest.algorithm, "sha256");
        assert_eq!(receipt.programs[0].input_graph_digest.hex.len(), 64);
        assert_eq!(receipt.programs[0].mir_digest.algorithm, "sha256");
        assert_eq!(receipt.programs[0].mir_digest.hex.len(), 64);
    }

    #[test]
    fn mir_representation_text_digests_normalize_line_endings() {
        let lf = b"fn main() -> i32 {\n    return 1;\n}\n";
        let crlf = b"fn main() -> i32 {\r\n    return 1;\r\n}\r\n";

        assert_eq!(mir_source_digest(lf), mir_source_digest(crlf));

        let mut lf_ledger = InputDigestLedger::text_normalized();
        lf_ledger.record("entry", Path::new("programs/main.bld"), lf);
        let mut crlf_ledger = InputDigestLedger::text_normalized();
        crlf_ledger.record("entry", Path::new("programs/main.bld"), crlf);

        assert_eq!(
            input_graph_digest(&lf_ledger.into_sorted_records()).hex,
            input_graph_digest(&crlf_ledger.into_sorted_records()).hex
        );
    }

    #[test]
    fn validate_corpus_relative_path_rejects_root_qualified_paths() {
        let corpus_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("compiler manifest has repository parent")
            .join("semantic-corpus");
        let rooted = if cfg!(windows) {
            "\\manifest.json"
        } else {
            "/manifest.json"
        };

        let err = validate_corpus_relative_path(&corpus_root, rooted, "program.path")
            .expect_err("root-qualified path should be rejected");

        assert!(err.contains("must stay within corpus root"), "{err}");
    }

    #[test]
    fn mir_representation_summary_sorts_and_deduplicates_families() {
        let programs = vec![
            MirRepresentationProgram {
                id: "b".to_string(),
                path: "programs/b.bld".to_string(),
                source_digest: MirRepresentationDigest {
                    algorithm: "sha256".to_string(),
                    hex: "1".repeat(64),
                },
                input_graph_digest: MirRepresentationDigest {
                    algorithm: "sha256".to_string(),
                    hex: "3".repeat(64),
                },
                mir_digest: MirRepresentationDigest {
                    algorithm: "sha256".to_string(),
                    hex: "4".repeat(64),
                },
                module: MirRepresentationModuleCounts::default(),
                symbols: MirRepresentationSymbols::default(),
                operations: MirRepresentationOperations {
                    statements: vec!["FieldAssign".to_string(), "Assign".to_string()],
                    rvalues: vec!["Use".to_string(), "BinaryOp".to_string()],
                    terminators: vec!["Return".to_string()],
                    binary_ops: Vec::new(),
                    unary_ops: Vec::new(),
                    casts: Vec::new(),
                    aggregate_kinds: Vec::new(),
                    place_projections: Vec::new(),
                },
                memory_surfaces: MirRepresentationMemorySurfaces {
                    field_writes: true,
                    ..Default::default()
                },
                control_flow: MirRepresentationControlFlow::default(),
            },
            MirRepresentationProgram {
                id: "a".to_string(),
                path: "programs/a.bld".to_string(),
                source_digest: MirRepresentationDigest {
                    algorithm: "sha256".to_string(),
                    hex: "2".repeat(64),
                },
                input_graph_digest: MirRepresentationDigest {
                    algorithm: "sha256".to_string(),
                    hex: "5".repeat(64),
                },
                mir_digest: MirRepresentationDigest {
                    algorithm: "sha256".to_string(),
                    hex: "6".repeat(64),
                },
                module: MirRepresentationModuleCounts::default(),
                symbols: MirRepresentationSymbols::default(),
                operations: MirRepresentationOperations {
                    statements: vec!["Assign".to_string()],
                    rvalues: vec!["Use".to_string()],
                    terminators: vec!["If".to_string()],
                    binary_ops: Vec::new(),
                    unary_ops: Vec::new(),
                    casts: Vec::new(),
                    aggregate_kinds: Vec::new(),
                    place_projections: Vec::new(),
                },
                memory_surfaces: MirRepresentationMemorySurfaces {
                    references: true,
                    ..Default::default()
                },
                control_flow: MirRepresentationControlFlow::default(),
            },
        ];

        let summary = summarize_programs(&programs);

        assert_eq!(summary.program_count, 2);
        assert_eq!(summary.statement_families, vec!["Assign", "FieldAssign"]);
        assert_eq!(summary.rvalue_families, vec!["BinaryOp", "Use"]);
        assert_eq!(summary.terminator_families, vec!["If", "Return"]);
        assert_eq!(summary.memory_surfaces, vec!["field_writes", "references"]);
    }

    #[test]
    fn mir_representation_memory_surfaces_derive_from_mir() {
        let mut module = MirModule::new("memory_test");
        module.add_type(buildlang_type_def_point());

        let sig = MirFnSig::new(vec![], MirType::Void);
        let mut func = MirFunction::new("main", sig);
        func.add_local(MirLocal::new(
            LocalId(0),
            MirType::Struct(Arc::from("Point")),
        ));
        func.add_local(MirLocal::new(
            LocalId(1),
            MirType::Ptr(Box::new(MirType::Struct(Arc::from("Point")))),
        ));
        let mut block = MirBlock::new(BlockId::ENTRY);
        block.push_stmt(MirStmt::assign(
            LocalId(1),
            MirRValue::Ref {
                is_mut: true,
                place: MirPlace::local(LocalId(0)),
            },
        ));
        block.push_stmt(MirStmt::assign(
            LocalId(0),
            MirRValue::Aggregate {
                kind: AggregateKind::Struct(Arc::from("Point")),
                operands: vec![MirValue::Const(MirConst::Int(1, MirType::i32()))],
            },
        ));
        block.push_stmt(MirStmt::assign(
            LocalId(0),
            MirRValue::FieldAccess {
                base: MirValue::Local(LocalId(0)),
                field_name: Arc::from("x"),
                field_ty: MirType::i32(),
            },
        ));
        block.push_stmt(MirStmt::new(MirStmtKind::DerefAssign {
            ptr: LocalId(1),
            value: MirRValue::Use(MirValue::Const(MirConst::Int(2, MirType::i32()))),
        }));
        block.set_terminator(MirTerminator::Return(None));
        func.add_block(block);
        module.add_function(func);

        let program = summarize_mir_program(
            "memory_test",
            "programs/memory_test.bld",
            MirRepresentationDigest {
                algorithm: "sha256".to_string(),
                hex: "3".repeat(64),
            },
            MirRepresentationDigest {
                algorithm: "sha256".to_string(),
                hex: "4".repeat(64),
            },
            MirRepresentationDigest {
                algorithm: "sha256".to_string(),
                hex: "5".repeat(64),
            },
            &module,
        );

        assert!(program.memory_surfaces.references);
        assert!(program.memory_surfaces.mutable_references);
        assert!(program.memory_surfaces.deref_writes);
        assert!(program.memory_surfaces.field_reads);
        assert!(program.memory_surfaces.aggregate_values);
        assert_eq!(
            program.operations.rvalues,
            vec!["Aggregate", "FieldAccess", "Ref", "Use"]
        );
    }

    #[test]
    fn mir_digest_changes_when_payload_changes_without_family_drift() {
        let mut first = MirModule::new("payload_test");
        let mut first_function = MirFunction::new("main", MirFnSig::new(vec![], MirType::Void));
        let mut first_block = MirBlock::new(BlockId::ENTRY);
        first_block.push_stmt(MirStmt::assign(
            LocalId(0),
            MirRValue::Use(MirValue::Const(MirConst::Int(1, MirType::i32()))),
        ));
        first_block.set_terminator(MirTerminator::Return(None));
        first_function.add_block(first_block);
        first.add_function(first_function);

        let mut second = MirModule::new("payload_test");
        let mut second_function = MirFunction::new("main", MirFnSig::new(vec![], MirType::Void));
        let mut second_block = MirBlock::new(BlockId::ENTRY);
        second_block.push_stmt(MirStmt::assign(
            LocalId(0),
            MirRValue::Use(MirValue::Const(MirConst::Int(2, MirType::i32()))),
        ));
        second_block.set_terminator(MirTerminator::Return(None));
        second_function.add_block(second_block);
        second.add_function(second_function);

        let first_program = summarize_mir_program(
            "payload_test",
            "programs/payload_test.bld",
            MirRepresentationDigest {
                algorithm: "sha256".to_string(),
                hex: "7".repeat(64),
            },
            MirRepresentationDigest {
                algorithm: "sha256".to_string(),
                hex: "8".repeat(64),
            },
            digest_mir_module(&first),
            &first,
        );
        let second_program = summarize_mir_program(
            "payload_test",
            "programs/payload_test.bld",
            MirRepresentationDigest {
                algorithm: "sha256".to_string(),
                hex: "7".repeat(64),
            },
            MirRepresentationDigest {
                algorithm: "sha256".to_string(),
                hex: "8".repeat(64),
            },
            digest_mir_module(&second),
            &second,
        );

        assert_eq!(first_program.operations, second_program.operations);
        assert_ne!(first_program.mir_digest, second_program.mir_digest);
    }

    #[test]
    fn mir_digest_is_stable_for_equivalent_vtables_in_different_orders() {
        let render_sig = MirFnSig::new(vec![MirType::i32()], MirType::Void);
        let debug_sig = MirFnSig::new(vec![MirType::Bool], MirType::i32());
        let eq_sig = MirFnSig::new(vec![MirType::i32(), MirType::i32()], MirType::Bool);

        let mut first = MirModule::new("vtable_order");
        first.vtables = vec![
            MirVtable {
                trait_name: Arc::from("Display"),
                type_name: Arc::from("Point"),
                methods: vec![
                    (
                        Arc::from("render"),
                        Arc::from("Point_display_render"),
                        render_sig.clone(),
                    ),
                    (
                        Arc::from("debug"),
                        Arc::from("Point_display_debug"),
                        debug_sig.clone(),
                    ),
                ],
            },
            MirVtable {
                trait_name: Arc::from("Eq"),
                type_name: Arc::from("Point"),
                methods: vec![(Arc::from("eq"), Arc::from("Point_eq"), eq_sig.clone())],
            },
        ];

        let mut second = MirModule::new("vtable_order");
        second.vtables = vec![
            MirVtable {
                trait_name: Arc::from("Eq"),
                type_name: Arc::from("Point"),
                methods: vec![(Arc::from("eq"), Arc::from("Point_eq"), eq_sig)],
            },
            MirVtable {
                trait_name: Arc::from("Display"),
                type_name: Arc::from("Point"),
                methods: vec![
                    (
                        Arc::from("debug"),
                        Arc::from("Point_display_debug"),
                        debug_sig,
                    ),
                    (
                        Arc::from("render"),
                        Arc::from("Point_display_render"),
                        render_sig,
                    ),
                ],
            },
        ];

        assert_eq!(digest_mir_module(&first), digest_mir_module(&second));
    }

    // =========================================================================
    // MIR INTERLINGUA ROUND-TRIP TESTS (buildlang.mir/v0)
    // =========================================================================

    #[test]
    fn mir_envelope_schema_string_is_versioned() {
        use buildlang::codegen::MIR_SCHEMA;
        assert_eq!(MIR_SCHEMA, "buildlang.mir/v0");
    }

    #[test]
    fn mir_envelope_round_trips_for_every_corpus_program() {
        use buildlang::codegen::MirModuleEnvelope;

        let corpus_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("compiler manifest has repository parent")
            .join("semantic-corpus");
        let manifest: SemanticCorpusManifest =
            serde_json::from_slice(&std::fs::read(corpus_root.join("manifest.json")).unwrap())
                .unwrap();

        assert!(
            !manifest.programs.is_empty(),
            "semantic corpus must contain programs"
        );

        for program in &manifest.programs {
            let program_path =
                validate_corpus_relative_path(&corpus_root, &program.path, "program.path")
                    .expect("corpus program path resolves");
            let lowered = lower_program_to_mir(&program_path)
                .unwrap_or_else(|err| panic!("lower {} to MIR: {err}", program.id));

            // 1. Wrap in the versioned envelope and serialize to JSON.
            let envelope = MirModuleEnvelope::wrap(&lowered.module);
            assert_eq!(envelope.schema, "buildlang.mir/v0");
            let json = serde_json::to_string(&envelope)
                .unwrap_or_else(|err| panic!("serialize {} MIR to JSON: {err}", program.id));

            // 2. Deserialize back to an owned envelope/module.
            let restored: MirModuleEnvelope = serde_json::from_str(&json)
                .unwrap_or_else(|err| panic!("deserialize {} MIR from JSON: {err}", program.id));
            assert_eq!(
                restored.schema, "buildlang.mir/v0",
                "schema survives round-trip for {}",
                program.id
            );

            // 3. Structural equality: the deserialized module equals the original.
            assert_eq!(
                restored.module, lowered.module,
                "structural round-trip mismatch for {}",
                program.id
            );

            // 4. Digest equality: receipt digest of the round-tripped module is
            //    byte-identical to the original (proves the interlingua is faithful
            //    and the serde additions did not perturb the receipt surface).
            assert_eq!(
                digest_mir_module(&restored.module),
                digest_mir_module(&lowered.module),
                "MIR digest round-trip mismatch for {}",
                program.id
            );

            // 5. Re-serializing the restored module yields identical JSON (idempotent).
            let rejson = serde_json::to_string(&MirModuleEnvelope::wrap(&restored.module))
                .expect("re-serialize restored MIR");
            assert_eq!(
                json, rejson,
                "JSON serialization is not idempotent for {}",
                program.id
            );
        }
    }

    #[test]
    fn mir_envelope_round_trips_lossless_floats_including_non_finite() {
        use buildlang::codegen::MirModuleEnvelope;

        // Floats (incl. NaN, +/-inf, -0.0, subnormal) must survive bit-exact.
        let floats = [
            0.0_f64,
            -0.0_f64,
            1.0_f64,
            -1.5_f64,
            f64::MIN_POSITIVE,
            f64::MAX,
            f64::MIN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
            std::f64::consts::PI,
            5e-324_f64, // smallest positive subnormal
        ];

        let mut module = MirModule::new("float_roundtrip");
        let mut func = MirFunction::new("main", MirFnSig::new(vec![], MirType::Void));
        let mut block = MirBlock::new(BlockId::ENTRY);
        for (i, f) in floats.iter().enumerate() {
            block.push_stmt(MirStmt::assign(
                LocalId(i as u32),
                MirRValue::Use(MirValue::Const(MirConst::Float(*f, MirType::f64()))),
            ));
        }
        block.set_terminator(MirTerminator::Return(None));
        func.add_block(block);
        module.add_function(func);

        let json = serde_json::to_string(&MirModuleEnvelope::wrap(&module)).expect("serialize");
        let restored: MirModuleEnvelope = serde_json::from_str(&json).expect("deserialize");

        let restored_func = &restored.module.functions[0];
        let restored_block = &restored_func.blocks.as_ref().unwrap()[0];
        for (i, original) in floats.iter().enumerate() {
            match &restored_block.stmts[i].kind {
                MirStmtKind::Assign {
                    value: MirRValue::Use(MirValue::Const(MirConst::Float(v, _))),
                    ..
                } => {
                    assert_eq!(
                        v.to_bits(),
                        original.to_bits(),
                        "float {original} did not round-trip bit-exact"
                    );
                }
                other => panic!("unexpected stmt kind: {other:?}"),
            }
        }

        // The whole module must be structurally equal under PartialEq too
        // (note: this also exercises NaN handling in the derived PartialEq path
        // via bit comparison, which we route through digest equality instead).
        assert_eq!(
            digest_mir_module(&restored.module),
            digest_mir_module(&module)
        );
    }

    fn buildlang_type_def_point() -> MirTypeDef {
        MirTypeDef {
            name: Arc::from("Point"),
            kind: TypeDefKind::Struct {
                fields: vec![(Some(Arc::from("x")), MirType::i32())],
                packed: false,
            },
        }
    }
}
