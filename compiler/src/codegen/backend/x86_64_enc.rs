// ===============================================================================
// QUANTALANG CODE GENERATOR - X86-64 INSTRUCTION ENCODER
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! x86-64 machine code instruction encoder.
//!
//! This module provides direct encoding of x86-64 instructions to binary
//! machine code, bypassing the need for an external assembler.
//!
//! ## Encoding Format
//!
//! x86-64 instructions have the following format:
//! ```text
//! [Legacy Prefixes] [REX] [Opcode] [ModR/M] [SIB] [Displacement] [Immediate]
//! ```
//!
//! ## Supported Features
//!
//! - General purpose registers (RAX-R15)
//! - Memory addressing modes (direct, indirect, scaled index)
//! - Immediate operands (8, 16, 32, 64-bit)
//! - All basic ALU operations
//! - Control flow instructions (JMP, Jcc, CALL, RET)
//! - SSE/AVX floating point operations

use std::collections::HashMap;

// =============================================================================
// Registers
// =============================================================================

/// x86-64 general purpose register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Reg64 {
    RAX = 0, RCX = 1, RDX = 2, RBX = 3,
    RSP = 4, RBP = 5, RSI = 6, RDI = 7,
    R8 = 8, R9 = 9, R10 = 10, R11 = 11,
    R12 = 12, R13 = 13, R14 = 14, R15 = 15,
}

impl Reg64 {
    /// Get the register encoding (3 bits).
    pub fn encoding(self) -> u8 {
        (self as u8) & 0x7
    }

    /// Check if this register requires REX.B prefix.
    pub fn requires_rex(self) -> bool {
        (self as u8) >= 8
    }

    /// Check if this is RSP/R12 (requires SIB byte in some addressing modes).
    pub fn requires_sib(self) -> bool {
        matches!(self, Reg64::RSP | Reg64::R12)
    }

    /// Check if this is RBP/R13 (requires displacement in some addressing modes).
    pub fn requires_disp(self) -> bool {
        matches!(self, Reg64::RBP | Reg64::R13)
    }

    /// Get the 32-bit version name.
    pub fn as_32(self) -> Reg32 {
        unsafe { std::mem::transmute(self as u8) }
    }
}

/// x86-64 32-bit register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Reg32 {
    EAX = 0, ECX = 1, EDX = 2, EBX = 3,
    ESP = 4, EBP = 5, ESI = 6, EDI = 7,
    R8D = 8, R9D = 9, R10D = 10, R11D = 11,
    R12D = 12, R13D = 13, R14D = 14, R15D = 15,
}

impl Reg32 {
    pub fn encoding(self) -> u8 {
        (self as u8) & 0x7
    }

    pub fn requires_rex(self) -> bool {
        (self as u8) >= 8
    }
}

/// x86-64 16-bit register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Reg16 {
    AX = 0, CX = 1, DX = 2, BX = 3,
    SP = 4, BP = 5, SI = 6, DI = 7,
    R8W = 8, R9W = 9, R10W = 10, R11W = 11,
    R12W = 12, R13W = 13, R14W = 14, R15W = 15,
}

impl Reg16 {
    pub fn encoding(self) -> u8 {
        (self as u8) & 0x7
    }

    pub fn requires_rex(self) -> bool {
        (self as u8) >= 8
    }
}

/// x86-64 8-bit register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Reg8 {
    AL = 0, CL = 1, DL = 2, BL = 3,
    AH = 4, CH = 5, DH = 6, BH = 7,
    R8B = 8, R9B = 9, R10B = 10, R11B = 11,
    R12B = 12, R13B = 13, R14B = 14, R15B = 15,
    SPL = 16, BPL = 17, SIL = 18, DIL = 19,
}

impl Reg8 {
    pub fn encoding(self) -> u8 {
        match self {
            Reg8::AL | Reg8::R8B => 0,
            Reg8::CL | Reg8::R9B => 1,
            Reg8::DL | Reg8::R10B => 2,
            Reg8::BL | Reg8::R11B => 3,
            Reg8::AH | Reg8::SPL | Reg8::R12B => 4,
            Reg8::CH | Reg8::BPL | Reg8::R13B => 5,
            Reg8::DH | Reg8::SIL | Reg8::R14B => 6,
            Reg8::BH | Reg8::DIL | Reg8::R15B => 7,
        }
    }

    pub fn requires_rex(self) -> bool {
        (self as u8) >= 8
    }
}

/// XMM/YMM registers for SIMD operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RegXmm {
    XMM0 = 0, XMM1 = 1, XMM2 = 2, XMM3 = 3,
    XMM4 = 4, XMM5 = 5, XMM6 = 6, XMM7 = 7,
    XMM8 = 8, XMM9 = 9, XMM10 = 10, XMM11 = 11,
    XMM12 = 12, XMM13 = 13, XMM14 = 14, XMM15 = 15,
}

impl RegXmm {
    pub fn encoding(self) -> u8 {
        (self as u8) & 0x7
    }

    pub fn requires_rex(self) -> bool {
        (self as u8) >= 8
    }
}

// =============================================================================
// Memory Operands
// =============================================================================

/// Scale factor for SIB byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Scale {
    X1 = 0,
    X2 = 1,
    X4 = 2,
    X8 = 3,
}

/// Memory operand.
#[derive(Debug, Clone, Copy)]
pub struct Mem {
    /// Base register (None for RIP-relative or absolute).
    pub base: Option<Reg64>,
    /// Index register for SIB addressing.
    pub index: Option<Reg64>,
    /// Scale factor for index.
    pub scale: Scale,
    /// Displacement.
    pub disp: i32,
    /// RIP-relative addressing.
    pub rip_relative: bool,
}

impl Mem {
    /// Create a simple base+displacement memory operand.
    pub fn base_disp(base: Reg64, disp: i32) -> Self {
        Self {
            base: Some(base),
            index: None,
            scale: Scale::X1,
            disp,
            rip_relative: false,
        }
    }

    /// Create a base only memory operand.
    pub fn base(base: Reg64) -> Self {
        Self::base_disp(base, 0)
    }

    /// Create a scaled index memory operand.
    pub fn base_index_scale_disp(base: Reg64, index: Reg64, scale: Scale, disp: i32) -> Self {
        Self {
            base: Some(base),
            index: Some(index),
            scale,
            disp,
            rip_relative: false,
        }
    }

    /// Create a RIP-relative memory operand.
    pub fn rip_relative(disp: i32) -> Self {
        Self {
            base: None,
            index: None,
            scale: Scale::X1,
            disp,
            rip_relative: true,
        }
    }

    /// Create an absolute address memory operand.
    pub fn absolute(addr: i32) -> Self {
        Self {
            base: None,
            index: None,
            scale: Scale::X1,
            disp: addr,
            rip_relative: false,
        }
    }

    /// Check if this memory operand needs a SIB byte.
    fn needs_sib(&self) -> bool {
        self.index.is_some() ||
        self.base.map_or(false, |b| b.requires_sib()) ||
        (self.base.is_none() && !self.rip_relative)
    }

    /// Get the Mod field for ModR/M byte.
    fn get_mod(&self) -> u8 {
        if self.base.is_none() {
            if self.rip_relative {
                0b00 // RIP-relative uses mod=00 with r/m=101
            } else {
                0b00 // Absolute uses mod=00 with SIB
            }
        } else if self.disp == 0 && !self.base.unwrap().requires_disp() {
            0b00
        } else if self.disp >= -128 && self.disp <= 127 {
            0b01
        } else {
            0b10
        }
    }
}

// =============================================================================
// Operand Types
// =============================================================================

/// Generic operand for instruction encoding.
#[derive(Debug, Clone, Copy)]
pub enum Operand {
    /// 64-bit register.
    Reg64(Reg64),
    /// 32-bit register.
    Reg32(Reg32),
    /// 16-bit register.
    Reg16(Reg16),
    /// 8-bit register.
    Reg8(Reg8),
    /// XMM register.
    Xmm(RegXmm),
    /// Memory operand.
    Mem(Mem),
    /// 8-bit immediate.
    Imm8(i8),
    /// 16-bit immediate.
    Imm16(i16),
    /// 32-bit immediate.
    Imm32(i32),
    /// 64-bit immediate.
    Imm64(i64),
}

// =============================================================================
// Condition Codes
// =============================================================================

/// x86-64 condition codes for Jcc, SETcc, CMOVcc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Cond {
    /// Overflow (OF=1).
    O = 0x0,
    /// No overflow (OF=0).
    NO = 0x1,
    /// Below/Carry (CF=1).
    B = 0x2,
    /// Above or equal/No carry (CF=0).
    AE = 0x3,
    /// Equal/Zero (ZF=1).
    E = 0x4,
    /// Not equal/Not zero (ZF=0).
    NE = 0x5,
    /// Below or equal (CF=1 or ZF=1).
    BE = 0x6,
    /// Above (CF=0 and ZF=0).
    A = 0x7,
    /// Sign (SF=1).
    S = 0x8,
    /// No sign (SF=0).
    NS = 0x9,
    /// Parity (PF=1).
    P = 0xA,
    /// No parity (PF=0).
    NP = 0xB,
    /// Less (SF≠OF).
    L = 0xC,
    /// Greater or equal (SF=OF).
    GE = 0xD,
    /// Less or equal (ZF=1 or SF≠OF).
    LE = 0xE,
    /// Greater (ZF=0 and SF=OF).
    G = 0xF,
}

impl Cond {
    /// Get the inverted condition.
    pub fn invert(self) -> Self {
        unsafe { std::mem::transmute((self as u8) ^ 1) }
    }
}

// =============================================================================
// REX Prefix
// =============================================================================

/// REX prefix byte builder.
#[derive(Debug, Clone, Copy, Default)]
pub struct Rex {
    /// W bit: 64-bit operand size.
    pub w: bool,
    /// R bit: Extension of ModR/M reg field.
    pub r: bool,
    /// X bit: Extension of SIB index field.
    pub x: bool,
    /// B bit: Extension of ModR/M r/m, SIB base, or opcode reg.
    pub b: bool,
}

impl Rex {
    /// Check if REX prefix is needed.
    pub fn is_needed(&self) -> bool {
        self.w || self.r || self.x || self.b
    }

    /// Encode to byte.
    pub fn encode(&self) -> u8 {
        0x40 | ((self.w as u8) << 3) | ((self.r as u8) << 2) | ((self.x as u8) << 1) | (self.b as u8)
    }
}

// =============================================================================
// ModR/M and SIB Bytes
// =============================================================================

/// Encode ModR/M byte.
pub fn encode_modrm(mod_: u8, reg: u8, rm: u8) -> u8 {
    ((mod_ & 0x3) << 6) | ((reg & 0x7) << 3) | (rm & 0x7)
}

/// Encode SIB byte.
pub fn encode_sib(scale: u8, index: u8, base: u8) -> u8 {
    ((scale & 0x3) << 6) | ((index & 0x7) << 3) | (base & 0x7)
}

// =============================================================================
// Instruction Encoder
// =============================================================================

/// Relocation entry for later fixup.
#[derive(Debug, Clone)]
pub struct Relocation {
    /// Offset in the code where the relocation applies.
    pub offset: usize,
    /// Symbol name for external references.
    pub symbol: String,
    /// Relocation type.
    pub kind: RelocKind,
    /// Addend for the relocation.
    pub addend: i64,
}

/// Relocation kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelocKind {
    /// 32-bit PC-relative.
    Rel32,
    /// 64-bit absolute.
    Abs64,
    /// 32-bit absolute.
    Abs32,
    /// GOT entry.
    GotPcRel,
    /// PLT entry.
    PltRel,
}

/// Label reference for forward jumps.
#[derive(Debug, Clone)]
pub struct LabelRef {
    /// Offset in code where the label is referenced.
    pub offset: usize,
    /// Label ID.
    pub label: u32,
    /// Size of the displacement (1, 2, or 4 bytes).
    pub size: u8,
}

/// x86-64 instruction encoder.
pub struct X86_64Encoder {
    /// Output buffer.
    pub code: Vec<u8>,
    /// Label positions.
    pub labels: HashMap<u32, usize>,
    /// Forward label references to fix up.
    pub label_refs: Vec<LabelRef>,
    /// External relocations.
    pub relocations: Vec<Relocation>,
    /// Next label ID.
    next_label: u32,
}

impl X86_64Encoder {
    /// Create a new encoder.
    pub fn new() -> Self {
        Self {
            code: Vec::new(),
            labels: HashMap::new(),
            label_refs: Vec::new(),
            relocations: Vec::new(),
            next_label: 0,
        }
    }

    /// Get current code position.
    pub fn position(&self) -> usize {
        self.code.len()
    }

    /// Allocate a new label.
    pub fn new_label(&mut self) -> u32 {
        let id = self.next_label;
        self.next_label += 1;
        id
    }

    /// Define a label at the current position.
    pub fn define_label(&mut self, label: u32) {
        self.labels.insert(label, self.code.len());
    }

    /// Emit a raw byte.
    pub fn emit_u8(&mut self, b: u8) {
        self.code.push(b);
    }

    /// Emit a 16-bit value.
    pub fn emit_u16(&mut self, v: u16) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }

    /// Emit a 32-bit value.
    pub fn emit_u32(&mut self, v: u32) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }

    /// Emit a 64-bit value.
    pub fn emit_u64(&mut self, v: u64) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }

    /// Emit REX prefix if needed.
    fn emit_rex(&mut self, rex: Rex) {
        if rex.is_needed() {
            self.emit_u8(rex.encode());
        }
    }

    /// Emit REX prefix for reg-reg operation.
    fn emit_rex_rr(&mut self, dst: Reg64, src: Reg64, w: bool) {
        let rex = Rex {
            w,
            r: dst.requires_rex(),
            x: false,
            b: src.requires_rex(),
        };
        self.emit_rex(rex);
    }

    /// Emit REX prefix for reg-mem operation.
    fn emit_rex_rm(&mut self, reg: Reg64, mem: &Mem, w: bool) {
        let rex = Rex {
            w,
            r: reg.requires_rex(),
            x: mem.index.map_or(false, |i| i.requires_rex()),
            b: mem.base.map_or(false, |b| b.requires_rex()),
        };
        self.emit_rex(rex);
    }

    /// Emit ModR/M byte for reg-reg.
    fn emit_modrm_rr(&mut self, dst: Reg64, src: Reg64) {
        self.emit_u8(encode_modrm(0b11, dst.encoding(), src.encoding()));
    }

    /// Emit ModR/M and SIB bytes for reg-mem.
    fn emit_modrm_rm(&mut self, reg: Reg64, mem: &Mem) {
        let mod_ = mem.get_mod();

        if mem.rip_relative {
            // RIP-relative: mod=00, r/m=101
            self.emit_u8(encode_modrm(0b00, reg.encoding(), 0b101));
            self.emit_u32(mem.disp as u32);
            return;
        }

        if mem.needs_sib() {
            // Need SIB byte
            self.emit_u8(encode_modrm(mod_, reg.encoding(), 0b100));

            let base_enc = mem.base.map_or(0b101, |b| b.encoding());
            let index_enc = mem.index.map_or(0b100, |i| i.encoding()); // 100 = no index

            self.emit_u8(encode_sib(mem.scale as u8, index_enc, base_enc));
        } else {
            let base = mem.base.unwrap();
            self.emit_u8(encode_modrm(mod_, reg.encoding(), base.encoding()));
        }

        // Emit displacement
        match mod_ {
            0b00 if mem.base.is_none() => {
                self.emit_u32(mem.disp as u32);
            }
            0b01 => {
                self.emit_u8(mem.disp as u8);
            }
            0b10 => {
                self.emit_u32(mem.disp as u32);
            }
            _ => {}
        }
    }

    // =========================================================================
    // Data Movement Instructions
    // =========================================================================

    /// MOV r64, r64
    pub fn mov_rr(&mut self, dst: Reg64, src: Reg64) {
        self.emit_rex_rr(dst, src, true);
        self.emit_u8(0x89);
        self.emit_modrm_rr(src, dst);
    }

    /// MOV r64, imm64
    pub fn mov_ri64(&mut self, dst: Reg64, imm: i64) {
        self.emit_rex(Rex {
            w: true,
            r: false,
            x: false,
            b: dst.requires_rex(),
        });
        self.emit_u8(0xB8 + dst.encoding());
        self.emit_u64(imm as u64);
    }

    /// MOV r64, imm32 (sign-extended)
    pub fn mov_ri32(&mut self, dst: Reg64, imm: i32) {
        self.emit_rex(Rex {
            w: true,
            r: false,
            x: false,
            b: dst.requires_rex(),
        });
        self.emit_u8(0xC7);
        self.emit_u8(encode_modrm(0b11, 0, dst.encoding()));
        self.emit_u32(imm as u32);
    }

    /// MOV r32, imm32
    pub fn mov_r32i32(&mut self, dst: Reg32, imm: i32) {
        if dst.requires_rex() {
            self.emit_rex(Rex {
                w: false,
                r: false,
                x: false,
                b: true,
            });
        }
        self.emit_u8(0xB8 + dst.encoding());
        self.emit_u32(imm as u32);
    }

    /// MOV r64, [mem]
    pub fn mov_rm(&mut self, dst: Reg64, src: &Mem) {
        self.emit_rex_rm(dst, src, true);
        self.emit_u8(0x8B);
        self.emit_modrm_rm(dst, src);
    }

    /// MOV [mem], r64
    pub fn mov_mr(&mut self, dst: &Mem, src: Reg64) {
        self.emit_rex_rm(src, dst, true);
        self.emit_u8(0x89);
        self.emit_modrm_rm(src, dst);
    }

    /// MOV [mem], imm32 (sign-extended to 64)
    pub fn mov_mi32(&mut self, dst: &Mem, imm: i32) {
        self.emit_rex_rm(Reg64::RAX, dst, true); // RAX encoding = 0
        self.emit_u8(0xC7);
        self.emit_modrm_rm(Reg64::RAX, dst);
        self.emit_u32(imm as u32);
    }

    /// MOVZX r64, r8
    pub fn movzx_r64_r8(&mut self, dst: Reg64, src: Reg8) {
        self.emit_rex(Rex {
            w: true,
            r: dst.requires_rex(),
            x: false,
            b: src.requires_rex(),
        });
        self.emit_u8(0x0F);
        self.emit_u8(0xB6);
        self.emit_u8(encode_modrm(0b11, dst.encoding(), src.encoding()));
    }

    /// MOVSX r64, r32
    pub fn movsxd(&mut self, dst: Reg64, src: Reg32) {
        self.emit_rex(Rex {
            w: true,
            r: dst.requires_rex(),
            x: false,
            b: src.requires_rex(),
        });
        self.emit_u8(0x63);
        self.emit_u8(encode_modrm(0b11, dst.encoding(), src.encoding()));
    }

    /// LEA r64, [mem]
    pub fn lea(&mut self, dst: Reg64, src: &Mem) {
        self.emit_rex_rm(dst, src, true);
        self.emit_u8(0x8D);
        self.emit_modrm_rm(dst, src);
    }

    /// XCHG r64, r64
    pub fn xchg(&mut self, a: Reg64, b: Reg64) {
        if a == Reg64::RAX {
            self.emit_rex(Rex {
                w: true,
                r: false,
                x: false,
                b: b.requires_rex(),
            });
            self.emit_u8(0x90 + b.encoding());
        } else if b == Reg64::RAX {
            self.emit_rex(Rex {
                w: true,
                r: false,
                x: false,
                b: a.requires_rex(),
            });
            self.emit_u8(0x90 + a.encoding());
        } else {
            self.emit_rex_rr(a, b, true);
            self.emit_u8(0x87);
            self.emit_modrm_rr(a, b);
        }
    }

    /// PUSH r64
    pub fn push(&mut self, reg: Reg64) {
        if reg.requires_rex() {
            self.emit_rex(Rex {
                w: false,
                r: false,
                x: false,
                b: true,
            });
        }
        self.emit_u8(0x50 + reg.encoding());
    }

    /// POP r64
    pub fn pop(&mut self, reg: Reg64) {
        if reg.requires_rex() {
            self.emit_rex(Rex {
                w: false,
                r: false,
                x: false,
                b: true,
            });
        }
        self.emit_u8(0x58 + reg.encoding());
    }

    /// PUSH imm32
    pub fn push_imm32(&mut self, imm: i32) {
        self.emit_u8(0x68);
        self.emit_u32(imm as u32);
    }

    /// PUSH imm8
    pub fn push_imm8(&mut self, imm: i8) {
        self.emit_u8(0x6A);
        self.emit_u8(imm as u8);
    }

    // =========================================================================
    // Arithmetic Instructions
    // =========================================================================

    /// ADD r64, r64
    pub fn add_rr(&mut self, dst: Reg64, src: Reg64) {
        self.emit_rex_rr(src, dst, true);
        self.emit_u8(0x01);
        self.emit_modrm_rr(src, dst);
    }

    /// ADD r64, imm32
    pub fn add_ri32(&mut self, dst: Reg64, imm: i32) {
        self.emit_rex(Rex {
            w: true,
            r: false,
            x: false,
            b: dst.requires_rex(),
        });
        if imm >= -128 && imm <= 127 {
            self.emit_u8(0x83);
            self.emit_u8(encode_modrm(0b11, 0, dst.encoding()));
            self.emit_u8(imm as u8);
        } else if dst == Reg64::RAX {
            self.emit_u8(0x05);
            self.emit_u32(imm as u32);
        } else {
            self.emit_u8(0x81);
            self.emit_u8(encode_modrm(0b11, 0, dst.encoding()));
            self.emit_u32(imm as u32);
        }
    }

    /// ADD r64, [mem]
    pub fn add_rm(&mut self, dst: Reg64, src: &Mem) {
        self.emit_rex_rm(dst, src, true);
        self.emit_u8(0x03);
        self.emit_modrm_rm(dst, src);
    }

    /// SUB r64, r64
    pub fn sub_rr(&mut self, dst: Reg64, src: Reg64) {
        self.emit_rex_rr(src, dst, true);
        self.emit_u8(0x29);
        self.emit_modrm_rr(src, dst);
    }

    /// SUB r64, imm32
    pub fn sub_ri32(&mut self, dst: Reg64, imm: i32) {
        self.emit_rex(Rex {
            w: true,
            r: false,
            x: false,
            b: dst.requires_rex(),
        });
        if imm >= -128 && imm <= 127 {
            self.emit_u8(0x83);
            self.emit_u8(encode_modrm(0b11, 5, dst.encoding()));
            self.emit_u8(imm as u8);
        } else if dst == Reg64::RAX {
            self.emit_u8(0x2D);
            self.emit_u32(imm as u32);
        } else {
            self.emit_u8(0x81);
            self.emit_u8(encode_modrm(0b11, 5, dst.encoding()));
            self.emit_u32(imm as u32);
        }
    }

    /// IMUL r64, r64
    pub fn imul_rr(&mut self, dst: Reg64, src: Reg64) {
        self.emit_rex_rr(dst, src, true);
        self.emit_u8(0x0F);
        self.emit_u8(0xAF);
        self.emit_modrm_rr(dst, src);
    }

    /// IMUL r64, r64, imm32
    pub fn imul_rri32(&mut self, dst: Reg64, src: Reg64, imm: i32) {
        self.emit_rex_rr(dst, src, true);
        if imm >= -128 && imm <= 127 {
            self.emit_u8(0x6B);
            self.emit_modrm_rr(dst, src);
            self.emit_u8(imm as u8);
        } else {
            self.emit_u8(0x69);
            self.emit_modrm_rr(dst, src);
            self.emit_u32(imm as u32);
        }
    }

    /// IDIV r64 (RDX:RAX / src -> RAX, RDX)
    pub fn idiv(&mut self, src: Reg64) {
        self.emit_rex(Rex {
            w: true,
            r: false,
            x: false,
            b: src.requires_rex(),
        });
        self.emit_u8(0xF7);
        self.emit_u8(encode_modrm(0b11, 7, src.encoding()));
    }

    /// DIV r64 (unsigned)
    pub fn div(&mut self, src: Reg64) {
        self.emit_rex(Rex {
            w: true,
            r: false,
            x: false,
            b: src.requires_rex(),
        });
        self.emit_u8(0xF7);
        self.emit_u8(encode_modrm(0b11, 6, src.encoding()));
    }

    /// CQO (sign-extend RAX into RDX:RAX)
    pub fn cqo(&mut self) {
        self.emit_rex(Rex {
            w: true,
            r: false,
            x: false,
            b: false,
        });
        self.emit_u8(0x99);
    }

    /// CDQ (sign-extend EAX into EDX:EAX)
    pub fn cdq(&mut self) {
        self.emit_u8(0x99);
    }

    /// NEG r64
    pub fn neg(&mut self, dst: Reg64) {
        self.emit_rex(Rex {
            w: true,
            r: false,
            x: false,
            b: dst.requires_rex(),
        });
        self.emit_u8(0xF7);
        self.emit_u8(encode_modrm(0b11, 3, dst.encoding()));
    }

    /// INC r64
    pub fn inc(&mut self, dst: Reg64) {
        self.emit_rex(Rex {
            w: true,
            r: false,
            x: false,
            b: dst.requires_rex(),
        });
        self.emit_u8(0xFF);
        self.emit_u8(encode_modrm(0b11, 0, dst.encoding()));
    }

    /// DEC r64
    pub fn dec(&mut self, dst: Reg64) {
        self.emit_rex(Rex {
            w: true,
            r: false,
            x: false,
            b: dst.requires_rex(),
        });
        self.emit_u8(0xFF);
        self.emit_u8(encode_modrm(0b11, 1, dst.encoding()));
    }

    // =========================================================================
    // Bitwise Instructions
    // =========================================================================

    /// AND r64, r64
    pub fn and_rr(&mut self, dst: Reg64, src: Reg64) {
        self.emit_rex_rr(src, dst, true);
        self.emit_u8(0x21);
        self.emit_modrm_rr(src, dst);
    }

    /// AND r64, imm32
    pub fn and_ri32(&mut self, dst: Reg64, imm: i32) {
        self.emit_rex(Rex {
            w: true,
            r: false,
            x: false,
            b: dst.requires_rex(),
        });
        if imm >= -128 && imm <= 127 {
            self.emit_u8(0x83);
            self.emit_u8(encode_modrm(0b11, 4, dst.encoding()));
            self.emit_u8(imm as u8);
        } else if dst == Reg64::RAX {
            self.emit_u8(0x25);
            self.emit_u32(imm as u32);
        } else {
            self.emit_u8(0x81);
            self.emit_u8(encode_modrm(0b11, 4, dst.encoding()));
            self.emit_u32(imm as u32);
        }
    }

    /// OR r64, r64
    pub fn or_rr(&mut self, dst: Reg64, src: Reg64) {
        self.emit_rex_rr(src, dst, true);
        self.emit_u8(0x09);
        self.emit_modrm_rr(src, dst);
    }

    /// OR r64, imm32
    pub fn or_ri32(&mut self, dst: Reg64, imm: i32) {
        self.emit_rex(Rex {
            w: true,
            r: false,
            x: false,
            b: dst.requires_rex(),
        });
        if imm >= -128 && imm <= 127 {
            self.emit_u8(0x83);
            self.emit_u8(encode_modrm(0b11, 1, dst.encoding()));
            self.emit_u8(imm as u8);
        } else if dst == Reg64::RAX {
            self.emit_u8(0x0D);
            self.emit_u32(imm as u32);
        } else {
            self.emit_u8(0x81);
            self.emit_u8(encode_modrm(0b11, 1, dst.encoding()));
            self.emit_u32(imm as u32);
        }
    }

    /// XOR r64, r64
    pub fn xor_rr(&mut self, dst: Reg64, src: Reg64) {
        self.emit_rex_rr(src, dst, true);
        self.emit_u8(0x31);
        self.emit_modrm_rr(src, dst);
    }

    /// XOR r64, imm32
    pub fn xor_ri32(&mut self, dst: Reg64, imm: i32) {
        self.emit_rex(Rex {
            w: true,
            r: false,
            x: false,
            b: dst.requires_rex(),
        });
        if imm >= -128 && imm <= 127 {
            self.emit_u8(0x83);
            self.emit_u8(encode_modrm(0b11, 6, dst.encoding()));
            self.emit_u8(imm as u8);
        } else if dst == Reg64::RAX {
            self.emit_u8(0x35);
            self.emit_u32(imm as u32);
        } else {
            self.emit_u8(0x81);
            self.emit_u8(encode_modrm(0b11, 6, dst.encoding()));
            self.emit_u32(imm as u32);
        }
    }

    /// NOT r64
    pub fn not(&mut self, dst: Reg64) {
        self.emit_rex(Rex {
            w: true,
            r: false,
            x: false,
            b: dst.requires_rex(),
        });
        self.emit_u8(0xF7);
        self.emit_u8(encode_modrm(0b11, 2, dst.encoding()));
    }

    /// SHL r64, cl
    pub fn shl_cl(&mut self, dst: Reg64) {
        self.emit_rex(Rex {
            w: true,
            r: false,
            x: false,
            b: dst.requires_rex(),
        });
        self.emit_u8(0xD3);
        self.emit_u8(encode_modrm(0b11, 4, dst.encoding()));
    }

    /// SHL r64, imm8
    pub fn shl_imm(&mut self, dst: Reg64, imm: u8) {
        self.emit_rex(Rex {
            w: true,
            r: false,
            x: false,
            b: dst.requires_rex(),
        });
        if imm == 1 {
            self.emit_u8(0xD1);
            self.emit_u8(encode_modrm(0b11, 4, dst.encoding()));
        } else {
            self.emit_u8(0xC1);
            self.emit_u8(encode_modrm(0b11, 4, dst.encoding()));
            self.emit_u8(imm);
        }
    }

    /// SHR r64, cl (logical shift right)
    pub fn shr_cl(&mut self, dst: Reg64) {
        self.emit_rex(Rex {
            w: true,
            r: false,
            x: false,
            b: dst.requires_rex(),
        });
        self.emit_u8(0xD3);
        self.emit_u8(encode_modrm(0b11, 5, dst.encoding()));
    }

    /// SHR r64, imm8
    pub fn shr_imm(&mut self, dst: Reg64, imm: u8) {
        self.emit_rex(Rex {
            w: true,
            r: false,
            x: false,
            b: dst.requires_rex(),
        });
        if imm == 1 {
            self.emit_u8(0xD1);
            self.emit_u8(encode_modrm(0b11, 5, dst.encoding()));
        } else {
            self.emit_u8(0xC1);
            self.emit_u8(encode_modrm(0b11, 5, dst.encoding()));
            self.emit_u8(imm);
        }
    }

    /// SAR r64, cl (arithmetic shift right)
    pub fn sar_cl(&mut self, dst: Reg64) {
        self.emit_rex(Rex {
            w: true,
            r: false,
            x: false,
            b: dst.requires_rex(),
        });
        self.emit_u8(0xD3);
        self.emit_u8(encode_modrm(0b11, 7, dst.encoding()));
    }

    /// SAR r64, imm8
    pub fn sar_imm(&mut self, dst: Reg64, imm: u8) {
        self.emit_rex(Rex {
            w: true,
            r: false,
            x: false,
            b: dst.requires_rex(),
        });
        if imm == 1 {
            self.emit_u8(0xD1);
            self.emit_u8(encode_modrm(0b11, 7, dst.encoding()));
        } else {
            self.emit_u8(0xC1);
            self.emit_u8(encode_modrm(0b11, 7, dst.encoding()));
            self.emit_u8(imm);
        }
    }

    // =========================================================================
    // Comparison and Test Instructions
    // =========================================================================

    /// CMP r64, r64
    pub fn cmp_rr(&mut self, a: Reg64, b: Reg64) {
        self.emit_rex_rr(b, a, true);
        self.emit_u8(0x39);
        self.emit_modrm_rr(b, a);
    }

    /// CMP r64, imm32
    pub fn cmp_ri32(&mut self, dst: Reg64, imm: i32) {
        self.emit_rex(Rex {
            w: true,
            r: false,
            x: false,
            b: dst.requires_rex(),
        });
        if imm >= -128 && imm <= 127 {
            self.emit_u8(0x83);
            self.emit_u8(encode_modrm(0b11, 7, dst.encoding()));
            self.emit_u8(imm as u8);
        } else if dst == Reg64::RAX {
            self.emit_u8(0x3D);
            self.emit_u32(imm as u32);
        } else {
            self.emit_u8(0x81);
            self.emit_u8(encode_modrm(0b11, 7, dst.encoding()));
            self.emit_u32(imm as u32);
        }
    }

    /// CMP r64, [mem]
    pub fn cmp_rm(&mut self, dst: Reg64, src: &Mem) {
        self.emit_rex_rm(dst, src, true);
        self.emit_u8(0x3B);
        self.emit_modrm_rm(dst, src);
    }

    /// TEST r64, r64
    pub fn test_rr(&mut self, a: Reg64, b: Reg64) {
        self.emit_rex_rr(b, a, true);
        self.emit_u8(0x85);
        self.emit_modrm_rr(b, a);
    }

    /// TEST r64, imm32
    pub fn test_ri32(&mut self, dst: Reg64, imm: i32) {
        self.emit_rex(Rex {
            w: true,
            r: false,
            x: false,
            b: dst.requires_rex(),
        });
        if dst == Reg64::RAX {
            self.emit_u8(0xA9);
        } else {
            self.emit_u8(0xF7);
            self.emit_u8(encode_modrm(0b11, 0, dst.encoding()));
        }
        self.emit_u32(imm as u32);
    }

    /// SETcc r8
    pub fn setcc(&mut self, cond: Cond, dst: Reg8) {
        if dst.requires_rex() {
            self.emit_rex(Rex {
                w: false,
                r: false,
                x: false,
                b: true,
            });
        }
        self.emit_u8(0x0F);
        self.emit_u8(0x90 + cond as u8);
        self.emit_u8(encode_modrm(0b11, 0, dst.encoding()));
    }

    /// CMOVcc r64, r64
    pub fn cmovcc(&mut self, cond: Cond, dst: Reg64, src: Reg64) {
        self.emit_rex_rr(dst, src, true);
        self.emit_u8(0x0F);
        self.emit_u8(0x40 + cond as u8);
        self.emit_modrm_rr(dst, src);
    }

    // =========================================================================
    // Control Flow Instructions
    // =========================================================================

    /// JMP rel32
    pub fn jmp_rel32(&mut self, offset: i32) {
        self.emit_u8(0xE9);
        self.emit_u32(offset as u32);
    }

    /// JMP to label (forward reference)
    pub fn jmp_label(&mut self, label: u32) {
        if let Some(&target) = self.labels.get(&label) {
            // Label already defined, calculate offset
            let offset = (target as i64) - (self.code.len() as i64 + 5);
            self.jmp_rel32(offset as i32);
        } else {
            // Forward reference
            self.emit_u8(0xE9);
            self.label_refs.push(LabelRef {
                offset: self.code.len(),
                label,
                size: 4,
            });
            self.emit_u32(0); // Placeholder
        }
    }

    /// JMP r64
    pub fn jmp_reg(&mut self, target: Reg64) {
        if target.requires_rex() {
            self.emit_rex(Rex {
                w: false,
                r: false,
                x: false,
                b: true,
            });
        }
        self.emit_u8(0xFF);
        self.emit_u8(encode_modrm(0b11, 4, target.encoding()));
    }

    /// Jcc rel32
    pub fn jcc_rel32(&mut self, cond: Cond, offset: i32) {
        self.emit_u8(0x0F);
        self.emit_u8(0x80 + cond as u8);
        self.emit_u32(offset as u32);
    }

    /// Jcc to label (forward reference)
    pub fn jcc_label(&mut self, cond: Cond, label: u32) {
        if let Some(&target) = self.labels.get(&label) {
            let offset = (target as i64) - (self.code.len() as i64 + 6);
            self.jcc_rel32(cond, offset as i32);
        } else {
            self.emit_u8(0x0F);
            self.emit_u8(0x80 + cond as u8);
            self.label_refs.push(LabelRef {
                offset: self.code.len(),
                label,
                size: 4,
            });
            self.emit_u32(0);
        }
    }

    /// Jcc rel8 (short jump)
    pub fn jcc_rel8(&mut self, cond: Cond, offset: i8) {
        self.emit_u8(0x70 + cond as u8);
        self.emit_u8(offset as u8);
    }

    /// CALL rel32
    pub fn call_rel32(&mut self, offset: i32) {
        self.emit_u8(0xE8);
        self.emit_u32(offset as u32);
    }

    /// CALL to symbol (external relocation)
    pub fn call_symbol(&mut self, symbol: &str) {
        self.emit_u8(0xE8);
        self.relocations.push(Relocation {
            offset: self.code.len(),
            symbol: symbol.to_string(),
            kind: RelocKind::Rel32,
            addend: -4,
        });
        self.emit_u32(0);
    }

    /// CALL r64
    pub fn call_reg(&mut self, target: Reg64) {
        if target.requires_rex() {
            self.emit_rex(Rex {
                w: false,
                r: false,
                x: false,
                b: true,
            });
        }
        self.emit_u8(0xFF);
        self.emit_u8(encode_modrm(0b11, 2, target.encoding()));
    }

    /// RET
    pub fn ret(&mut self) {
        self.emit_u8(0xC3);
    }

    /// RET imm16
    pub fn ret_imm(&mut self, imm: u16) {
        self.emit_u8(0xC2);
        self.emit_u16(imm);
    }

    /// NOP
    pub fn nop(&mut self) {
        self.emit_u8(0x90);
    }

    /// Multi-byte NOP (for alignment)
    pub fn nop_n(&mut self, n: usize) {
        // Use recommended NOP encodings
        let nops: &[&[u8]] = &[
            &[],                                          // 0
            &[0x90],                                      // 1
            &[0x66, 0x90],                               // 2
            &[0x0F, 0x1F, 0x00],                         // 3
            &[0x0F, 0x1F, 0x40, 0x00],                   // 4
            &[0x0F, 0x1F, 0x44, 0x00, 0x00],             // 5
            &[0x66, 0x0F, 0x1F, 0x44, 0x00, 0x00],       // 6
            &[0x0F, 0x1F, 0x80, 0x00, 0x00, 0x00, 0x00], // 7
            &[0x0F, 0x1F, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00], // 8
            &[0x66, 0x0F, 0x1F, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00], // 9
        ];

        let mut remaining = n;
        while remaining > 0 {
            let chunk = remaining.min(9);
            self.code.extend_from_slice(nops[chunk]);
            remaining -= chunk;
        }
    }

    /// UD2 (undefined instruction trap)
    pub fn ud2(&mut self) {
        self.emit_u8(0x0F);
        self.emit_u8(0x0B);
    }

    /// INT3 (debug breakpoint)
    pub fn int3(&mut self) {
        self.emit_u8(0xCC);
    }

    /// INT imm8
    pub fn int(&mut self, n: u8) {
        self.emit_u8(0xCD);
        self.emit_u8(n);
    }

    /// SYSCALL
    pub fn syscall(&mut self) {
        self.emit_u8(0x0F);
        self.emit_u8(0x05);
    }

    // =========================================================================
    // SSE/AVX Floating Point Instructions
    // =========================================================================

    /// MOVSD xmm, xmm (scalar double)
    pub fn movsd_rr(&mut self, dst: RegXmm, src: RegXmm) {
        self.emit_u8(0xF2);
        if dst.requires_rex() || src.requires_rex() {
            self.emit_rex(Rex {
                w: false,
                r: dst.requires_rex(),
                x: false,
                b: src.requires_rex(),
            });
        }
        self.emit_u8(0x0F);
        self.emit_u8(0x10);
        self.emit_u8(encode_modrm(0b11, dst.encoding(), src.encoding()));
    }

    /// MOVSD xmm, [mem]
    pub fn movsd_rm(&mut self, dst: RegXmm, src: &Mem) {
        self.emit_u8(0xF2);
        let rex = Rex {
            w: false,
            r: dst.requires_rex(),
            x: src.index.map_or(false, |i| i.requires_rex()),
            b: src.base.map_or(false, |b| b.requires_rex()),
        };
        self.emit_rex(rex);
        self.emit_u8(0x0F);
        self.emit_u8(0x10);
        // Convert RegXmm to Reg64 for modrm emission
        let reg = unsafe { std::mem::transmute::<u8, Reg64>(dst as u8) };
        self.emit_modrm_rm(reg, src);
    }

    /// MOVSD [mem], xmm
    pub fn movsd_mr(&mut self, dst: &Mem, src: RegXmm) {
        self.emit_u8(0xF2);
        let rex = Rex {
            w: false,
            r: src.requires_rex(),
            x: dst.index.map_or(false, |i| i.requires_rex()),
            b: dst.base.map_or(false, |b| b.requires_rex()),
        };
        self.emit_rex(rex);
        self.emit_u8(0x0F);
        self.emit_u8(0x11);
        let reg = unsafe { std::mem::transmute::<u8, Reg64>(src as u8) };
        self.emit_modrm_rm(reg, dst);
    }

    /// MOVSS xmm, xmm (scalar single)
    pub fn movss_rr(&mut self, dst: RegXmm, src: RegXmm) {
        self.emit_u8(0xF3);
        if dst.requires_rex() || src.requires_rex() {
            self.emit_rex(Rex {
                w: false,
                r: dst.requires_rex(),
                x: false,
                b: src.requires_rex(),
            });
        }
        self.emit_u8(0x0F);
        self.emit_u8(0x10);
        self.emit_u8(encode_modrm(0b11, dst.encoding(), src.encoding()));
    }

    /// ADDSD xmm, xmm
    pub fn addsd(&mut self, dst: RegXmm, src: RegXmm) {
        self.emit_u8(0xF2);
        if dst.requires_rex() || src.requires_rex() {
            self.emit_rex(Rex {
                w: false,
                r: dst.requires_rex(),
                x: false,
                b: src.requires_rex(),
            });
        }
        self.emit_u8(0x0F);
        self.emit_u8(0x58);
        self.emit_u8(encode_modrm(0b11, dst.encoding(), src.encoding()));
    }

    /// SUBSD xmm, xmm
    pub fn subsd(&mut self, dst: RegXmm, src: RegXmm) {
        self.emit_u8(0xF2);
        if dst.requires_rex() || src.requires_rex() {
            self.emit_rex(Rex {
                w: false,
                r: dst.requires_rex(),
                x: false,
                b: src.requires_rex(),
            });
        }
        self.emit_u8(0x0F);
        self.emit_u8(0x5C);
        self.emit_u8(encode_modrm(0b11, dst.encoding(), src.encoding()));
    }

    /// MULSD xmm, xmm
    pub fn mulsd(&mut self, dst: RegXmm, src: RegXmm) {
        self.emit_u8(0xF2);
        if dst.requires_rex() || src.requires_rex() {
            self.emit_rex(Rex {
                w: false,
                r: dst.requires_rex(),
                x: false,
                b: src.requires_rex(),
            });
        }
        self.emit_u8(0x0F);
        self.emit_u8(0x59);
        self.emit_u8(encode_modrm(0b11, dst.encoding(), src.encoding()));
    }

    /// DIVSD xmm, xmm
    pub fn divsd(&mut self, dst: RegXmm, src: RegXmm) {
        self.emit_u8(0xF2);
        if dst.requires_rex() || src.requires_rex() {
            self.emit_rex(Rex {
                w: false,
                r: dst.requires_rex(),
                x: false,
                b: src.requires_rex(),
            });
        }
        self.emit_u8(0x0F);
        self.emit_u8(0x5E);
        self.emit_u8(encode_modrm(0b11, dst.encoding(), src.encoding()));
    }

    /// ADDSS xmm, xmm
    pub fn addss(&mut self, dst: RegXmm, src: RegXmm) {
        self.emit_u8(0xF3);
        if dst.requires_rex() || src.requires_rex() {
            self.emit_rex(Rex {
                w: false,
                r: dst.requires_rex(),
                x: false,
                b: src.requires_rex(),
            });
        }
        self.emit_u8(0x0F);
        self.emit_u8(0x58);
        self.emit_u8(encode_modrm(0b11, dst.encoding(), src.encoding()));
    }

    /// SUBSS xmm, xmm
    pub fn subss(&mut self, dst: RegXmm, src: RegXmm) {
        self.emit_u8(0xF3);
        if dst.requires_rex() || src.requires_rex() {
            self.emit_rex(Rex {
                w: false,
                r: dst.requires_rex(),
                x: false,
                b: src.requires_rex(),
            });
        }
        self.emit_u8(0x0F);
        self.emit_u8(0x5C);
        self.emit_u8(encode_modrm(0b11, dst.encoding(), src.encoding()));
    }

    /// MULSS xmm, xmm
    pub fn mulss(&mut self, dst: RegXmm, src: RegXmm) {
        self.emit_u8(0xF3);
        if dst.requires_rex() || src.requires_rex() {
            self.emit_rex(Rex {
                w: false,
                r: dst.requires_rex(),
                x: false,
                b: src.requires_rex(),
            });
        }
        self.emit_u8(0x0F);
        self.emit_u8(0x59);
        self.emit_u8(encode_modrm(0b11, dst.encoding(), src.encoding()));
    }

    /// DIVSS xmm, xmm
    pub fn divss(&mut self, dst: RegXmm, src: RegXmm) {
        self.emit_u8(0xF3);
        if dst.requires_rex() || src.requires_rex() {
            self.emit_rex(Rex {
                w: false,
                r: dst.requires_rex(),
                x: false,
                b: src.requires_rex(),
            });
        }
        self.emit_u8(0x0F);
        self.emit_u8(0x5E);
        self.emit_u8(encode_modrm(0b11, dst.encoding(), src.encoding()));
    }

    /// UCOMISD xmm, xmm (unordered compare)
    pub fn ucomisd(&mut self, a: RegXmm, b: RegXmm) {
        self.emit_u8(0x66);
        if a.requires_rex() || b.requires_rex() {
            self.emit_rex(Rex {
                w: false,
                r: a.requires_rex(),
                x: false,
                b: b.requires_rex(),
            });
        }
        self.emit_u8(0x0F);
        self.emit_u8(0x2E);
        self.emit_u8(encode_modrm(0b11, a.encoding(), b.encoding()));
    }

    /// UCOMISS xmm, xmm
    pub fn ucomiss(&mut self, a: RegXmm, b: RegXmm) {
        if a.requires_rex() || b.requires_rex() {
            self.emit_rex(Rex {
                w: false,
                r: a.requires_rex(),
                x: false,
                b: b.requires_rex(),
            });
        }
        self.emit_u8(0x0F);
        self.emit_u8(0x2E);
        self.emit_u8(encode_modrm(0b11, a.encoding(), b.encoding()));
    }

    /// CVTSI2SD xmm, r64 (convert int to double)
    pub fn cvtsi2sd(&mut self, dst: RegXmm, src: Reg64) {
        self.emit_u8(0xF2);
        self.emit_rex(Rex {
            w: true,
            r: dst.requires_rex(),
            x: false,
            b: src.requires_rex(),
        });
        self.emit_u8(0x0F);
        self.emit_u8(0x2A);
        self.emit_u8(encode_modrm(0b11, dst.encoding(), src.encoding()));
    }

    /// CVTSD2SI r64, xmm (convert double to int)
    pub fn cvtsd2si(&mut self, dst: Reg64, src: RegXmm) {
        self.emit_u8(0xF2);
        self.emit_rex(Rex {
            w: true,
            r: dst.requires_rex(),
            x: false,
            b: src.requires_rex(),
        });
        self.emit_u8(0x0F);
        self.emit_u8(0x2D);
        self.emit_u8(encode_modrm(0b11, dst.encoding(), src.encoding()));
    }

    /// CVTTSD2SI r64, xmm (convert double to int with truncation)
    pub fn cvttsd2si(&mut self, dst: Reg64, src: RegXmm) {
        self.emit_u8(0xF2);
        self.emit_rex(Rex {
            w: true,
            r: dst.requires_rex(),
            x: false,
            b: src.requires_rex(),
        });
        self.emit_u8(0x0F);
        self.emit_u8(0x2C);
        self.emit_u8(encode_modrm(0b11, dst.encoding(), src.encoding()));
    }

    /// CVTSS2SD xmm, xmm (convert single to double)
    pub fn cvtss2sd(&mut self, dst: RegXmm, src: RegXmm) {
        self.emit_u8(0xF3);
        if dst.requires_rex() || src.requires_rex() {
            self.emit_rex(Rex {
                w: false,
                r: dst.requires_rex(),
                x: false,
                b: src.requires_rex(),
            });
        }
        self.emit_u8(0x0F);
        self.emit_u8(0x5A);
        self.emit_u8(encode_modrm(0b11, dst.encoding(), src.encoding()));
    }

    /// CVTSD2SS xmm, xmm (convert double to single)
    pub fn cvtsd2ss(&mut self, dst: RegXmm, src: RegXmm) {
        self.emit_u8(0xF2);
        if dst.requires_rex() || src.requires_rex() {
            self.emit_rex(Rex {
                w: false,
                r: dst.requires_rex(),
                x: false,
                b: src.requires_rex(),
            });
        }
        self.emit_u8(0x0F);
        self.emit_u8(0x5A);
        self.emit_u8(encode_modrm(0b11, dst.encoding(), src.encoding()));
    }

    /// XORPS xmm, xmm (zero XMM register)
    pub fn xorps(&mut self, dst: RegXmm, src: RegXmm) {
        if dst.requires_rex() || src.requires_rex() {
            self.emit_rex(Rex {
                w: false,
                r: dst.requires_rex(),
                x: false,
                b: src.requires_rex(),
            });
        }
        self.emit_u8(0x0F);
        self.emit_u8(0x57);
        self.emit_u8(encode_modrm(0b11, dst.encoding(), src.encoding()));
    }

    // =========================================================================
    // Label Fixup
    // =========================================================================

    /// Fix up all forward label references.
    pub fn fixup_labels(&mut self) {
        for label_ref in &self.label_refs {
            if let Some(&target) = self.labels.get(&label_ref.label) {
                let offset = (target as i64) - (label_ref.offset as i64 + label_ref.size as i64);
                match label_ref.size {
                    1 => {
                        self.code[label_ref.offset] = offset as u8;
                    }
                    4 => {
                        let bytes = (offset as i32).to_le_bytes();
                        self.code[label_ref.offset..label_ref.offset + 4].copy_from_slice(&bytes);
                    }
                    _ => panic!("Unsupported label reference size"),
                }
            } else {
                panic!("Undefined label: {}", label_ref.label);
            }
        }
        self.label_refs.clear();
    }

    /// Align code to given boundary with NOPs.
    pub fn align(&mut self, alignment: usize) {
        let pos = self.code.len();
        let padding = (alignment - (pos % alignment)) % alignment;
        self.nop_n(padding);
    }

    // =========================================================================
    // Label convenience aliases
    // =========================================================================

    /// Create a new label (alias for new_label).
    pub fn create_label(&mut self) -> u32 {
        self.new_label()
    }

    /// Bind a label to current position (alias for define_label).
    pub fn bind_label(&mut self, label: u32) {
        self.define_label(label);
    }

    // =========================================================================
    // Instruction convenience aliases
    // =========================================================================

    /// MOV register, immediate (64-bit)
    pub fn mov_ri(&mut self, dst: Reg64, imm: i64) {
        self.mov_ri64(dst, imm);
    }

    /// TEST register, immediate (32-bit)
    pub fn test_ri(&mut self, dst: Reg64, imm: i32) {
        self.test_ri32(dst, imm);
    }

    /// SHR register, immediate (8-bit shift amount)
    pub fn shr_ri(&mut self, dst: Reg64, imm: u8) {
        self.shr_imm(dst, imm);
    }

    /// Get the generated code.
    pub fn finish(mut self) -> Vec<u8> {
        self.fixup_labels();
        self.code
    }
}

impl Default for X86_64Encoder {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mov_rr() {
        let mut enc = X86_64Encoder::new();
        enc.mov_rr(Reg64::RAX, Reg64::RCX);
        assert_eq!(enc.code, vec![0x48, 0x89, 0xC8]); // REX.W MOV rax, rcx
    }

    #[test]
    fn test_mov_ri64() {
        let mut enc = X86_64Encoder::new();
        enc.mov_ri64(Reg64::RAX, 0x123456789ABCDEF0i64);
        assert_eq!(enc.code.len(), 10); // REX.W + opcode + 8-byte imm
        assert_eq!(enc.code[0], 0x48); // REX.W
        assert_eq!(enc.code[1], 0xB8); // MOV rax, imm64
    }

    #[test]
    fn test_mov_r8() {
        let mut enc = X86_64Encoder::new();
        enc.mov_rr(Reg64::R8, Reg64::R9);
        // REX.WRB (0x4D) MOV r8, r9
        assert_eq!(enc.code, vec![0x4D, 0x89, 0xC8]);
    }

    #[test]
    fn test_push_pop() {
        let mut enc = X86_64Encoder::new();
        enc.push(Reg64::RAX);
        enc.push(Reg64::R15);
        enc.pop(Reg64::R15);
        enc.pop(Reg64::RAX);
        assert_eq!(enc.code, vec![0x50, 0x41, 0x57, 0x41, 0x5F, 0x58]);
    }

    #[test]
    fn test_add_rr() {
        let mut enc = X86_64Encoder::new();
        enc.add_rr(Reg64::RAX, Reg64::RCX);
        assert_eq!(enc.code, vec![0x48, 0x01, 0xC8]); // REX.W ADD rax, rcx
    }

    #[test]
    fn test_add_ri32_small() {
        let mut enc = X86_64Encoder::new();
        enc.add_ri32(Reg64::RAX, 10);
        // REX.W ADD rax, imm8 (sign-extended)
        assert_eq!(enc.code, vec![0x48, 0x83, 0xC0, 0x0A]);
    }

    #[test]
    fn test_add_ri32_large() {
        let mut enc = X86_64Encoder::new();
        enc.add_ri32(Reg64::RAX, 0x12345678);
        // REX.W ADD rax, imm32
        assert_eq!(enc.code, vec![0x48, 0x05, 0x78, 0x56, 0x34, 0x12]);
    }

    #[test]
    fn test_sub_rr() {
        let mut enc = X86_64Encoder::new();
        enc.sub_rr(Reg64::RBX, Reg64::RDX);
        assert_eq!(enc.code, vec![0x48, 0x29, 0xD3]);
    }

    #[test]
    fn test_imul_rr() {
        let mut enc = X86_64Encoder::new();
        enc.imul_rr(Reg64::RAX, Reg64::RCX);
        assert_eq!(enc.code, vec![0x48, 0x0F, 0xAF, 0xC1]);
    }

    #[test]
    fn test_idiv() {
        let mut enc = X86_64Encoder::new();
        enc.cqo();
        enc.idiv(Reg64::RCX);
        assert_eq!(enc.code, vec![0x48, 0x99, 0x48, 0xF7, 0xF9]);
    }

    #[test]
    fn test_neg() {
        let mut enc = X86_64Encoder::new();
        enc.neg(Reg64::RAX);
        assert_eq!(enc.code, vec![0x48, 0xF7, 0xD8]);
    }

    #[test]
    fn test_and_rr() {
        let mut enc = X86_64Encoder::new();
        enc.and_rr(Reg64::RAX, Reg64::RCX);
        assert_eq!(enc.code, vec![0x48, 0x21, 0xC8]);
    }

    #[test]
    fn test_or_rr() {
        let mut enc = X86_64Encoder::new();
        enc.or_rr(Reg64::RAX, Reg64::RCX);
        assert_eq!(enc.code, vec![0x48, 0x09, 0xC8]);
    }

    #[test]
    fn test_xor_rr() {
        let mut enc = X86_64Encoder::new();
        enc.xor_rr(Reg64::RAX, Reg64::RAX); // Zero RAX
        assert_eq!(enc.code, vec![0x48, 0x31, 0xC0]);
    }

    #[test]
    fn test_shl_imm() {
        let mut enc = X86_64Encoder::new();
        enc.shl_imm(Reg64::RAX, 4);
        assert_eq!(enc.code, vec![0x48, 0xC1, 0xE0, 0x04]);
    }

    #[test]
    fn test_shr_imm() {
        let mut enc = X86_64Encoder::new();
        enc.shr_imm(Reg64::RAX, 1);
        assert_eq!(enc.code, vec![0x48, 0xD1, 0xE8]); // SHR by 1 uses short form
    }

    #[test]
    fn test_cmp_rr() {
        let mut enc = X86_64Encoder::new();
        enc.cmp_rr(Reg64::RAX, Reg64::RCX);
        assert_eq!(enc.code, vec![0x48, 0x39, 0xC8]);
    }

    #[test]
    fn test_test_rr() {
        let mut enc = X86_64Encoder::new();
        enc.test_rr(Reg64::RAX, Reg64::RAX);
        assert_eq!(enc.code, vec![0x48, 0x85, 0xC0]);
    }

    #[test]
    fn test_setcc() {
        let mut enc = X86_64Encoder::new();
        enc.setcc(Cond::E, Reg8::AL);
        assert_eq!(enc.code, vec![0x0F, 0x94, 0xC0]);
    }

    #[test]
    fn test_jmp_rel32() {
        let mut enc = X86_64Encoder::new();
        enc.jmp_rel32(0x12345678);
        assert_eq!(enc.code, vec![0xE9, 0x78, 0x56, 0x34, 0x12]);
    }

    #[test]
    fn test_jcc_rel32() {
        let mut enc = X86_64Encoder::new();
        enc.jcc_rel32(Cond::E, 0x100);
        assert_eq!(enc.code, vec![0x0F, 0x84, 0x00, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn test_call_ret() {
        let mut enc = X86_64Encoder::new();
        enc.call_rel32(0);
        enc.ret();
        assert_eq!(enc.code, vec![0xE8, 0x00, 0x00, 0x00, 0x00, 0xC3]);
    }

    #[test]
    fn test_nop() {
        let mut enc = X86_64Encoder::new();
        enc.nop();
        enc.nop_n(5);
        assert_eq!(enc.code[0], 0x90); // Single NOP
        assert_eq!(enc.code.len(), 6); // 1 + 5
    }

    #[test]
    fn test_ud2() {
        let mut enc = X86_64Encoder::new();
        enc.ud2();
        assert_eq!(enc.code, vec![0x0F, 0x0B]);
    }

    #[test]
    fn test_syscall() {
        let mut enc = X86_64Encoder::new();
        enc.syscall();
        assert_eq!(enc.code, vec![0x0F, 0x05]);
    }

    #[test]
    fn test_labels() {
        let mut enc = X86_64Encoder::new();
        let label = enc.new_label();
        enc.jmp_label(label);
        enc.nop();
        enc.nop();
        enc.define_label(label);
        enc.nop();
        enc.fixup_labels();

        // JMP should jump forward 2 bytes (2 NOPs)
        let offset = enc.code[1..5].to_vec();
        assert_eq!(offset, vec![0x02, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn test_mov_rm() {
        let mut enc = X86_64Encoder::new();
        let mem = Mem::base_disp(Reg64::RBP, -8);
        enc.mov_rm(Reg64::RAX, &mem);
        // REX.W MOV rax, [rbp-8]
        assert_eq!(enc.code, vec![0x48, 0x8B, 0x45, 0xF8]);
    }

    #[test]
    fn test_mov_mr() {
        let mut enc = X86_64Encoder::new();
        let mem = Mem::base_disp(Reg64::RBP, -16);
        enc.mov_mr(&mem, Reg64::RCX);
        // REX.W MOV [rbp-16], rcx
        assert_eq!(enc.code, vec![0x48, 0x89, 0x4D, 0xF0]);
    }

    #[test]
    fn test_lea() {
        let mut enc = X86_64Encoder::new();
        let mem = Mem::base_index_scale_disp(Reg64::RBX, Reg64::RCX, Scale::X4, 8);
        enc.lea(Reg64::RAX, &mem);
        // REX.W LEA rax, [rbx+rcx*4+8]
        assert!(enc.code.len() > 0);
    }

    #[test]
    fn test_movsd_rr() {
        let mut enc = X86_64Encoder::new();
        enc.movsd_rr(RegXmm::XMM0, RegXmm::XMM1);
        assert_eq!(enc.code, vec![0xF2, 0x0F, 0x10, 0xC1]);
    }

    #[test]
    fn test_addsd() {
        let mut enc = X86_64Encoder::new();
        enc.addsd(RegXmm::XMM0, RegXmm::XMM1);
        assert_eq!(enc.code, vec![0xF2, 0x0F, 0x58, 0xC1]);
    }

    #[test]
    fn test_cvtsi2sd() {
        let mut enc = X86_64Encoder::new();
        enc.cvtsi2sd(RegXmm::XMM0, Reg64::RAX);
        assert_eq!(enc.code, vec![0xF2, 0x48, 0x0F, 0x2A, 0xC0]);
    }

    #[test]
    fn test_condition_invert() {
        assert_eq!(Cond::E.invert(), Cond::NE);
        assert_eq!(Cond::L.invert(), Cond::GE);
        assert_eq!(Cond::G.invert(), Cond::LE);
    }

    #[test]
    fn test_rex_encoding() {
        let rex = Rex {
            w: true,
            r: true,
            x: false,
            b: true,
        };
        assert_eq!(rex.encode(), 0x4D); // 0100_1101
    }

    #[test]
    fn test_modrm_encoding() {
        assert_eq!(encode_modrm(0b11, 0, 1), 0xC1); // mod=11, reg=0, r/m=1
        assert_eq!(encode_modrm(0b00, 4, 5), 0x25); // mod=00, reg=4, r/m=5
    }

    #[test]
    fn test_sib_encoding() {
        // SIB byte: scale (bits 7-6) | index (bits 5-3) | base (bits 2-0)
        // scale=2 (×4), index=1, base=3 => 10_001_011 = 0x8B
        assert_eq!(encode_sib(2, 1, 3), 0x8B);
    }

    #[test]
    fn test_prologue_epilogue() {
        let mut enc = X86_64Encoder::new();
        // Standard function prologue
        enc.push(Reg64::RBP);
        enc.mov_rr(Reg64::RBP, Reg64::RSP);
        enc.sub_ri32(Reg64::RSP, 32);

        // Standard function epilogue
        enc.mov_rr(Reg64::RSP, Reg64::RBP);
        enc.pop(Reg64::RBP);
        enc.ret();

        // Verify we generated something reasonable
        assert!(enc.code.len() > 10);
        assert_eq!(*enc.code.last().unwrap(), 0xC3); // RET
    }

    #[test]
    fn test_align() {
        let mut enc = X86_64Encoder::new();
        enc.nop();
        enc.align(16);
        assert_eq!(enc.code.len() % 16, 0);
    }
}
