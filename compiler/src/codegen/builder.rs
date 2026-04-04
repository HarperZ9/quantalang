// ===============================================================================
// QUANTALANG CODE GENERATOR - MIR BUILDER
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! MIR builder for constructing MIR code programmatically.

use std::sync::Arc;

use super::ir::*;

/// Builder for constructing MIR functions.
pub struct MirBuilder {
    /// The function being built.
    func: MirFunction,
    /// Current block ID.
    current_block: BlockId,
    /// Next local ID.
    next_local: u32,
}

impl MirBuilder {
    /// Create a new MIR builder.
    pub fn new(name: impl Into<Arc<str>>, sig: MirFnSig) -> Self {
        let mut func = MirFunction::new(name, sig.clone());

        // Create entry block
        let entry = MirBlock::new(BlockId::ENTRY);
        func.add_block(entry);

        // Add parameters as locals
        let mut next_local = 0u32;
        for (i, param_ty) in sig.params.iter().enumerate() {
            let local_id = LocalId(next_local);
            let mut local = MirLocal::new(local_id, param_ty.clone());
            local.is_param = true;
            local.name = Some(Arc::from(format!("arg{}", i)));
            func.locals.push(local);
            next_local += 1;
        }

        // Add return local if not void
        if sig.ret != MirType::Void {
            let local_id = LocalId(next_local);
            let mut local = MirLocal::new(local_id, sig.ret.clone());
            local.name = Some(Arc::from("_ret"));
            func.locals.push(local);
            next_local += 1;
        }

        Self {
            func,
            current_block: BlockId::ENTRY,
            next_local,
        }
    }

    /// Finish building and return the function.
    pub fn build(self) -> MirFunction {
        self.func
    }

    /// Get the current block ID.
    pub fn current_block(&self) -> BlockId {
        self.current_block
    }

    /// Create a new block and return its ID.
    pub fn create_block(&mut self) -> BlockId {
        let id = BlockId(self.func.blocks.as_ref().map(|b| b.len()).unwrap_or(0) as u32);
        let block = MirBlock::new(id);
        self.func.add_block(block);
        id
    }

    /// Create a new labeled block.
    pub fn create_labeled_block(&mut self, label: impl Into<Arc<str>>) -> BlockId {
        let id = BlockId(self.func.blocks.as_ref().map(|b| b.len()).unwrap_or(0) as u32);
        let block = MirBlock::with_label(id, label);
        self.func.add_block(block);
        id
    }

    /// Switch to a different block.
    pub fn switch_to_block(&mut self, block: BlockId) {
        self.current_block = block;
    }

    /// Create a new local variable.
    pub fn create_local(&mut self, ty: MirType) -> LocalId {
        let id = LocalId(self.next_local);
        self.next_local += 1;
        let local = MirLocal::new(id, ty);
        self.func.add_local(local);
        id
    }

    /// Create a new named local variable.
    pub fn create_named_local(&mut self, name: impl Into<Arc<str>>, ty: MirType) -> LocalId {
        let id = LocalId(self.next_local);
        self.next_local += 1;
        let local = MirLocal::named(id, name, ty);
        self.func.add_local(local);
        id
    }

    /// Get the return local (if not void).
    pub fn return_local(&self) -> Option<LocalId> {
        if self.func.sig.ret != MirType::Void {
            // Return local is after params
            Some(LocalId(self.func.sig.params.len() as u32))
        } else {
            None
        }
    }

    /// Get a parameter local.
    pub fn param_local(&self, index: usize) -> LocalId {
        LocalId(index as u32)
    }

    /// Look up the type of a local variable by its ID.
    pub fn local_type(&self, id: LocalId) -> Option<MirType> {
        self.func
            .locals
            .iter()
            .find(|l| l.id == id)
            .map(|l| l.ty.clone())
    }

    /// Change the type of an existing local.  Used when a let binding has
    /// an explicit type annotation but the init expression produced a
    /// fallback type (i32).
    pub fn retype_local(&mut self, id: LocalId, new_ty: MirType) {
        if let Some(local) = self.func.locals.iter_mut().find(|l| l.id == id) {
            local.ty = new_ty;
        }
    }

    /// Check if a local ID exists in this function.
    pub fn local_exists(&self, id: LocalId) -> bool {
        self.func.locals.iter().any(|l| l.id == id)
    }

    /// Get the function's return type.
    pub fn return_type(&self) -> &MirType {
        &self.func.sig.ret
    }

    /// Rename a parameter local.
    pub fn set_param_name(&mut self, index: usize, name: impl Into<Arc<str>>) {
        if let Some(local) = self.func.locals.get_mut(index) {
            local.name = Some(name.into());
        }
    }

    /// Set type annotations on a parameter local (e.g., ColorSpace, Precision).
    pub fn set_param_annotations(&mut self, index: usize, annotations: Vec<Arc<str>>) {
        if let Some(local) = self.func.locals.get_mut(index) {
            local.annotations = annotations;
        }
    }

    // =========================================================================
    // STATEMENTS
    // =========================================================================

    /// Add a statement to the current block.
    fn push_stmt(&mut self, kind: MirStmtKind) {
        if let Some(block) = self.func.block_mut(self.current_block) {
            block.push_stmt(MirStmt::new(kind));
        }
    }

    /// Assign a value to a local.
    pub fn assign(&mut self, dest: LocalId, value: MirRValue) {
        self.push_stmt(MirStmtKind::Assign { dest, value });
    }

    /// Assign a constant to a local.
    pub fn assign_const(&mut self, dest: LocalId, value: MirConst) {
        self.assign(dest, MirRValue::Use(MirValue::Const(value)));
    }

    /// Copy a local to another.
    pub fn copy_local(&mut self, dest: LocalId, src: LocalId) {
        self.assign(dest, MirRValue::Use(MirValue::Local(src)));
    }

    /// Mark storage as live.
    pub fn storage_live(&mut self, local: LocalId) {
        self.push_stmt(MirStmtKind::StorageLive(local));
    }

    /// Mark storage as dead.
    pub fn storage_dead(&mut self, local: LocalId) {
        self.push_stmt(MirStmtKind::StorageDead(local));
    }

    /// Add a no-op.
    pub fn nop(&mut self) {
        self.push_stmt(MirStmtKind::Nop);
    }

    // =========================================================================
    // OPERATIONS
    // =========================================================================

    /// Binary operation.
    pub fn binary_op(&mut self, dest: LocalId, op: BinOp, left: MirValue, right: MirValue) {
        self.assign(dest, MirRValue::BinaryOp { op, left, right });
    }

    /// Unary operation.
    pub fn unary_op(&mut self, dest: LocalId, op: UnaryOp, operand: MirValue) {
        self.assign(dest, MirRValue::UnaryOp { op, operand });
    }

    /// Cast operation.
    pub fn cast(&mut self, dest: LocalId, kind: CastKind, value: MirValue, ty: MirType) {
        self.assign(dest, MirRValue::Cast { kind, value, ty });
    }

    /// Create a reference.
    pub fn make_ref(&mut self, dest: LocalId, is_mut: bool, place: MirPlace) {
        self.assign(dest, MirRValue::Ref { is_mut, place });
    }

    /// Store through a pointer (dereference assignment): `*ptr = value`
    pub fn push_deref_assign(&mut self, ptr: LocalId, value: MirRValue) {
        self.push_stmt(MirStmtKind::DerefAssign { ptr, value });
    }

    /// Store to a field through a pointer: `ptr->field = value`
    pub fn push_field_deref_assign(
        &mut self,
        ptr: LocalId,
        field_name: Arc<str>,
        value: MirRValue,
    ) {
        self.push_stmt(MirStmtKind::FieldDerefAssign {
            ptr,
            field_name,
            value,
        });
    }

    /// Store to a field on a local struct: `local.field = value`
    pub fn push_field_assign(&mut self, base: LocalId, field_name: Arc<str>, value: MirRValue) {
        self.push_stmt(MirStmtKind::FieldAssign {
            base,
            field_name,
            value,
        });
    }

    /// Create an aggregate (tuple, struct, array).
    pub fn aggregate(&mut self, dest: LocalId, kind: AggregateKind, operands: Vec<MirValue>) {
        self.assign(dest, MirRValue::Aggregate { kind, operands });
    }

    // =========================================================================
    // TERMINATORS
    // =========================================================================

    /// Set the terminator for the current block.
    fn set_terminator(&mut self, term: MirTerminator) {
        if let Some(block) = self.func.block_mut(self.current_block) {
            block.set_terminator(term);
        }
    }

    /// Unconditional goto.
    pub fn goto(&mut self, target: BlockId) {
        self.set_terminator(MirTerminator::Goto(target));
    }

    /// Conditional branch.
    pub fn branch(&mut self, cond: MirValue, then_block: BlockId, else_block: BlockId) {
        self.set_terminator(MirTerminator::If {
            cond,
            then_block,
            else_block,
        });
    }

    /// Switch statement.
    pub fn switch(&mut self, value: MirValue, targets: Vec<(MirConst, BlockId)>, default: BlockId) {
        self.set_terminator(MirTerminator::Switch {
            value,
            targets,
            default,
        });
    }

    /// Function call.
    pub fn call(
        &mut self,
        func: MirValue,
        args: Vec<MirValue>,
        dest: Option<LocalId>,
        target: BlockId,
    ) {
        self.set_terminator(MirTerminator::Call {
            func,
            args,
            dest,
            target: Some(target),
            unwind: None,
        });
    }

    /// Void function call (no return value).
    pub fn call_void(&mut self, func: MirValue, args: Vec<MirValue>, target: BlockId) {
        self.call(func, args, None, target);
    }

    /// Return from function.
    pub fn ret(&mut self, value: Option<MirValue>) {
        self.set_terminator(MirTerminator::Return(value));
    }

    /// Return void.
    pub fn ret_void(&mut self) {
        self.ret(None);
    }

    /// Unreachable.
    pub fn unreachable(&mut self) {
        self.set_terminator(MirTerminator::Unreachable);
    }

    /// Assert.
    pub fn assert(
        &mut self,
        cond: MirValue,
        expected: bool,
        msg: impl Into<Arc<str>>,
        target: BlockId,
    ) {
        self.set_terminator(MirTerminator::Assert {
            cond,
            expected,
            msg: msg.into(),
            target,
            unwind: None,
        });
    }

    /// Abort.
    pub fn abort(&mut self) {
        self.set_terminator(MirTerminator::Abort);
    }
}

/// Builder for constructing MIR modules.
pub struct MirModuleBuilder {
    /// The module being built.
    module: MirModule,
}

impl MirModuleBuilder {
    /// Create a new module builder.
    pub fn new(name: impl Into<Arc<str>>) -> Self {
        Self {
            module: MirModule::new(name),
        }
    }

    /// Finish building and return the module.
    pub fn build(self) -> MirModule {
        self.module
    }

    /// Find a global variable by name.
    pub fn find_global(&self, name: &str) -> Option<&MirGlobal> {
        self.module.find_global(name)
    }

    /// Get mutable access to the underlying MirModule.
    pub fn module_mut(&mut self) -> &mut MirModule {
        &mut self.module
    }

    /// Add a function.
    pub fn add_function(&mut self, func: MirFunction) {
        self.module.add_function(func);
    }

    /// Add a global variable.
    pub fn add_global(&mut self, global: MirGlobal) {
        self.module.add_global(global);
    }

    /// Add a type definition.
    pub fn add_type(&mut self, ty: MirTypeDef) {
        self.module.add_type(ty);
    }

    /// Add an external declaration.
    pub fn add_external(&mut self, ext: MirExternal) {
        self.module.externals.push(ext);
    }

    /// Intern a string.
    pub fn intern_string(&mut self, s: impl Into<Arc<str>>) -> u32 {
        self.module.intern_string(s)
    }

    /// Declare an external function.
    pub fn declare_function(&mut self, name: impl Into<Arc<str>>, sig: MirFnSig) {
        let func = MirFunction::declaration(name, sig);
        self.module.add_function(func);
    }

    /// Create a struct type.
    pub fn create_struct(
        &mut self,
        name: impl Into<Arc<str>>,
        fields: Vec<(Option<Arc<str>>, MirType)>,
    ) {
        let ty = MirTypeDef {
            name: name.into(),
            kind: TypeDefKind::Struct {
                fields,
                packed: false,
            },
        };
        self.module.add_type(ty);
    }

    /// Create an enum type.
    pub fn create_enum(
        &mut self,
        name: impl Into<Arc<str>>,
        discriminant_ty: MirType,
        variants: Vec<MirEnumVariant>,
    ) {
        let ty = MirTypeDef {
            name: name.into(),
            kind: TypeDefKind::Enum {
                discriminant_ty,
                variants,
            },
        };
        self.module.add_type(ty);
    }

    /// Find a function by name in the module built so far.
    pub fn find_function(&self, name: &str) -> Option<&MirFunction> {
        self.module.find_function(name)
    }

    /// Find a type definition by name.
    pub fn find_type(&self, name: &str) -> Option<&MirTypeDef> {
        self.module.types.iter().find(|t| t.name.as_ref() == name)
    }

    /// Find a type whose name ends with `_suffix`.
    /// Used to resolve cross-module references: `Operator` → `tonemap_Operator`.
    pub fn find_type_by_suffix(&self, suffix: &str) -> Option<String> {
        let pattern = format!("_{}", suffix);
        self.module
            .types
            .iter()
            .find(|t| t.name.ends_with(&pattern))
            .map(|t| t.name.to_string())
    }
}

/// Helper to create common MIR values.
pub mod values {
    use super::*;

    /// Create a local value.
    pub fn local(id: LocalId) -> MirValue {
        MirValue::Local(id)
    }

    /// Create an i32 constant.
    pub fn i32(v: i32) -> MirValue {
        MirValue::Const(MirConst::Int(v as i128, MirType::i32()))
    }

    /// Create an i64 constant.
    pub fn i64(v: i64) -> MirValue {
        MirValue::Const(MirConst::Int(v as i128, MirType::i64()))
    }

    /// Create a u32 constant.
    pub fn u32(v: u32) -> MirValue {
        MirValue::Const(MirConst::Uint(v as u128, MirType::u32()))
    }

    /// Create a u64 constant.
    pub fn u64(v: u64) -> MirValue {
        MirValue::Const(MirConst::Uint(v as u128, MirType::u64()))
    }

    /// Create a bool constant.
    pub fn bool(v: bool) -> MirValue {
        MirValue::Const(MirConst::Bool(v))
    }

    /// Create a float constant.
    pub fn f32(v: f32) -> MirValue {
        MirValue::Const(MirConst::Float(v as f64, MirType::f32()))
    }

    /// Create a float constant.
    pub fn f64(v: f64) -> MirValue {
        MirValue::Const(MirConst::Float(v, MirType::f64()))
    }

    /// Create a unit constant.
    pub fn unit() -> MirValue {
        MirValue::Const(MirConst::Unit)
    }

    /// Create a null pointer.
    pub fn null(ty: MirType) -> MirValue {
        MirValue::Const(MirConst::Null(ty))
    }

    /// Create a function reference.
    pub fn func(name: impl Into<Arc<str>>) -> MirValue {
        MirValue::Function(name.into())
    }

    /// Create a global reference.
    pub fn global(name: impl Into<Arc<str>>) -> MirValue {
        MirValue::Global(name.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // MIR BUILDER TESTS
    // =========================================================================

    #[test]
    fn test_builder_simple_function() {
        // Build: fn add(a: i32, b: i32) -> i32 { a + b }
        let sig = MirFnSig::new(vec![MirType::i32(), MirType::i32()], MirType::i32());
        let mut builder = MirBuilder::new("add", sig);

        let a = builder.param_local(0);
        let b = builder.param_local(1);
        let result = builder.create_local(MirType::i32());

        builder.binary_op(result, BinOp::Add, values::local(a), values::local(b));
        builder.ret(Some(values::local(result)));

        let func = builder.build();
        assert_eq!(func.name.as_ref(), "add");
        assert!(!func.is_declaration());
    }

    #[test]
    fn test_builder_with_branch() {
        // Build: fn max(a: i32, b: i32) -> i32 { if a > b { a } else { b } }
        let sig = MirFnSig::new(vec![MirType::i32(), MirType::i32()], MirType::i32());
        let mut builder = MirBuilder::new("max", sig);

        let a = builder.param_local(0);
        let b = builder.param_local(1);
        let cond = builder.create_local(MirType::Bool);

        let then_block = builder.create_block();
        let else_block = builder.create_block();

        // Entry block: compare a > b
        builder.binary_op(cond, BinOp::Gt, values::local(a), values::local(b));
        builder.branch(values::local(cond), then_block, else_block);

        // Then block: return a
        builder.switch_to_block(then_block);
        builder.ret(Some(values::local(a)));

        // Else block: return b
        builder.switch_to_block(else_block);
        builder.ret(Some(values::local(b)));

        let func = builder.build();
        assert_eq!(func.blocks.as_ref().unwrap().len(), 3);
    }

    #[test]
    fn test_builder_void_function() {
        let sig = MirFnSig::new(vec![], MirType::Void);
        let mut builder = MirBuilder::new("noop", sig);
        builder.ret_void();

        let func = builder.build();
        assert_eq!(func.name.as_ref(), "noop");
        assert!(func.sig.params.is_empty());
        assert_eq!(func.sig.ret, MirType::Void);
    }

    #[test]
    fn test_builder_create_local() {
        let sig = MirFnSig::new(vec![], MirType::Void);
        let mut builder = MirBuilder::new("test", sig);

        let local1 = builder.create_local(MirType::i32());
        let local2 = builder.create_local(MirType::f64());
        let local3 = builder.create_named_local("named", MirType::Bool);

        assert_ne!(local1, local2);
        assert_ne!(local2, local3);

        builder.ret_void();
        let func = builder.build();

        // Should have the locals we created
        assert!(func.locals.len() >= 3);
    }

    #[test]
    fn test_builder_unary_op() {
        let sig = MirFnSig::new(vec![MirType::i32()], MirType::i32());
        let mut builder = MirBuilder::new("negate", sig);

        let input = builder.param_local(0);
        let result = builder.create_local(MirType::i32());

        builder.unary_op(result, UnaryOp::Neg, values::local(input));
        builder.ret(Some(values::local(result)));

        let func = builder.build();
        let blocks = func.blocks.as_ref().unwrap();
        assert!(!blocks[0].stmts.is_empty());
    }

    #[test]
    fn test_builder_assign_const() {
        let sig = MirFnSig::new(vec![], MirType::i32());
        let mut builder = MirBuilder::new("const_42", sig);

        let result = builder.create_local(MirType::i32());
        builder.assign_const(result, MirConst::Int(42, MirType::i32()));
        builder.ret(Some(values::local(result)));

        let func = builder.build();
        let blocks = func.blocks.as_ref().unwrap();
        assert_eq!(blocks[0].stmts.len(), 1);
    }

    #[test]
    fn test_builder_goto() {
        let sig = MirFnSig::new(vec![], MirType::Void);
        let mut builder = MirBuilder::new("test_goto", sig);

        let target = builder.create_block();
        builder.goto(target);

        builder.switch_to_block(target);
        builder.ret_void();

        let func = builder.build();
        let blocks = func.blocks.as_ref().unwrap();
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn test_builder_switch() {
        let sig = MirFnSig::new(vec![MirType::i32()], MirType::i32());
        let mut builder = MirBuilder::new("test_switch", sig);

        let input = builder.param_local(0);
        let case1 = builder.create_block();
        let case2 = builder.create_block();
        let default = builder.create_block();

        builder.switch(
            values::local(input),
            vec![
                (MirConst::Int(1, MirType::i32()), case1),
                (MirConst::Int(2, MirType::i32()), case2),
            ],
            default,
        );

        builder.switch_to_block(case1);
        builder.ret(Some(values::i32(100)));

        builder.switch_to_block(case2);
        builder.ret(Some(values::i32(200)));

        builder.switch_to_block(default);
        builder.ret(Some(values::i32(0)));

        let func = builder.build();
        assert_eq!(func.blocks.as_ref().unwrap().len(), 4);
    }

    #[test]
    fn test_builder_call() {
        let sig = MirFnSig::new(vec![], MirType::i32());
        let mut builder = MirBuilder::new("caller", sig);

        let result = builder.create_local(MirType::i32());
        let cont = builder.create_block();

        builder.call(
            values::func("other_func"),
            vec![values::i32(10)],
            Some(result),
            cont,
        );

        builder.switch_to_block(cont);
        builder.ret(Some(values::local(result)));

        let func = builder.build();
        assert_eq!(func.blocks.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_builder_unreachable() {
        let sig = MirFnSig::new(vec![], MirType::Never);
        let mut builder = MirBuilder::new("panic_fn", sig);
        builder.unreachable();

        let func = builder.build();
        let blocks = func.blocks.as_ref().unwrap();
        assert!(matches!(
            blocks[0].terminator,
            Some(MirTerminator::Unreachable)
        ));
    }

    #[test]
    fn test_builder_abort() {
        let sig = MirFnSig::new(vec![], MirType::Never);
        let mut builder = MirBuilder::new("abort_fn", sig);
        builder.abort();

        let func = builder.build();
        let blocks = func.blocks.as_ref().unwrap();
        assert!(matches!(blocks[0].terminator, Some(MirTerminator::Abort)));
    }

    #[test]
    fn test_builder_labeled_block() {
        let sig = MirFnSig::new(vec![], MirType::Void);
        let mut builder = MirBuilder::new("test", sig);

        let loop_block = builder.create_labeled_block("loop_start");
        builder.goto(loop_block);

        builder.switch_to_block(loop_block);
        builder.ret_void();

        let func = builder.build();
        let blocks = func.blocks.as_ref().unwrap();
        assert_eq!(blocks[1].label.as_ref().unwrap().as_ref(), "loop_start");
    }

    // =========================================================================
    // MODULE BUILDER TESTS
    // =========================================================================

    #[test]
    fn test_module_builder_new() {
        let builder = MirModuleBuilder::new("test_module");
        let module = builder.build();
        assert_eq!(module.name.as_ref(), "test_module");
    }

    #[test]
    fn test_module_builder_add_function() {
        let mut builder = MirModuleBuilder::new("test");

        let sig = MirFnSig::new(vec![], MirType::Void);
        let func = MirFunction::new("my_func", sig);
        builder.add_function(func);

        let module = builder.build();
        assert_eq!(module.functions.len(), 1);
    }

    #[test]
    fn test_module_builder_add_global() {
        let mut builder = MirModuleBuilder::new("test");

        let global = MirGlobal::new("CONSTANT", MirType::i32());
        builder.add_global(global);

        let module = builder.build();
        assert_eq!(module.globals.len(), 1);
    }

    #[test]
    fn test_module_builder_intern_string() {
        let mut builder = MirModuleBuilder::new("test");

        let idx1 = builder.intern_string("hello");
        let idx2 = builder.intern_string("world");
        let idx3 = builder.intern_string("hello"); // Duplicate

        assert_eq!(idx1, 0);
        assert_eq!(idx2, 1);
        assert_eq!(idx3, 0); // Same as first
    }

    #[test]
    fn test_module_builder_declare_function() {
        let mut builder = MirModuleBuilder::new("test");

        let sig = MirFnSig::new(vec![MirType::i32()], MirType::i32());
        builder.declare_function("external_fn", sig);

        let module = builder.build();
        assert!(module.functions[0].is_declaration());
    }

    #[test]
    fn test_module_builder_create_struct() {
        let mut builder = MirModuleBuilder::new("test");

        builder.create_struct(
            "Point",
            vec![
                (Some(Arc::from("x")), MirType::i32()),
                (Some(Arc::from("y")), MirType::i32()),
            ],
        );

        let module = builder.build();
        assert_eq!(module.types.len(), 1);
        match &module.types[0].kind {
            TypeDefKind::Struct { fields, .. } => assert_eq!(fields.len(), 2),
            _ => panic!("Expected struct"),
        }
    }

    #[test]
    fn test_module_builder_create_enum() {
        let mut builder = MirModuleBuilder::new("test");

        builder.create_enum(
            "Color",
            MirType::i32(),
            vec![
                MirEnumVariant {
                    name: Arc::from("Red"),
                    discriminant: 0,
                    fields: vec![],
                },
                MirEnumVariant {
                    name: Arc::from("Green"),
                    discriminant: 1,
                    fields: vec![],
                },
            ],
        );

        let module = builder.build();
        assert_eq!(module.types.len(), 1);
    }

    // =========================================================================
    // VALUES HELPER TESTS
    // =========================================================================

    #[test]
    fn test_values_local() {
        let val = values::local(LocalId(5));
        match val {
            MirValue::Local(id) => assert_eq!(id.0, 5),
            _ => panic!("Expected Local"),
        }
    }

    #[test]
    fn test_values_i32() {
        let val = values::i32(42);
        match val {
            MirValue::Const(MirConst::Int(v, ty)) => {
                assert_eq!(v, 42);
                assert_eq!(ty, MirType::i32());
            }
            _ => panic!("Expected i32 const"),
        }
    }

    #[test]
    fn test_values_i64() {
        let val = values::i64(1234567890123i64);
        match val {
            MirValue::Const(MirConst::Int(v, ty)) => {
                assert_eq!(v, 1234567890123);
                assert_eq!(ty, MirType::i64());
            }
            _ => panic!("Expected i64 const"),
        }
    }

    #[test]
    fn test_values_bool() {
        let val_true = values::bool(true);
        let val_false = values::bool(false);

        match val_true {
            MirValue::Const(MirConst::Bool(b)) => assert!(b),
            _ => panic!("Expected bool true"),
        }
        match val_false {
            MirValue::Const(MirConst::Bool(b)) => assert!(!b),
            _ => panic!("Expected bool false"),
        }
    }

    #[test]
    fn test_values_f32() {
        let val = values::f32(3.14f32);
        match val {
            MirValue::Const(MirConst::Float(_, ty)) => {
                assert_eq!(ty, MirType::f32());
            }
            _ => panic!("Expected f32 const"),
        }
    }

    #[test]
    fn test_values_f64() {
        let val = values::f64(3.14159265358979);
        match val {
            MirValue::Const(MirConst::Float(v, ty)) => {
                assert!((v - 3.14159265358979).abs() < 1e-10);
                assert_eq!(ty, MirType::f64());
            }
            _ => panic!("Expected f64 const"),
        }
    }

    #[test]
    fn test_values_unit() {
        let val = values::unit();
        match val {
            MirValue::Const(MirConst::Unit) => {}
            _ => panic!("Expected unit"),
        }
    }

    #[test]
    fn test_values_null() {
        let val = values::null(MirType::Ptr(Box::new(MirType::i32())));
        match val {
            MirValue::Const(MirConst::Null(_)) => {}
            _ => panic!("Expected null"),
        }
    }

    #[test]
    fn test_values_func() {
        let val = values::func("my_function");
        match val {
            MirValue::Function(name) => assert_eq!(name.as_ref(), "my_function"),
            _ => panic!("Expected function ref"),
        }
    }

    #[test]
    fn test_values_global() {
        let val = values::global("GLOBAL_VAR");
        match val {
            MirValue::Global(name) => assert_eq!(name.as_ref(), "GLOBAL_VAR"),
            _ => panic!("Expected global ref"),
        }
    }
}
