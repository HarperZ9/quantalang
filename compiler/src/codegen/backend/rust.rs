// ===============================================================================
// BUILDLANG CODE GENERATOR - RUST BACKEND
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. BuildLang Fair-Source License v1.0 (see LICENSE).
// ===============================================================================

//! Rust source backend.
//!
//! This backend is a conservative bridge from BuildLang MIR to Rust source. It
//! is intentionally subset-based: MIR constructs that are not safely projected
//! yet return `CodegenError::Unsupported` instead of emitting plausible but
//! incorrect Rust.

use std::collections::HashMap;
use std::sync::Arc;

use super::{Backend, CodegenError, CodegenResult, Target};
use crate::codegen::ir::*;
use crate::codegen::{GeneratedCode, OutputFormat};

/// Backend that emits Rust source from MIR.
pub struct RustBackend {
    output: String,
    indent: usize,
    strings: Vec<Arc<str>>,
    struct_fields: HashMap<String, Vec<String>>,
}

impl RustBackend {
    /// Create a new Rust backend.
    pub fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
            strings: Vec::new(),
            struct_fields: HashMap::new(),
        }
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent {
            self.output.push_str("    ");
        }
    }

    fn writeln(&mut self, line: &str) {
        self.write_indent();
        self.output.push_str(line);
        self.output.push('\n');
    }

    fn collect_struct_fields(&mut self, types: &[MirTypeDef]) {
        self.struct_fields.clear();
        for ty in types {
            if let TypeDefKind::Struct { fields, .. } = &ty.kind {
                let names = fields
                    .iter()
                    .enumerate()
                    .map(|(i, (name, _))| {
                        name.as_ref()
                            .map(|n| Self::rust_ident(n))
                            .unwrap_or_else(|| format!("field{}", i))
                    })
                    .collect();
                self.struct_fields.insert(ty.name.to_string(), names);
            }
        }
    }

    fn emit_runtime(&mut self) {
        self.writeln("fn build_string_new<S: AsRef<str>>(s: S) -> String {");
        self.indent += 1;
        self.writeln("s.as_ref().to_string()");
        self.indent -= 1;
        self.writeln("}");
        self.writeln("");
        self.writeln("fn build_format(fmt: &str, args: &[String]) -> String {");
        self.indent += 1;
        self.writeln("let mut out = String::new();");
        self.writeln("let mut args_iter = args.iter();");
        self.writeln("let mut chars = fmt.chars().peekable();");
        self.writeln("while let Some(ch) = chars.next() {");
        self.indent += 1;
        self.writeln("if ch == '{' {");
        self.indent += 1;
        self.writeln("match chars.peek().copied() {");
        self.indent += 1;
        self.writeln(
            "Some('}') => { chars.next(); out.push_str(args_iter.next().map(String::as_str).unwrap_or(\"\")); }",
        );
        self.writeln("Some('{') => { chars.next(); out.push('{'); }");
        self.writeln("_ => out.push(ch),");
        self.indent -= 1;
        self.writeln("}");
        self.writeln("continue;");
        self.indent -= 1;
        self.writeln("}");
        self.writeln("if ch == '}' {");
        self.indent += 1;
        self.writeln("if chars.peek() == Some(&'}') { chars.next(); }");
        self.writeln("out.push('}');");
        self.writeln("continue;");
        self.indent -= 1;
        self.writeln("}");
        self.writeln("if ch != '%' {");
        self.indent += 1;
        self.writeln("out.push(ch);");
        self.writeln("continue;");
        self.indent -= 1;
        self.writeln("}");
        self.writeln("match chars.next() {");
        self.indent += 1;
        self.writeln("Some('%') => out.push('%'),");
        self.writeln(
            "Some('d' | 'i' | 'u' | 's' | 'f' | 'g') => out.push_str(args_iter.next().map(String::as_str).unwrap_or(\"\")),",
        );
        self.writeln("Some('l') => {");
        self.indent += 1;
        self.writeln("if chars.peek() == Some(&'l') { chars.next(); }");
        self.writeln("match chars.next() {");
        self.indent += 1;
        self.writeln(
            "Some('d' | 'i' | 'u') => out.push_str(args_iter.next().map(String::as_str).unwrap_or(\"\")),",
        );
        self.writeln("Some(other) => { out.push('%'); out.push('l'); out.push(other); }");
        self.writeln("None => { out.push('%'); out.push('l'); }");
        self.indent -= 1;
        self.writeln("}");
        self.indent -= 1;
        self.writeln("}");
        self.writeln("Some(other) => { out.push('%'); out.push(other); }");
        self.writeln("None => out.push('%'),");
        self.indent -= 1;
        self.writeln("}");
        self.indent -= 1;
        self.writeln("}");
        self.writeln("out");
        self.indent -= 1;
        self.writeln("}");
        self.writeln("");
        self.writeln("fn build_printf<S: AsRef<str>>(fmt: S, args: &[String]) -> i32 {");
        self.indent += 1;
        self.writeln("print!(\"{}\", build_format(fmt.as_ref(), args));");
        self.writeln("0");
        self.indent -= 1;
        self.writeln("}");
        self.writeln("");
        self.writeln("fn build_println<S: AsRef<str>>(fmt: S, args: &[String]) -> i32 {");
        self.indent += 1;
        self.writeln("println!(\"{}\", build_format(fmt.as_ref(), args));");
        self.writeln("0");
        self.indent -= 1;
        self.writeln("}");
        self.writeln("");
        // Numeric-to-string intrinsics. The C runtime returns a heap
        // `BuildString`; here each returns a native `String` whose `Display`
        // form matches the C output: the integer paths print decimal, and the
        // float paths print Rust's shortest round-trip, which is the same
        // positional decimal the C `bl_fmt_f64`/`bl_fmt_f32` helpers produce.
        self.writeln("fn build_i32_to_string(v: i32) -> String { format!(\"{}\", v) }");
        self.writeln("fn build_i64_to_string(v: i64) -> String { format!(\"{}\", v) }");
        self.writeln("fn build_f32_to_string(v: f32) -> String { format!(\"{}\", v) }");
        self.writeln("fn build_f64_to_string(v: f64) -> String { format!(\"{}\", v) }");
        self.writeln("");
        // The MIR extracts a `char*` from a runtime string before formatting
        // it, following the C string model. This backend has no stable raw
        // pointer to hand back, so it materializes a `'static` slice by leaking
        // a copy of the bytes. The generated program runs once and exits, so
        // the bounded, deliberate leak stays local to the projection.
        self.writeln("fn build_str_ptr(s: &str) -> &'static str {");
        self.indent += 1;
        self.writeln("Box::leak(s.to_string().into_boxed_str())");
        self.indent -= 1;
        self.writeln("}");
        self.writeln("");
    }

    fn emit_type_definitions(&mut self, types: &[MirTypeDef]) -> CodegenResult<()> {
        for ty in types {
            match &ty.kind {
                TypeDefKind::Struct { fields, .. } => {
                    self.writeln("#[derive(Clone, Debug, Default)]");
                    self.writeln(&format!("struct {} {{", Self::rust_type_name(&ty.name)));
                    self.indent += 1;
                    for (i, (name, field_ty)) in fields.iter().enumerate() {
                        let field_name = name
                            .as_ref()
                            .map(|n| Self::rust_ident(n))
                            .unwrap_or_else(|| format!("field{}", i));
                        self.writeln(&format!("{}: {},", field_name, self.type_to_rust(field_ty)));
                    }
                    self.indent -= 1;
                    self.writeln("}");
                    self.writeln("");
                }
                TypeDefKind::Union { .. } => {
                    return Err(CodegenError::Unsupported(format!(
                        "Rust backend does not yet lower union type '{}'",
                        ty.name
                    )));
                }
                TypeDefKind::Enum { .. } => {
                    return Err(CodegenError::Unsupported(format!(
                        "Rust backend does not yet lower enum type '{}'",
                        ty.name
                    )));
                }
            }
        }
        Ok(())
    }

    fn emit_string_table(&mut self) {
        if self.strings.is_empty() {
            return;
        }
        for (i, s) in self.strings.clone().iter().enumerate() {
            self.writeln(&format!("const __STR{}: &str = {:?};", i, s.as_ref()));
        }
        self.writeln("");
    }

    fn generate_function(&mut self, func: &MirFunction) -> CodegenResult<()> {
        if func.is_declaration() {
            return Ok(());
        }

        let is_main = func.name.as_ref() == "main";
        let fn_name = Self::rust_ident(&func.name);
        let params = if is_main {
            Vec::new()
        } else {
            func.locals
                .iter()
                .filter(|local| local.is_param)
                .map(|local| {
                    format!(
                        "{}: {}",
                        self.local_name(local.id, &func.locals),
                        self.type_to_rust(&local.ty)
                    )
                })
                .collect::<Vec<_>>()
        };

        let ret = if is_main || matches!(func.sig.ret, MirType::Void | MirType::Never) {
            String::new()
        } else {
            format!(" -> {}", self.type_to_rust(&func.sig.ret))
        };

        self.writeln(&format!("fn {}({}){} {{", fn_name, params.join(", "), ret));
        self.indent += 1;

        for local in &func.locals {
            if local.is_param || matches!(local.ty, MirType::Void) {
                continue;
            }
            self.writeln(&format!(
                "let mut {}: {} = {};",
                self.local_name(local.id, &func.locals),
                self.type_to_rust(&local.ty),
                self.default_value(&local.ty)
            ));
        }

        if let Some(blocks) = &func.blocks {
            self.writeln("let mut __bb: u32 = 0;");
            self.writeln("loop {");
            self.indent += 1;
            self.writeln("match __bb {");
            self.indent += 1;
            for block in blocks {
                self.writeln(&format!("{} => {{", block.id.0));
                self.indent += 1;
                for stmt in &block.stmts {
                    self.generate_statement(stmt, &func.locals)?;
                }
                if let Some(term) = &block.terminator {
                    self.generate_terminator(term, &func.locals, is_main)?;
                } else {
                    self.generate_fallthrough_return(&func.sig.ret, is_main);
                }
                self.indent -= 1;
                self.writeln("}");
            }
            self.writeln("_ => unreachable!(),");
            self.indent -= 1;
            self.writeln("}");
            self.indent -= 1;
            self.writeln("}");
        } else {
            self.generate_fallthrough_return(&func.sig.ret, is_main);
        }

        self.indent -= 1;
        self.writeln("}");
        self.writeln("");
        Ok(())
    }

    fn generate_fallthrough_return(&mut self, ret_ty: &MirType, is_main: bool) {
        if is_main || matches!(ret_ty, MirType::Void | MirType::Never) {
            self.writeln("return;");
        } else {
            self.writeln(&format!("return {};", self.default_value(ret_ty)));
        }
    }

    fn generate_statement(&mut self, stmt: &MirStmt, locals: &[MirLocal]) -> CodegenResult<()> {
        match &stmt.kind {
            MirStmtKind::Assign { dest, value } => {
                if locals
                    .get(dest.0 as usize)
                    .map(|local| matches!(local.ty, MirType::Void))
                    .unwrap_or(false)
                {
                    return Ok(());
                }
                let dest_name = self.local_name(*dest, locals);
                let rvalue = self.rvalue_to_rust(value, locals)?;
                self.writeln(&format!("{} = {};", dest_name, rvalue));
            }
            MirStmtKind::DerefAssign { ptr, value } => {
                let ptr_name = self.local_name(*ptr, locals);
                let rvalue = self.rvalue_to_rust(value, locals)?;
                self.writeln(&format!("unsafe {{ *{} = {}; }}", ptr_name, rvalue));
            }
            MirStmtKind::FieldDerefAssign {
                ptr,
                field_name,
                value,
            } => {
                let ptr_name = self.local_name(*ptr, locals);
                let rvalue = self.rvalue_to_rust(value, locals)?;
                self.writeln(&format!(
                    "unsafe {{ (*{}).{} = {}; }}",
                    ptr_name,
                    Self::rust_ident(field_name),
                    rvalue
                ));
            }
            MirStmtKind::FieldAssign {
                base,
                field_name,
                value,
            } => {
                let base_name = self.local_name(*base, locals);
                let rvalue = self.rvalue_to_rust(value, locals)?;
                self.writeln(&format!(
                    "{}.{} = {};",
                    base_name,
                    Self::rust_ident(field_name),
                    rvalue
                ));
            }
            MirStmtKind::GlobalStore { name, value } => {
                let rvalue = self.rvalue_to_rust(value, locals)?;
                self.writeln(&format!("{} = {};", Self::rust_ident(name), rvalue));
            }
            MirStmtKind::IndexStore {
                base, index, value, ..
            } => {
                let base_str = self.value_to_rust(base, locals);
                let index_str = self.value_to_rust(index, locals);
                let rvalue = self.rvalue_to_rust(value, locals)?;
                self.writeln(&format!(
                    "{}[({}) as usize] = {};",
                    base_str, index_str, rvalue
                ));
            }
            // A workgroup barrier is a no-op on the single-threaded Rust source
            // backend (there is no workgroup to synchronize).
            MirStmtKind::StorageLive(_)
            | MirStmtKind::StorageDead(_)
            | MirStmtKind::Nop
            | MirStmtKind::WorkgroupBarrier => {}
        }
        Ok(())
    }

    fn generate_terminator(
        &mut self,
        term: &MirTerminator,
        locals: &[MirLocal],
        is_main: bool,
    ) -> CodegenResult<()> {
        match term {
            MirTerminator::Goto(target) => self.emit_jump(*target),
            MirTerminator::If {
                cond,
                then_block,
                else_block,
            } => {
                let cond_str = self.value_to_rust(cond, locals);
                self.writeln(&format!(
                    "if {} {{ __bb = {}; }} else {{ __bb = {}; }}",
                    cond_str, then_block.0, else_block.0
                ));
                self.writeln("continue;");
            }
            MirTerminator::Switch {
                value,
                targets,
                default,
            } => {
                let val = self.value_to_rust(value, locals);
                self.writeln(&format!("match {} {{", val));
                self.indent += 1;
                for (case, target) in targets {
                    self.writeln(&format!(
                        "{} => __bb = {},",
                        self.const_to_rust(case),
                        target.0
                    ));
                }
                self.writeln(&format!("_ => __bb = {},", default.0));
                self.indent -= 1;
                self.writeln("}");
                self.writeln("continue;");
            }
            MirTerminator::Call {
                func,
                args,
                dest,
                target,
                ..
            } => {
                self.emit_call(func, args, *dest, locals)?;
                if let Some(target) = target {
                    self.emit_jump(*target);
                }
            }
            MirTerminator::Return(value) => self.emit_return(value.as_ref(), locals, is_main),
            MirTerminator::Unreachable => self.writeln("unreachable!();"),
            MirTerminator::Drop { target, .. } => self.emit_jump(*target),
            MirTerminator::Assert {
                cond,
                expected,
                target,
                msg,
                ..
            } => {
                let mut cond_str = self.value_to_rust(cond, locals);
                if !expected {
                    cond_str = format!("!({})", cond_str);
                }
                if msg.is_empty() {
                    self.writeln(&format!("assert!({});", cond_str));
                } else {
                    self.writeln(&format!("assert!({}, {:?});", cond_str, msg.as_ref()));
                }
                self.emit_jump(*target);
            }
            MirTerminator::Resume => self.writeln("panic!(\"resume unwinding\");"),
            MirTerminator::Abort => self.writeln("std::process::abort();"),
        }
        Ok(())
    }

    fn emit_jump(&mut self, target: BlockId) {
        self.writeln(&format!("__bb = {};", target.0));
        self.writeln("continue;");
    }

    fn emit_return(&mut self, value: Option<&MirValue>, locals: &[MirLocal], is_main: bool) {
        if is_main {
            if let Some(value) = value {
                let code = self.value_to_rust(value, locals);
                self.writeln(&format!("let __code = {};", code));
                self.writeln("if __code != 0 { std::process::exit(__code as i32); }");
            }
            self.writeln("return;");
        } else if let Some(value) = value {
            self.writeln(&format!("return {};", self.value_to_rust(value, locals)));
        } else {
            self.writeln("return;");
        }
    }

    fn emit_call(
        &mut self,
        func: &MirValue,
        args: &[MirValue],
        dest: Option<LocalId>,
        locals: &[MirLocal],
    ) -> CodegenResult<()> {
        let func_name = self.value_to_rust(func, locals);
        if func_name == "printf" || func_name == "println" {
            if args.is_empty() {
                return Ok(());
            }
            let fmt = self.value_to_rust(&args[0], locals);
            let fmt = if self.value_is_string_like(&args[0], locals) {
                format!("&{}", fmt)
            } else {
                fmt
            };
            let arg_strings = args
                .iter()
                .skip(1)
                .map(|arg| format!("format!(\"{{}}\", {})", self.value_to_rust(arg, locals)))
                .collect::<Vec<_>>();
            let runtime_call = if func_name == "println" {
                "build_println"
            } else {
                "build_printf"
            };
            let call = format!("{}({}, &[{}])", runtime_call, fmt, arg_strings.join(", "));
            if let Some(dest) = dest {
                self.writeln(&format!("{} = {};", self.local_name(dest, locals), call));
            } else {
                self.writeln(&format!("{};", call));
            }
            return Ok(());
        }

        if func_name == "fflush" {
            return Ok(());
        }

        let args_str = args
            .iter()
            .map(|arg| self.value_to_owned_rust(arg, locals))
            .collect::<Vec<_>>()
            .join(", ");
        let call = format!("{}({})", func_name, args_str);
        if let Some(dest) = dest {
            self.writeln(&format!("{} = {};", self.local_name(dest, locals), call));
        } else {
            self.writeln(&format!("{};", call));
        }
        Ok(())
    }

    fn rvalue_to_rust(&self, rvalue: &MirRValue, locals: &[MirLocal]) -> CodegenResult<String> {
        Ok(match rvalue {
            MirRValue::Use(value) => self.value_to_owned_rust(value, locals),
            MirRValue::BinaryOp { op, left, right } => {
                let l = self.value_to_rust(left, locals);
                let r = self.value_to_rust(right, locals);
                if *op == BinOp::Pow {
                    format!("({} as f64).powf({} as f64)", l, r)
                } else {
                    format!("({} {} {})", l, Self::binop_to_rust(*op), r)
                }
            }
            MirRValue::UnaryOp { op, operand } => {
                let v = self.value_to_rust(operand, locals);
                let op_str = match op {
                    UnaryOp::Not => "!",
                    UnaryOp::BitNot => "!",
                    UnaryOp::Neg => "-",
                };
                format!("({}{})", op_str, v)
            }
            MirRValue::Ref { is_mut, place } | MirRValue::AddressOf { is_mut, place } => {
                let place = self.place_to_rust(place, locals)?;
                if *is_mut {
                    format!("(&mut {} as *mut _)", place)
                } else {
                    format!("(&{} as *const _ as *mut _)", place)
                }
            }
            MirRValue::Cast { value, ty, .. } => {
                format!(
                    "({} as {})",
                    self.value_to_rust(value, locals),
                    self.type_to_rust(ty)
                )
            }
            MirRValue::Aggregate { kind, operands } => match kind {
                AggregateKind::Array(_) => {
                    let vals = operands
                        .iter()
                        .map(|op| self.value_to_owned_rust(op, locals))
                        .collect::<Vec<_>>();
                    format!("[{}]", vals.join(", "))
                }
                AggregateKind::Tuple => {
                    let vals = operands
                        .iter()
                        .map(|op| self.value_to_owned_rust(op, locals))
                        .collect::<Vec<_>>();
                    match vals.len() {
                        0 => "()".to_string(),
                        _ => {
                            let tuple_name = Self::rust_type_name(&MirType::tuple_type_name(
                                &operands
                                    .iter()
                                    .map(|op| self.type_of_value(op, locals))
                                    .collect::<Vec<_>>(),
                            ));
                            let fields = vals
                                .iter()
                                .enumerate()
                                .map(|(i, val)| format!("_{}: {}", i, val))
                                .collect::<Vec<_>>();
                            format!("{} {{ {} }}", tuple_name, fields.join(", "))
                        }
                    }
                }
                AggregateKind::Struct(name) => {
                    let vals = operands
                        .iter()
                        .map(|op| self.value_to_owned_rust(op, locals))
                        .collect::<Vec<_>>();
                    let type_name = Self::rust_type_name(name);
                    let fields = self.struct_fields.get(name.as_ref());
                    if let Some(fields) = fields {
                        let pairs = vals
                            .iter()
                            .enumerate()
                            .map(|(i, val)| {
                                let field = fields
                                    .get(i)
                                    .cloned()
                                    .unwrap_or_else(|| format!("field{}", i));
                                format!("{}: {}", field, val)
                            })
                            .collect::<Vec<_>>();
                        format!("{} {{ {} }}", type_name, pairs.join(", "))
                    } else if vals.is_empty() {
                        format!("{} {{}}", type_name)
                    } else {
                        return Err(CodegenError::Unsupported(format!(
                                "Rust backend cannot lower struct aggregate '{}' without field metadata",
                                name
                            )));
                    }
                }
                AggregateKind::Variant(_, _, _) | AggregateKind::Closure(_) => {
                    return Err(CodegenError::Unsupported(
                        "Rust backend does not yet lower enum variants or closures".to_string(),
                    ));
                }
            },
            MirRValue::Repeat { value, count } => {
                let value_str = self.value_to_rust(value, locals);
                if self.value_is_copy_like(value, locals) {
                    format!("[{}; {}]", value_str, count)
                } else {
                    format!("std::array::from_fn(|_| ({}).clone())", value_str)
                }
            }
            MirRValue::Discriminant(_)
            | MirRValue::VariantField { .. }
            | MirRValue::TextureSample { .. } => {
                return Err(CodegenError::Unsupported(
                    "Rust backend does not yet lower enum discriminants, variant fields, or texture samples"
                        .to_string(),
                ));
            }
            MirRValue::Len(place) => format!("{}.len()", self.place_to_rust(place, locals)?),
            MirRValue::NullaryOp(op, ty) => match op {
                NullaryOp::SizeOf => format!("std::mem::size_of::<{}>()", self.type_to_rust(ty)),
                NullaryOp::AlignOf => format!("std::mem::align_of::<{}>()", self.type_to_rust(ty)),
                // GPU-only built-ins; the Rust source backend is not a compute
                // target, so they lower to 0.
                NullaryOp::ThreadIndex(_)
                | NullaryOp::LocalInvocationId(_)
                | NullaryOp::WorkgroupId(_) => "0".to_string(),
            },
            MirRValue::FieldAccess {
                base,
                field_name,
                field_ty,
            } => {
                let base_str = self.value_to_rust(base, locals);
                // The MIR models a runtime string as a C `BuildString` carrying
                // `ptr`, `len`, and `cap` fields. This backend maps that string
                // to a native Rust `String`, which has no such fields, so a
                // projection off a string base is redirected to the matching
                // `String` operation. `ptr` becomes a `&'static str` over the
                // bytes (the only consumer formats it), and `len`/`cap` become
                // the byte length and capacity cast to the projected width.
                if self.value_is_string_like(base, locals) {
                    let field: &str = field_name;
                    match field {
                        "ptr" => format!("build_str_ptr(&{})", base_str),
                        "len" => {
                            format!("(({}).len() as {})", base_str, self.type_to_rust(field_ty))
                        }
                        "cap" => format!(
                            "(({}).capacity() as {})",
                            base_str,
                            self.type_to_rust(field_ty)
                        ),
                        // No other field exists on the runtime string; fall back
                        // to a by-name access so an unexpected projection fails
                        // at rustc rather than miscompiling silently.
                        _ => format!("{}.{}", base_str, Self::rust_ident(field_name)),
                    }
                } else {
                    let field = Self::rust_ident(field_name);
                    let access = if self.value_is_raw_pointer(base, locals) {
                        format!("unsafe {{ (*{}).{} }}", base_str, field)
                    } else {
                        format!("{}.{}", base_str, field)
                    };
                    if Self::is_copy_like_type(field_ty) {
                        access
                    } else {
                        format!("({}).clone()", access)
                    }
                }
            }
            MirRValue::IndexAccess {
                base,
                index,
                elem_ty,
            } => {
                let access = format!(
                    "{}[{} as usize]",
                    self.value_to_rust(base, locals),
                    self.value_to_rust(index, locals)
                );
                if Self::is_copy_like_type(elem_ty) {
                    access
                } else {
                    format!("({}).clone()", access)
                }
            }
            MirRValue::Deref { ptr, pointee_ty } => {
                let ptr = self.value_to_rust(ptr, locals);
                if Self::is_copy_like_type(pointee_ty) {
                    format!("unsafe {{ *{} }}", ptr)
                } else {
                    format!("unsafe {{ (*{}).clone() }}", ptr)
                }
            }
        })
    }

    fn place_to_rust(&self, place: &MirPlace, locals: &[MirLocal]) -> CodegenResult<String> {
        let mut out = self.local_name(place.local, locals);
        for projection in &place.projections {
            match projection {
                PlaceProjection::Deref => out = format!("unsafe {{ *{} }}", out),
                PlaceProjection::Field(_, name, _) => {
                    // Generated Rust structs declare named fields (`x`, or the
                    // `_0`/`_1` fields of a lowered tuple), so a field place must
                    // access by name, not by ordinal. Positional `.field{idx}`
                    // does not compile against a named-field struct. `rust_ident`
                    // matches the escaping used when the struct is defined.
                    out = format!("{}.{}", out, Self::rust_ident(name));
                }
                PlaceProjection::Index(id) => {
                    out = format!("{}[{} as usize]", out, self.local_name(*id, locals));
                }
                PlaceProjection::ConstantIndex { offset, .. } => {
                    out = format!("{}[{}]", out, offset);
                }
                PlaceProjection::Subslice { .. } | PlaceProjection::Downcast(_) => {
                    return Err(CodegenError::Unsupported(
                        "Rust backend does not yet lower subslice or downcast places".to_string(),
                    ));
                }
            }
        }
        Ok(out)
    }

    fn value_to_rust(&self, value: &MirValue, locals: &[MirLocal]) -> String {
        match value {
            MirValue::Local(id) => self.local_name(*id, locals),
            MirValue::Const(c) => self.const_to_rust(c),
            MirValue::Global(name) | MirValue::Function(name) => Self::rust_ident(name),
        }
    }

    fn const_to_rust(&self, c: &MirConst) -> String {
        match c {
            MirConst::Bool(b) => b.to_string(),
            MirConst::Int(v, _) => v.to_string(),
            MirConst::Uint(v, _) => v.to_string(),
            MirConst::Float(v, ty) => {
                let mut s = v.to_string();
                if !s.contains('.') && !s.contains('e') && !s.contains('E') {
                    s.push_str(".0");
                }
                if matches!(ty, MirType::Float(FloatSize::F32)) {
                    s.push_str("f32");
                }
                s
            }
            MirConst::Str(idx) => format!("__STR{}", idx),
            MirConst::ByteStr(bytes) => format!("{:?}", bytes),
            MirConst::Null(_) => "std::ptr::null_mut()".to_string(),
            MirConst::Unit => "()".to_string(),
            MirConst::Zeroed(ty) => self.default_value(ty),
            MirConst::Undef(ty) => self.default_value(ty),
            MirConst::Struct(name, fields) => {
                let type_name = Self::rust_type_name(name);
                let field_names = self.struct_fields.get(name.as_ref());
                if let Some(field_names) = field_names {
                    let fields = fields
                        .iter()
                        .enumerate()
                        .map(|(i, value)| {
                            let field = field_names
                                .get(i)
                                .cloned()
                                .unwrap_or_else(|| format!("field{}", i));
                            format!("{}: {}", field, self.const_to_rust(value))
                        })
                        .collect::<Vec<_>>();
                    format!("{} {{ {} }}", type_name, fields.join(", "))
                } else {
                    format!("{}::default()", type_name)
                }
            }
        }
    }

    fn type_to_rust(&self, ty: &MirType) -> String {
        match ty {
            MirType::Void | MirType::Never => "()".to_string(),
            MirType::Bool => "bool".to_string(),
            MirType::Int(size, signed) => match (size, signed) {
                (IntSize::I8, true) => "i8".to_string(),
                (IntSize::I8, false) => "u8".to_string(),
                (IntSize::I16, true) => "i16".to_string(),
                (IntSize::I16, false) => "u16".to_string(),
                (IntSize::I32, true) => "i32".to_string(),
                (IntSize::I32, false) => "u32".to_string(),
                (IntSize::I64, true) => "i64".to_string(),
                (IntSize::I64, false) => "u64".to_string(),
                (IntSize::I128, true) => "i128".to_string(),
                (IntSize::I128, false) => "u128".to_string(),
                (IntSize::ISize, true) => "isize".to_string(),
                (IntSize::ISize, false) => "usize".to_string(),
            },
            MirType::Float(FloatSize::F32) => "f32".to_string(),
            MirType::Float(FloatSize::F64) => "f64".to_string(),
            MirType::Ptr(inner) if Self::is_i8_type(inner) => "&'static str".to_string(),
            MirType::Ptr(inner) => format!("*mut {}", self.type_to_rust(inner)),
            MirType::Array(elem, len) => format!("[{}; {}]", self.type_to_rust(elem), len),
            MirType::Slice(elem) => format!("&[{}]", self.type_to_rust(elem)),
            MirType::Struct(name) if name.as_ref() == "BuildString" => "String".to_string(),
            MirType::Struct(name) if name.as_ref() == "String" => "String".to_string(),
            MirType::Struct(name) => Self::rust_type_name(name),
            MirType::FnPtr(sig) => {
                let params = sig
                    .params
                    .iter()
                    .map(|p| self.type_to_rust(p))
                    .collect::<Vec<_>>();
                format!(
                    "fn({}) -> {}",
                    params.join(", "),
                    self.type_to_rust(&sig.ret)
                )
            }
            MirType::Vector(elem, lanes) => format!("[{}; {}]", self.type_to_rust(elem), lanes),
            MirType::Texture2D(_)
            | MirType::Sampler
            | MirType::SampledImage(_)
            | MirType::TraitObject(_) => "*mut std::ffi::c_void".to_string(),
            MirType::Vec(elem) => format!("Vec<{}>", self.type_to_rust(elem)),
            MirType::Map(key, value) => format!(
                "std::collections::BTreeMap<{}, {}>",
                self.type_to_rust(key),
                self.type_to_rust(value)
            ),
            MirType::Tuple(elems) => {
                if elems.is_empty() {
                    "()".to_string()
                } else {
                    Self::rust_type_name(&MirType::tuple_type_name(elems))
                }
            }
        }
    }

    fn default_value(&self, ty: &MirType) -> String {
        match ty {
            MirType::Void | MirType::Never => "()".to_string(),
            MirType::Bool => "false".to_string(),
            MirType::Int(_, _) => "0".to_string(),
            MirType::Float(FloatSize::F32) => "0.0f32".to_string(),
            MirType::Float(FloatSize::F64) => "0.0".to_string(),
            MirType::Ptr(inner) if Self::is_i8_type(inner) => "\"\"".to_string(),
            MirType::Ptr(_) => "std::ptr::null_mut()".to_string(),
            MirType::Array(elem, len) => {
                let elem_default = self.default_value(elem);
                if Self::is_copy_like_type(elem) {
                    format!("[{}; {}]", elem_default, len)
                } else {
                    format!("std::array::from_fn(|_| {})", elem_default)
                }
            }
            MirType::Slice(_) => "&[]".to_string(),
            MirType::Struct(name) if name.as_ref() == "String" => "String::new()".to_string(),
            MirType::Struct(name) if name.as_ref() == "BuildString" => "String::new()".to_string(),
            MirType::Vec(_) => "Vec::new()".to_string(),
            MirType::Map(_, _) => "std::collections::BTreeMap::new()".to_string(),
            _ => "Default::default()".to_string(),
        }
    }

    fn local_by_id(id: LocalId, locals: &[MirLocal]) -> Option<&MirLocal> {
        locals.iter().find(|local| local.id == id)
    }

    fn local_name(&self, id: LocalId, locals: &[MirLocal]) -> String {
        Self::local_by_id(id, locals)
            .and_then(|local| local.name.as_ref())
            .map(|name| {
                let base = Self::rust_ident(name);
                let has_dup = locals.iter().any(|other| {
                    other.id != id && other.name.as_ref().map(|s| s.as_ref()) == Some(name.as_ref())
                });
                if has_dup {
                    format!("{}_{}", base, id.0)
                } else {
                    base
                }
            })
            .unwrap_or_else(|| format!("_{}", id.0))
    }

    fn value_is_raw_pointer(&self, value: &MirValue, locals: &[MirLocal]) -> bool {
        match value {
            MirValue::Local(id) => Self::local_by_id(*id, locals)
                .map(|local| matches!(local.ty, MirType::Ptr(_)))
                .unwrap_or(false),
            _ => false,
        }
    }

    fn value_is_string_like(&self, value: &MirValue, locals: &[MirLocal]) -> bool {
        match value {
            MirValue::Local(id) => Self::local_by_id(*id, locals)
                .map(|local| Self::is_string_like_type(&local.ty))
                .unwrap_or(false),
            _ => false,
        }
    }

    fn value_to_owned_rust(&self, value: &MirValue, locals: &[MirLocal]) -> String {
        let value_str = self.value_to_rust(value, locals);
        if self.value_is_copy_like(value, locals) {
            value_str
        } else {
            format!("({}).clone()", value_str)
        }
    }

    fn type_of_value(&self, value: &MirValue, locals: &[MirLocal]) -> MirType {
        match value {
            MirValue::Local(id) => Self::local_by_id(*id, locals)
                .map(|local| local.ty.clone())
                .unwrap_or(MirType::Void),
            MirValue::Const(c) => Self::type_of_const(c),
            MirValue::Global(_) | MirValue::Function(_) => MirType::Void,
        }
    }

    fn value_is_copy_like(&self, value: &MirValue, locals: &[MirLocal]) -> bool {
        match value {
            MirValue::Local(id) => Self::local_by_id(*id, locals)
                .map(|local| Self::is_copy_like_type(&local.ty))
                .unwrap_or(true),
            MirValue::Const(c) => Self::const_is_copy_like(c),
            MirValue::Global(_) | MirValue::Function(_) => true,
        }
    }

    fn is_string_like_type(ty: &MirType) -> bool {
        matches!(
            ty,
            MirType::Struct(name) if name.as_ref() == "String" || name.as_ref() == "BuildString"
        )
    }

    fn const_is_copy_like(c: &MirConst) -> bool {
        match c {
            MirConst::Bool(_)
            | MirConst::Int(_, _)
            | MirConst::Uint(_, _)
            | MirConst::Float(_, _)
            | MirConst::Str(_)
            | MirConst::ByteStr(_)
            | MirConst::Null(_)
            | MirConst::Unit => true,
            MirConst::Zeroed(ty) | MirConst::Undef(ty) => Self::is_copy_like_type(ty),
            MirConst::Struct(_, _) => false,
        }
    }

    fn type_of_const(c: &MirConst) -> MirType {
        match c {
            MirConst::Bool(_) => MirType::Bool,
            MirConst::Int(_, ty)
            | MirConst::Uint(_, ty)
            | MirConst::Float(_, ty)
            | MirConst::Null(ty)
            | MirConst::Zeroed(ty)
            | MirConst::Undef(ty) => ty.clone(),
            MirConst::Str(_) | MirConst::ByteStr(_) => {
                MirType::Ptr(Box::new(MirType::Int(IntSize::I8, true)))
            }
            MirConst::Unit => MirType::Void,
            MirConst::Struct(name, _) => MirType::Struct(name.clone()),
        }
    }

    fn is_copy_like_type(ty: &MirType) -> bool {
        match ty {
            MirType::Void
            | MirType::Never
            | MirType::Bool
            | MirType::Int(_, _)
            | MirType::Float(_)
            | MirType::Ptr(_)
            | MirType::FnPtr(_)
            | MirType::Texture2D(_)
            | MirType::Sampler
            | MirType::SampledImage(_)
            | MirType::TraitObject(_) => true,
            MirType::Array(elem, _) | MirType::Vector(elem, _) => Self::is_copy_like_type(elem),
            MirType::Tuple(_) => false,
            MirType::Struct(name)
                if name.as_ref() == "String" || name.as_ref() == "BuildString" =>
            {
                false
            }
            MirType::Struct(_) | MirType::Slice(_) | MirType::Vec(_) | MirType::Map(_, _) => false,
        }
    }

    fn binop_to_rust(op: BinOp) -> &'static str {
        match op {
            BinOp::Add | BinOp::AddChecked | BinOp::AddWrapping | BinOp::AddSaturating => "+",
            BinOp::Sub | BinOp::SubChecked | BinOp::SubWrapping | BinOp::SubSaturating => "-",
            BinOp::Mul | BinOp::MulChecked | BinOp::MulWrapping => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::Pow => unreachable!("handled before operator conversion"),
        }
    }

    fn is_i8_type(ty: &MirType) -> bool {
        matches!(ty, MirType::Int(IntSize::I8, _))
    }

    fn rust_type_name(name: &str) -> String {
        Self::rust_ident(name)
    }

    fn rust_ident(name: &str) -> String {
        let mut out = String::with_capacity(name.len());
        for (i, ch) in name.chars().enumerate() {
            if (i == 0 && (ch.is_ascii_alphabetic() || ch == '_'))
                || (i > 0 && (ch.is_ascii_alphanumeric() || ch == '_'))
            {
                out.push(ch);
            } else {
                out.push('_');
            }
        }
        if out.is_empty() || out.chars().next().unwrap().is_ascii_digit() {
            out.insert(0, '_');
        }
        if Self::is_rust_reserved(&out) {
            out.insert(0, '_');
        }
        out
    }

    fn is_rust_reserved(name: &str) -> bool {
        matches!(
            name,
            "as" | "break"
                | "const"
                | "continue"
                | "crate"
                | "else"
                | "enum"
                | "extern"
                | "false"
                | "fn"
                | "for"
                | "if"
                | "impl"
                | "in"
                | "let"
                | "loop"
                | "match"
                | "mod"
                | "move"
                | "mut"
                | "pub"
                | "ref"
                | "return"
                | "self"
                | "Self"
                | "static"
                | "struct"
                | "super"
                | "trait"
                | "true"
                | "type"
                | "unsafe"
                | "use"
                | "where"
                | "while"
                | "async"
                | "await"
                | "dyn"
                | "try"
                | "union"
                | "yield"
        )
    }
}

impl Default for RustBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for RustBackend {
    fn generate(&mut self, mir: &MirModule) -> CodegenResult<GeneratedCode> {
        self.output.clear();
        self.indent = 0;
        self.strings = mir.strings.clone();
        self.collect_struct_fields(&mir.types);

        self.writeln("// Generated by BuildLang Compiler");
        self.writeln("// Rust target is experimental and subset-based.");
        self.writeln("#![allow(dead_code, non_snake_case, non_camel_case_types, unused_assignments, unused_mut, unused_parens, unused_variables, unreachable_code)]");
        self.writeln("");

        self.emit_runtime();
        self.emit_type_definitions(&mir.types)?;
        self.emit_string_table();

        for func in &mir.functions {
            if !func.is_declaration() {
                self.generate_function(func)?;
            }
        }

        Ok(GeneratedCode::new(
            OutputFormat::RustSource,
            self.output.clone().into_bytes(),
        ))
    }

    fn target(&self) -> Target {
        Target::Rust
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::CodeGenerator;
    use crate::lexer::{Lexer, SourceFile};
    use crate::parser::Parser;
    use crate::types::{TypeChecker, TypeContext};

    const CORPUS_SCALAR_BRANCH: &str =
        include_str!("../../../../semantic-corpus/programs/scalar_branch.bld");
    const CORPUS_REFERENCES_MUTATION: &str =
        include_str!("../../../../semantic-corpus/programs/references_mutation.bld");
    const CORPUS_STRUCTS_ARRAYS: &str =
        include_str!("../../../../semantic-corpus/programs/structs_arrays.bld");
    const CORPUS_TUPLE_OWNERSHIP_REUSE: &str =
        include_str!("../../../../semantic-corpus/programs/tuple_ownership_reuse.bld");
    const CORPUS_STRUCT_AGGREGATE_REUSE: &str =
        include_str!("../../../../semantic-corpus/programs/struct_aggregate_reuse.bld");
    const CORPUS_FIELD_ASSIGNMENT_REUSE: &str =
        include_str!("../../../../semantic-corpus/programs/field_assignment_reuse.bld");
    const CORPUS_NESTED_FIELD_REUSE: &str =
        include_str!("../../../../semantic-corpus/programs/nested_field_reuse.bld");
    const CORPUS_DEREF_REUSE: &str =
        include_str!("../../../../semantic-corpus/programs/deref_reuse.bld");

    #[derive(serde::Deserialize)]
    struct SemanticCorpusManifest {
        schema: String,
        rust_manifest_execution_test: String,
        programs: Vec<SemanticCorpusProgram>,
    }

    #[derive(serde::Deserialize)]
    struct SemanticCorpusProgram {
        id: String,
        path: String,
        surfaces: Vec<String>,
        expected_stdout: String,
        rust_execution_test: String,
    }

    #[derive(serde::Deserialize)]
    struct RustExecutionReceipt {
        receipt_id: String,
        backend: String,
        evidence_class: String,
        result: RustExecutionReceiptResult,
        declared_effects: Vec<String>,
        observed_capabilities: Vec<String>,
        capability_gate: String,
        capability_gate_test: String,
        manifest_execution_test: String,
        receipt_consistency_test: String,
        validator_chain: Vec<String>,
        programs: Vec<RustExecutionReceiptProgram>,
    }

    #[derive(serde::Deserialize)]
    struct RustExecutionReceiptResult {
        passed: usize,
        failed: usize,
        ignored: usize,
    }

    #[derive(serde::Deserialize)]
    struct RustExecutionReceiptProgram {
        id: String,
        path: String,
        expected_stdout: String,
    }

    fn semantic_corpus_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("compiler crate should live below repo root")
            .join("semantic-corpus")
    }

    fn load_semantic_corpus_manifest() -> SemanticCorpusManifest {
        let manifest_path = semantic_corpus_root().join("manifest.json");
        let manifest = std::fs::read_to_string(&manifest_path)
            .unwrap_or_else(|err| panic!("read {}: {}", manifest_path.display(), err));
        serde_json::from_str(&manifest)
            .unwrap_or_else(|err| panic!("parse {}: {}", manifest_path.display(), err))
    }

    fn load_rust_execution_receipt() -> RustExecutionReceipt {
        let receipt_path = semantic_corpus_root()
            .join("receipts")
            .join("rust-execution-2026-06-13.json");
        let receipt = std::fs::read_to_string(&receipt_path)
            .unwrap_or_else(|err| panic!("read {}: {}", receipt_path.display(), err));
        serde_json::from_str(&receipt)
            .unwrap_or_else(|err| panic!("parse {}: {}", receipt_path.display(), err))
    }

    fn compile_build_to_rust(source: &str) -> String {
        let source_file = SourceFile::new("rust_backend_test.bld", source);
        let mut lexer = Lexer::new(&source_file);
        let tokens = lexer.tokenize().expect("lexing should succeed");
        let mut parser = Parser::new(&source_file, tokens);
        let ast = parser.parse().expect("parsing should succeed");
        assert!(
            parser.errors().is_empty(),
            "unexpected parser errors: {:?}",
            parser.errors()
        );

        let mut ctx = TypeContext::new();
        let mut checker = TypeChecker::new(&mut ctx);
        checker.set_source_file(&source_file);
        checker.check_module(&ast);
        assert!(
            !checker.has_errors(),
            "unexpected type errors: {:?}",
            checker.errors()
        );

        let mut codegen =
            CodeGenerator::with_source(&ctx, Target::Rust, source_file.source().into());
        codegen
            .generate(&ast)
            .expect("rust codegen should succeed")
            .as_string()
            .expect("generated Rust should be UTF-8")
    }

    fn assert_rustc_metadata_ok(name: &str, rust_source: &str) {
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let dir = std::env::temp_dir().join(format!(
            "buildlang_rust_backend_{}_{}",
            name,
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let source_path = dir.join("generated.rs");
        let metadata_path = dir.join("generated.rmeta");
        std::fs::write(&source_path, rust_source).expect("write generated Rust");

        let output = std::process::Command::new(rustc)
            .arg("--emit=metadata")
            .arg("-o")
            .arg(&metadata_path)
            .arg(&source_path)
            .output()
            .expect("invoke rustc");

        assert!(
            output.status.success(),
            "rustc failed for {name}\nstdout:\n{}\nstderr:\n{}\nsource:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
            rust_source
        );
    }

    fn assert_rustc_run_stdout(name: &str, rust_source: &str, expected_stdout: &str) {
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let dir = std::env::temp_dir().join(format!(
            "buildlang_rust_backend_run_{}_{}",
            name,
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let source_path = dir.join("generated.rs");
        let exe_path = dir.join(format!("generated{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&source_path, rust_source).expect("write generated Rust");

        let compile = std::process::Command::new(&rustc)
            .arg(&source_path)
            .arg("-o")
            .arg(&exe_path)
            .output()
            .expect("invoke rustc");
        assert!(
            compile.status.success(),
            "rustc failed for {name}\nstdout:\n{}\nstderr:\n{}\nsource:\n{}",
            String::from_utf8_lossy(&compile.stdout),
            String::from_utf8_lossy(&compile.stderr),
            rust_source
        );

        let run = std::process::Command::new(&exe_path)
            .output()
            .expect("run generated Rust executable");
        assert!(
            run.status.success(),
            "generated executable failed for {name}\nstdout:\n{}\nstderr:\n{}\nsource:\n{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr),
            rust_source
        );
        assert_eq!(String::from_utf8_lossy(&run.stdout), expected_stdout);
    }

    #[test]
    fn backend_target_is_rust() {
        let backend = RustBackend::new();
        assert_eq!(backend.target(), Target::Rust);
    }

    #[test]
    fn generated_rust_compiles_for_scalar_branch_subset() {
        let source = r#"
fn choose(x: i32) -> i32 {
    if x > 0 { x } else { 0 }
}

fn main() ~ Console {
    let v: i32 = choose(4);
    println("{}", v);
}
"#;
        let rust = compile_build_to_rust(source);
        assert_rustc_metadata_ok("scalar_branch", &rust);
    }

    #[test]
    fn generated_rust_runs_for_scalar_branch_subset() {
        let rust = compile_build_to_rust(CORPUS_SCALAR_BRANCH);
        assert_rustc_run_stdout("run_scalar_branch", &rust, "4\n");
    }

    #[test]
    fn generated_rust_compiles_for_reference_subset() {
        let source = r#"
fn add_to(x: &mut i32, amount: i32) {
    *x = *x + amount;
}

fn read_value(x: &i32) -> i32 {
    *x
}

fn main() ~ Console {
    let mut n: i32 = 10;
    add_to(&mut n, 5);
    let val: i32 = read_value(&n);
    println("{}", val);
}
"#;
        let rust = compile_build_to_rust(source);
        assert_rustc_metadata_ok("references", &rust);
    }

    #[test]
    fn generated_rust_runs_for_reference_subset() {
        let rust = compile_build_to_rust(CORPUS_REFERENCES_MUTATION);
        assert_rustc_run_stdout("run_references", &rust, "15\n");
    }

    #[test]
    fn generated_rust_compiles_for_structs_and_arrays() {
        let source = r#"
struct Point {
    x: i32,
    y: i32,
}

fn sum_array(arr: [i32; 3]) -> i32 {
    arr[0] + arr[1] + arr[2]
}

fn main() ~ Console {
    let p = Point { x: 3, y: 4 };
    let values = [p.x, p.y, 5];
    let total = sum_array(values);
    println("{}", total);
}
"#;
        let rust = compile_build_to_rust(source);
        assert_rustc_metadata_ok("structs_arrays", &rust);
    }

    #[test]
    fn generated_rust_runs_for_structs_and_arrays() {
        let rust = compile_build_to_rust(CORPUS_STRUCTS_ARRAYS);
        assert_rustc_run_stdout("run_structs_arrays", &rust, "12\n");
    }

    #[test]
    fn generated_rust_compiles_for_struct_field_references() {
        let source = r#"
struct Point {
    x: i32,
    y: i32,
}

fn main() ~ Console {
    let p = Point { x: 3, y: 4 };
    let rx: &i32 = &p.x;
    println("{}", *rx);
}
"#;
        let rust = compile_build_to_rust(source);
        assert_rustc_metadata_ok("struct_field_references", &rust);
    }

    #[test]
    fn generated_rust_compiles_for_repeated_non_copy_struct_arrays() {
        let source = r#"
struct Point {
    x: i32,
    y: i32,
}

fn main() ~ Console {
    let p = Point { x: 3, y: 4 };
    let points = [p; 2];
    println("{}", points[0].x);
}
"#;
        let rust = compile_build_to_rust(source);
        assert_rustc_metadata_ok("repeated_non_copy_struct_arrays", &rust);
    }

    #[test]
    fn generated_rust_compiles_for_reused_struct_after_by_value_call() {
        let source = r#"
struct Point {
    x: i32,
    y: i32,
}

fn sum(p: Point) -> i32 {
    p.x + p.y
}

fn main() ~ Console {
    let p = Point { x: 3, y: 4 };
    let first = sum(p);
    let second = sum(p);
    println("{}", first + second);
}
"#;
        let rust = compile_build_to_rust(source);
        assert_rustc_metadata_ok("reused_struct_after_by_value_call", &rust);
    }

    #[test]
    fn generated_rust_compiles_for_reused_struct_after_assignment() {
        let source = r#"
struct Point {
    x: i32,
    y: i32,
}

fn main() ~ Console {
    let p = Point { x: 3, y: 4 };
    let q = p;
    let r = p;
    println("{}", q.x + r.y);
}
"#;
        let rust = compile_build_to_rust(source);
        assert_rustc_metadata_ok("reused_struct_after_assignment", &rust);
    }

    #[test]
    fn generated_rust_compiles_for_reused_non_copy_struct_aggregate_field() {
        let source = r#"
struct Point {
    x: i32,
    y: i32,
}

struct Pair {
    left: Point,
    right: Point,
}

fn main() ~ Console {
    let p = Point { x: 3, y: 4 };
    let pair = Pair { left: p, right: p };
    println("{}", pair.left.x + pair.right.y);
}
"#;
        let rust = compile_build_to_rust(source);
        assert_rustc_metadata_ok("reused_non_copy_struct_aggregate_field", &rust);
    }

    #[test]
    fn generated_rust_runs_for_struct_aggregate_reuse() {
        let rust = compile_build_to_rust(CORPUS_STRUCT_AGGREGATE_REUSE);
        assert_rustc_run_stdout("run_struct_aggregate_reuse", &rust, "7\n");
    }

    #[test]
    fn generated_rust_compiles_for_reused_non_copy_tuple_aggregate_field() {
        let source = r#"
struct Point {
    x: i32,
    y: i32,
}

fn main() ~ Console {
    let p = Point { x: 3, y: 4 };
    let pair = (p, p);
    println("{}", pair.0.x + pair.1.y);
}
"#;
        let rust = compile_build_to_rust(source);
        assert_rustc_metadata_ok("reused_non_copy_tuple_aggregate_field", &rust);
    }

    #[test]
    fn generated_rust_compiles_for_reused_non_copy_after_field_assignment() {
        let source = r#"
struct Point {
    x: i32,
    y: i32,
}

struct Holder {
    item: Point,
}

fn main() ~ Console {
    let p = Point { x: 3, y: 4 };
    let mut holder = Holder { item: p };
    holder.item = p;
    let again = p;
    println("{}", holder.item.x + again.y);
}
"#;
        let rust = compile_build_to_rust(source);
        assert_rustc_metadata_ok("reused_non_copy_after_field_assignment", &rust);
    }

    #[test]
    fn generated_rust_runs_for_field_assignment_reuse() {
        let rust = compile_build_to_rust(CORPUS_FIELD_ASSIGNMENT_REUSE);
        assert_rustc_run_stdout("run_field_assignment_reuse", &rust, "7\n");
    }

    #[test]
    fn generated_rust_compiles_for_reused_tuple_after_by_value_call() {
        let source = r#"
fn sum(pair: (i32, i32)) -> i32 {
    pair.0 + pair.1
}

fn main() ~ Console {
    let pair = (3, 4);
    let first = sum(pair);
    let second = sum(pair);
    println("{}", first + second);
}
"#;
        let rust = compile_build_to_rust(source);
        assert_rustc_metadata_ok("reused_tuple_after_by_value_call", &rust);
    }

    #[test]
    fn generated_rust_runs_for_tuple_after_by_value_call() {
        let rust = compile_build_to_rust(CORPUS_TUPLE_OWNERSHIP_REUSE);
        assert_rustc_run_stdout("run_tuple_after_by_value_call", &rust, "14\n");
    }

    #[test]
    fn semantic_corpus_manifest_programs_run_on_rust_backend() {
        let corpus = load_semantic_corpus_manifest();
        for program in corpus.programs {
            let source = std::fs::read_to_string(semantic_corpus_root().join(&program.path))
                .unwrap_or_else(|err| panic!("read corpus program {}: {}", program.id, err));
            let rust = compile_build_to_rust(&source);
            assert_rustc_run_stdout(
                &format!("semantic_corpus_{}", program.id),
                &rust,
                &program.expected_stdout,
            );
        }
    }

    #[test]
    fn semantic_corpus_manifest_records_executable_contract() {
        let manifest = load_semantic_corpus_manifest();
        let receipt = load_rust_execution_receipt();
        let mut ids = std::collections::HashSet::new();

        assert_eq!(manifest.schema, "buildlang-semantic-corpus/v1");
        assert_eq!(
            manifest.rust_manifest_execution_test,
            "semantic_corpus_manifest_programs_run_on_rust_backend"
        );
        assert_eq!(manifest.programs.len(), receipt.programs.len());

        for program in &manifest.programs {
            assert!(
                !program.id.trim().is_empty(),
                "semantic corpus program id must be non-empty"
            );
            assert!(
                ids.insert(program.id.as_str()),
                "duplicate semantic corpus program id {}",
                program.id
            );
            assert!(
                program.path.starts_with("programs/"),
                "semantic corpus path {} should live under programs/",
                program.path
            );
            assert!(
                semantic_corpus_root().join(&program.path).is_file(),
                "semantic corpus program {} should exist",
                program.path
            );
            assert!(
                !program.expected_stdout.is_empty(),
                "semantic corpus program {} should declare expected stdout",
                program.id
            );
            assert!(
                program.expected_stdout.ends_with('\n'),
                "semantic corpus expected stdout for {} should end with a newline",
                program.id
            );
            assert!(
                !program.surfaces.is_empty(),
                "semantic corpus program {} should declare covered surfaces",
                program.id
            );
            assert!(
                program.surfaces.iter().any(|surface| surface == "stdout"),
                "semantic corpus program {} should declare stdout surface",
                program.id
            );
            assert!(
                program
                    .rust_execution_test
                    .starts_with("generated_rust_runs_for_"),
                "semantic corpus program {} should name a Rust execution test",
                program.id
            );
        }
    }

    #[test]
    fn semantic_corpus_receipt_matches_manifest() {
        let manifest = load_semantic_corpus_manifest();
        let receipt = load_rust_execution_receipt();

        assert_eq!(receipt.programs.len(), manifest.programs.len());
        for (manifest_program, receipt_program) in
            manifest.programs.iter().zip(receipt.programs.iter())
        {
            assert_eq!(receipt_program.id, manifest_program.id);
            assert_eq!(
                receipt_program.path.trim_start_matches("../"),
                manifest_program.path
            );
            assert_eq!(
                receipt_program.expected_stdout,
                manifest_program.expected_stdout
            );
        }
        assert_eq!(receipt.result.failed, 0);
        assert_eq!(receipt.result.ignored, 0);
        assert_eq!(receipt.result.passed, manifest.programs.len() + 1);
    }

    #[test]
    fn semantic_corpus_receipt_records_validator_metadata() {
        let receipt = load_rust_execution_receipt();

        assert_eq!(receipt.receipt_id, "rust-execution-2026-06-13");
        assert_eq!(receipt.backend, "rust");
        assert_eq!(receipt.evidence_class, "generated-artifact-execution");
        assert_eq!(receipt.declared_effects, vec!["Console"]);
        assert_eq!(receipt.observed_capabilities, vec!["Console"]);
        assert_eq!(receipt.capability_gate, "passed");
        assert_eq!(
            receipt.capability_gate_test,
            "cargo test --manifest-path compiler/Cargo.toml capability --quiet"
        );
        assert_eq!(
            receipt.manifest_execution_test,
            "semantic_corpus_manifest_programs_run_on_rust_backend"
        );
        assert_eq!(
            receipt.receipt_consistency_test,
            "semantic_corpus_receipt_matches_manifest"
        );
        assert_eq!(
            receipt.validator_chain,
            vec![
                "BuildLang parser",
                "BuildLang type checker",
                "MIR lowerer",
                "Rust backend",
                "rustc executable build",
                "stdout assertion",
            ]
        );
    }

    #[test]
    fn generated_rust_compiles_for_reused_non_copy_struct_field_access() {
        let source = r#"
struct Point {
    x: i32,
    y: i32,
}

struct Wrapper {
    inner: Point,
}

fn main() ~ Console {
    let p = Point { x: 3, y: 4 };
    let w = Wrapper { inner: p };
    let a = w.inner;
    let b = w.inner;
    println("{}", a.x + b.y);
}
"#;
        let rust = compile_build_to_rust(source);
        assert_rustc_metadata_ok("reused_non_copy_struct_field_access", &rust);
    }

    #[test]
    fn generated_rust_runs_for_nested_field_reuse() {
        let rust = compile_build_to_rust(CORPUS_NESTED_FIELD_REUSE);
        assert_rustc_run_stdout("run_nested_field_reuse", &rust, "7\n");
    }

    #[test]
    fn generated_rust_compiles_for_reused_non_copy_deref() {
        let source = r#"
struct Point {
    x: i32,
    y: i32,
}

fn main() ~ Console {
    let p = Point { x: 3, y: 4 };
    let rp: &Point = &p;
    let a = *rp;
    let b = *rp;
    println("{}", a.x + b.y);
}
"#;
        let rust = compile_build_to_rust(source);
        assert_rustc_metadata_ok("reused_non_copy_deref", &rust);
    }

    #[test]
    fn generated_rust_runs_for_deref_reuse() {
        let rust = compile_build_to_rust(CORPUS_DEREF_REUSE);
        assert_rustc_run_stdout("run_deref_reuse", &rust, "7\n");
    }

    #[test]
    fn generated_rust_compiles_for_lifetime_smoke_program() {
        let source = r#"
fn identity(x: &i32) -> &i32 {
    x
}

fn main() ~ Console {
    let a: i32 = 42;
    let r: &i32 = identity(&a);
    println("{}", *r);
}
"#;
        let rust = compile_build_to_rust(source);
        assert_rustc_metadata_ok("lifetime_smoke", &rust);
    }
}
