// ===============================================================================
// QUANTALANG CODE GENERATOR - ARM64 INSTRUCTION ENCODER
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! ARM64/AArch64 machine code instruction encoder.
//!
//! This module provides direct encoding of ARM64 instructions to binary
//! machine code, bypassing the need for an external assembler.
//!
//! ## Instruction Format
//!
//! ARM64 uses fixed-width 32-bit instructions with several encoding formats:
//! - Data Processing (Immediate)
//! - Data Processing (Register)
//! - Loads and Stores
//! - Branches
//! - SIMD and Floating-Point
//!
//! ## Supported Features
//!
//! - General purpose registers (X0-X30, SP, XZR)
//! - SIMD/FP registers (V0-V31, D0-D31, S0-S31)
//! - All addressing modes (immediate, register, pre/post-indexed)
//! - Conditional branches and comparisons
//! - System instructions

use std::collections::HashMap;

// =============================================================================
// Registers
// =============================================================================

/// ARM64 64-bit general purpose register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Reg64 {
    X0 = 0,
    X1 = 1,
    X2 = 2,
    X3 = 3,
    X4 = 4,
    X5 = 5,
    X6 = 6,
    X7 = 7,
    X8 = 8,
    X9 = 9,
    X10 = 10,
    X11 = 11,
    X12 = 12,
    X13 = 13,
    X14 = 14,
    X15 = 15,
    X16 = 16,
    X17 = 17,
    X18 = 18,
    X19 = 19,
    X20 = 20,
    X21 = 21,
    X22 = 22,
    X23 = 23,
    X24 = 24,
    X25 = 25,
    X26 = 26,
    X27 = 27,
    X28 = 28,
    /// Frame pointer (X29).
    FP = 29,
    /// Link register (X30).
    LR = 30,
    /// Stack pointer (encoded as 31 in some contexts).
    SP = 31,
}

impl Reg64 {
    /// Get the 5-bit register encoding.
    pub fn encoding(self) -> u32 {
        self as u32
    }

    /// Get the 32-bit version of this register.
    pub fn as_32(self) -> Reg32 {
        unsafe { std::mem::transmute(self as u8) }
    }

    /// Check if this is the stack pointer.
    pub fn is_sp(self) -> bool {
        matches!(self, Reg64::SP)
    }

    /// Check if this is the zero register (XZR, same encoding as SP).
    pub fn is_zr(self) -> bool {
        matches!(self, Reg64::SP)
    }
}

/// ARM64 32-bit general purpose register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Reg32 {
    W0 = 0,
    W1 = 1,
    W2 = 2,
    W3 = 3,
    W4 = 4,
    W5 = 5,
    W6 = 6,
    W7 = 7,
    W8 = 8,
    W9 = 9,
    W10 = 10,
    W11 = 11,
    W12 = 12,
    W13 = 13,
    W14 = 14,
    W15 = 15,
    W16 = 16,
    W17 = 17,
    W18 = 18,
    W19 = 19,
    W20 = 20,
    W21 = 21,
    W22 = 22,
    W23 = 23,
    W24 = 24,
    W25 = 25,
    W26 = 26,
    W27 = 27,
    W28 = 28,
    W29 = 29,
    W30 = 30,
    /// Zero register (WZR).
    WZR = 31,
}

impl Reg32 {
    pub fn encoding(self) -> u32 {
        self as u32
    }
}

/// ARM64 SIMD/FP 128-bit vector register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RegV {
    V0 = 0,
    V1 = 1,
    V2 = 2,
    V3 = 3,
    V4 = 4,
    V5 = 5,
    V6 = 6,
    V7 = 7,
    V8 = 8,
    V9 = 9,
    V10 = 10,
    V11 = 11,
    V12 = 12,
    V13 = 13,
    V14 = 14,
    V15 = 15,
    V16 = 16,
    V17 = 17,
    V18 = 18,
    V19 = 19,
    V20 = 20,
    V21 = 21,
    V22 = 22,
    V23 = 23,
    V24 = 24,
    V25 = 25,
    V26 = 26,
    V27 = 27,
    V28 = 28,
    V29 = 29,
    V30 = 30,
    V31 = 31,
}

impl RegV {
    pub fn encoding(self) -> u32 {
        self as u32
    }

    /// Get as 64-bit double register.
    pub fn as_d(self) -> RegD {
        unsafe { std::mem::transmute(self as u8) }
    }

    /// Get as 32-bit single register.
    pub fn as_s(self) -> RegS {
        unsafe { std::mem::transmute(self as u8) }
    }
}

/// ARM64 64-bit double-precision FP register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RegD {
    D0 = 0,
    D1 = 1,
    D2 = 2,
    D3 = 3,
    D4 = 4,
    D5 = 5,
    D6 = 6,
    D7 = 7,
    D8 = 8,
    D9 = 9,
    D10 = 10,
    D11 = 11,
    D12 = 12,
    D13 = 13,
    D14 = 14,
    D15 = 15,
    D16 = 16,
    D17 = 17,
    D18 = 18,
    D19 = 19,
    D20 = 20,
    D21 = 21,
    D22 = 22,
    D23 = 23,
    D24 = 24,
    D25 = 25,
    D26 = 26,
    D27 = 27,
    D28 = 28,
    D29 = 29,
    D30 = 30,
    D31 = 31,
}

impl RegD {
    pub fn encoding(self) -> u32 {
        self as u32
    }
}

/// ARM64 32-bit single-precision FP register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RegS {
    S0 = 0,
    S1 = 1,
    S2 = 2,
    S3 = 3,
    S4 = 4,
    S5 = 5,
    S6 = 6,
    S7 = 7,
    S8 = 8,
    S9 = 9,
    S10 = 10,
    S11 = 11,
    S12 = 12,
    S13 = 13,
    S14 = 14,
    S15 = 15,
    S16 = 16,
    S17 = 17,
    S18 = 18,
    S19 = 19,
    S20 = 20,
    S21 = 21,
    S22 = 22,
    S23 = 23,
    S24 = 24,
    S25 = 25,
    S26 = 26,
    S27 = 27,
    S28 = 28,
    S29 = 29,
    S30 = 30,
    S31 = 31,
}

impl RegS {
    pub fn encoding(self) -> u32 {
        self as u32
    }
}

// =============================================================================
// Condition Codes
// =============================================================================

/// ARM64 condition codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Cond {
    /// Equal (Z=1).
    EQ = 0b0000,
    /// Not equal (Z=0).
    NE = 0b0001,
    /// Carry set / unsigned higher or same (C=1).
    CS = 0b0010,
    /// Carry clear / unsigned lower (C=0).
    CC = 0b0011,
    /// Minus / negative (N=1).
    MI = 0b0100,
    /// Plus / positive or zero (N=0).
    PL = 0b0101,
    /// Overflow (V=1).
    VS = 0b0110,
    /// No overflow (V=0).
    VC = 0b0111,
    /// Unsigned higher (C=1 and Z=0).
    HI = 0b1000,
    /// Unsigned lower or same (C=0 or Z=1).
    LS = 0b1001,
    /// Signed greater or equal (N=V).
    GE = 0b1010,
    /// Signed less than (N≠V).
    LT = 0b1011,
    /// Signed greater than (Z=0 and N=V).
    GT = 0b1100,
    /// Signed less or equal (Z=1 or N≠V).
    LE = 0b1101,
    /// Always (unconditional).
    AL = 0b1110,
    /// Never (reserved).
    NV = 0b1111,
}

impl Cond {
    pub fn encoding(self) -> u32 {
        self as u32
    }

    /// Invert the condition.
    pub fn invert(self) -> Self {
        unsafe { std::mem::transmute((self as u8) ^ 1) }
    }
}

// Aliases for common conditions
impl Cond {
    /// Alias for CS (unsigned >=).
    pub const HS: Cond = Cond::CS;
    /// Alias for CC (unsigned <).
    pub const LO: Cond = Cond::CC;
}

// =============================================================================
// Shift Types
// =============================================================================

/// Shift type for data processing operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Shift {
    /// Logical shift left.
    LSL = 0b00,
    /// Logical shift right.
    LSR = 0b01,
    /// Arithmetic shift right.
    ASR = 0b10,
    /// Rotate right.
    ROR = 0b11,
}

impl Shift {
    pub fn encoding(self) -> u32 {
        self as u32
    }
}

/// Extend type for addressing and arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Extend {
    /// Unsigned extend byte.
    UXTB = 0b000,
    /// Unsigned extend halfword.
    UXTH = 0b001,
    /// Unsigned extend word (or LSL for 32-bit).
    UXTW = 0b010,
    /// Unsigned extend doubleword (or LSL for 64-bit).
    UXTX = 0b011,
    /// Signed extend byte.
    SXTB = 0b100,
    /// Signed extend halfword.
    SXTH = 0b101,
    /// Signed extend word.
    SXTW = 0b110,
    /// Signed extend doubleword.
    SXTX = 0b111,
}

impl Extend {
    pub fn encoding(self) -> u32 {
        self as u32
    }
}

// =============================================================================
// Memory Operands
// =============================================================================

/// Memory addressing mode.
#[derive(Debug, Clone, Copy)]
pub enum AddrMode {
    /// Base register only: [Xn]
    Base(Reg64),
    /// Base + unsigned offset: [Xn, #imm]
    BaseOffset(Reg64, i32),
    /// Base + register: [Xn, Xm]
    BaseReg(Reg64, Reg64),
    /// Base + extended register: [Xn, Wm, extend #amount]
    BaseRegExt(Reg64, Reg64, Extend, u8),
    /// Pre-indexed: [Xn, #imm]!
    PreIndex(Reg64, i32),
    /// Post-indexed: [Xn], #imm
    PostIndex(Reg64, i32),
    /// Literal (PC-relative): label
    Literal(i32),
}

impl AddrMode {
    /// Create base-only addressing.
    pub fn base(reg: Reg64) -> Self {
        AddrMode::Base(reg)
    }

    /// Create base + offset addressing.
    pub fn offset(reg: Reg64, offset: i32) -> Self {
        AddrMode::BaseOffset(reg, offset)
    }

    /// Create pre-indexed addressing.
    pub fn pre_index(reg: Reg64, offset: i32) -> Self {
        AddrMode::PreIndex(reg, offset)
    }

    /// Create post-indexed addressing.
    pub fn post_index(reg: Reg64, offset: i32) -> Self {
        AddrMode::PostIndex(reg, offset)
    }
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
    /// 26-bit PC-relative branch.
    Branch26,
    /// 19-bit PC-relative conditional branch.
    Branch19,
    /// 21-bit PC-relative ADR.
    Adr21,
    /// 21-bit page-relative ADRP.
    AdrpPage21,
    /// 12-bit ADD/LDR offset.
    AddLo12,
    /// 64-bit absolute.
    Abs64,
    /// 32-bit absolute.
    Abs32,
    /// GOT entry.
    GotPage21,
    /// GOT offset.
    GotLo12,
    /// TLS descriptor call.
    TlsDesc,
}

/// Label reference for forward branches.
#[derive(Debug, Clone)]
pub struct LabelRef {
    /// Offset in code where the label is referenced.
    pub offset: usize,
    /// Label ID.
    pub label: u32,
    /// Branch type (affects encoding).
    pub kind: BranchKind,
}

/// Type of branch for label fixup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchKind {
    /// Unconditional branch (B, BL) - 26-bit offset.
    Unconditional,
    /// Conditional branch (B.cond) - 19-bit offset.
    Conditional,
    /// Compare and branch (CBZ, CBNZ) - 19-bit offset.
    Compare,
    /// Test and branch (TBZ, TBNZ) - 14-bit offset.
    Test,
    /// ADR - 21-bit offset.
    Adr,
}

/// ARM64 instruction encoder.
pub struct Arm64Encoder {
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

impl Arm64Encoder {
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

    /// Emit a 32-bit instruction.
    pub fn emit(&mut self, insn: u32) {
        self.code.extend_from_slice(&insn.to_le_bytes());
    }

    // =========================================================================
    // Data Processing (Immediate)
    // =========================================================================

    /// ADD Xd, Xn, #imm (64-bit)
    pub fn add_imm(&mut self, dst: Reg64, src: Reg64, imm: u16) {
        let insn =
            0x91000000 | ((imm as u32 & 0xFFF) << 10) | (src.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// ADD Wd, Wn, #imm (32-bit)
    pub fn add_imm32(&mut self, dst: Reg32, src: Reg32, imm: u16) {
        let insn =
            0x11000000 | ((imm as u32 & 0xFFF) << 10) | (src.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// ADDS Xd, Xn, #imm (64-bit, sets flags)
    pub fn adds_imm(&mut self, dst: Reg64, src: Reg64, imm: u16) {
        let insn =
            0xB1000000 | ((imm as u32 & 0xFFF) << 10) | (src.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// SUB Xd, Xn, #imm (64-bit)
    pub fn sub_imm(&mut self, dst: Reg64, src: Reg64, imm: u16) {
        let insn =
            0xD1000000 | ((imm as u32 & 0xFFF) << 10) | (src.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// SUB Wd, Wn, #imm (32-bit)
    pub fn sub_imm32(&mut self, dst: Reg32, src: Reg32, imm: u16) {
        let insn =
            0x51000000 | ((imm as u32 & 0xFFF) << 10) | (src.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// SUBS Xd, Xn, #imm (64-bit, sets flags)
    pub fn subs_imm(&mut self, dst: Reg64, src: Reg64, imm: u16) {
        let insn =
            0xF1000000 | ((imm as u32 & 0xFFF) << 10) | (src.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// CMP Xn, #imm (alias for SUBS XZR, Xn, #imm)
    pub fn cmp_imm(&mut self, src: Reg64, imm: u16) {
        self.subs_imm(Reg64::SP, src, imm); // XZR encoded as SP in dest
    }

    /// CMN Xn, #imm (alias for ADDS XZR, Xn, #imm)
    pub fn cmn_imm(&mut self, src: Reg64, imm: u16) {
        self.adds_imm(Reg64::SP, src, imm);
    }

    /// AND Xd, Xn, #imm (64-bit)
    pub fn and_imm(&mut self, dst: Reg64, src: Reg64, imm: u64) {
        if let Some((n, immr, imms)) = encode_bitmask_imm(imm, true) {
            let insn = 0x92000000
                | (n << 22)
                | (immr << 16)
                | (imms << 10)
                | (src.encoding() << 5)
                | dst.encoding();
            self.emit(insn);
        } else {
            panic!("Cannot encode immediate {} as bitmask", imm);
        }
    }

    /// ORR Xd, Xn, #imm (64-bit)
    pub fn orr_imm(&mut self, dst: Reg64, src: Reg64, imm: u64) {
        if let Some((n, immr, imms)) = encode_bitmask_imm(imm, true) {
            let insn = 0xB2000000
                | (n << 22)
                | (immr << 16)
                | (imms << 10)
                | (src.encoding() << 5)
                | dst.encoding();
            self.emit(insn);
        } else {
            panic!("Cannot encode immediate {} as bitmask", imm);
        }
    }

    /// EOR Xd, Xn, #imm (64-bit)
    pub fn eor_imm(&mut self, dst: Reg64, src: Reg64, imm: u64) {
        if let Some((n, immr, imms)) = encode_bitmask_imm(imm, true) {
            let insn = 0xD2000000
                | (n << 22)
                | (immr << 16)
                | (imms << 10)
                | (src.encoding() << 5)
                | dst.encoding();
            self.emit(insn);
        } else {
            panic!("Cannot encode immediate {} as bitmask", imm);
        }
    }

    /// MOVZ Xd, #imm, LSL #shift (64-bit)
    pub fn movz(&mut self, dst: Reg64, imm: u16, shift: u8) {
        let hw = (shift / 16) as u32;
        let insn = 0xD2800000 | (hw << 21) | ((imm as u32) << 5) | dst.encoding();
        self.emit(insn);
    }

    /// MOVZ Wd, #imm, LSL #shift (32-bit)
    pub fn movz32(&mut self, dst: Reg32, imm: u16, shift: u8) {
        let hw = (shift / 16) as u32;
        let insn = 0x52800000 | (hw << 21) | ((imm as u32) << 5) | dst.encoding();
        self.emit(insn);
    }

    /// MOVN Xd, #imm, LSL #shift (64-bit, move wide NOT)
    pub fn movn(&mut self, dst: Reg64, imm: u16, shift: u8) {
        let hw = (shift / 16) as u32;
        let insn = 0x92800000 | (hw << 21) | ((imm as u32) << 5) | dst.encoding();
        self.emit(insn);
    }

    /// MOVK Xd, #imm, LSL #shift (64-bit, move keep)
    pub fn movk(&mut self, dst: Reg64, imm: u16, shift: u8) {
        let hw = (shift / 16) as u32;
        let insn = 0xF2800000 | (hw << 21) | ((imm as u32) << 5) | dst.encoding();
        self.emit(insn);
    }

    /// Load a 64-bit immediate into a register.
    pub fn mov_imm64(&mut self, dst: Reg64, imm: u64) {
        // Check if we can use MOVZ/MOVN alone
        let neg = !imm;

        // Count non-zero 16-bit chunks
        let chunks = [
            (imm & 0xFFFF) as u16,
            ((imm >> 16) & 0xFFFF) as u16,
            ((imm >> 32) & 0xFFFF) as u16,
            ((imm >> 48) & 0xFFFF) as u16,
        ];

        let neg_chunks = [
            (neg & 0xFFFF) as u16,
            ((neg >> 16) & 0xFFFF) as u16,
            ((neg >> 32) & 0xFFFF) as u16,
            ((neg >> 48) & 0xFFFF) as u16,
        ];

        let zero_count = chunks.iter().filter(|&&c| c == 0).count();
        let neg_zero_count = neg_chunks.iter().filter(|&&c| c == 0).count();

        if zero_count >= neg_zero_count {
            // Use MOVZ + MOVK sequence
            let mut first = true;
            for (i, &chunk) in chunks.iter().enumerate() {
                if chunk != 0 || first && i == 3 {
                    if first {
                        self.movz(dst, chunk, (i * 16) as u8);
                        first = false;
                    } else {
                        self.movk(dst, chunk, (i * 16) as u8);
                    }
                }
            }
            if first {
                self.movz(dst, 0, 0);
            }
        } else {
            // Use MOVN + MOVK sequence
            let mut first = true;
            for (i, &chunk) in neg_chunks.iter().enumerate() {
                if chunk != 0xFFFF || first && i == 3 {
                    if first {
                        self.movn(dst, !chunks[i], (i * 16) as u8);
                        first = false;
                    } else if chunks[i] != 0xFFFF {
                        self.movk(dst, chunks[i], (i * 16) as u8);
                    }
                }
            }
        }
    }

    /// MOV Xd, Xn (alias for ORR Xd, XZR, Xn)
    pub fn mov(&mut self, dst: Reg64, src: Reg64) {
        self.orr_reg(dst, Reg64::SP, src); // XZR encoded as SP
    }

    /// MOV Xd, #imm (convenience wrapper)
    pub fn mov_imm(&mut self, dst: Reg64, imm: i64) {
        self.mov_imm64(dst, imm as u64);
    }

    // =========================================================================
    // Data Processing (Register)
    // =========================================================================

    /// ADD Xd, Xn, Xm (64-bit)
    pub fn add_reg(&mut self, dst: Reg64, src1: Reg64, src2: Reg64) {
        let insn = 0x8B000000 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// ADD Xd, Xn, Xm, shift #amount (64-bit)
    pub fn add_reg_shift(
        &mut self,
        dst: Reg64,
        src1: Reg64,
        src2: Reg64,
        shift: Shift,
        amount: u8,
    ) {
        let insn = 0x8B000000
            | (shift.encoding() << 22)
            | (src2.encoding() << 16)
            | ((amount as u32 & 0x3F) << 10)
            | (src1.encoding() << 5)
            | dst.encoding();
        self.emit(insn);
    }

    /// ADDS Xd, Xn, Xm (64-bit, sets flags)
    pub fn adds_reg(&mut self, dst: Reg64, src1: Reg64, src2: Reg64) {
        let insn = 0xAB000000 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// SUB Xd, Xn, Xm (64-bit)
    pub fn sub_reg(&mut self, dst: Reg64, src1: Reg64, src2: Reg64) {
        let insn = 0xCB000000 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// SUB Xd, Xn, Xm, shift #amount (64-bit)
    pub fn sub_reg_shift(
        &mut self,
        dst: Reg64,
        src1: Reg64,
        src2: Reg64,
        shift: Shift,
        amount: u8,
    ) {
        let insn = 0xCB000000
            | (shift.encoding() << 22)
            | (src2.encoding() << 16)
            | ((amount as u32 & 0x3F) << 10)
            | (src1.encoding() << 5)
            | dst.encoding();
        self.emit(insn);
    }

    /// SUBS Xd, Xn, Xm (64-bit, sets flags)
    pub fn subs_reg(&mut self, dst: Reg64, src1: Reg64, src2: Reg64) {
        let insn = 0xEB000000 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// CMP Xn, Xm (alias for SUBS XZR, Xn, Xm)
    pub fn cmp_reg(&mut self, src1: Reg64, src2: Reg64) {
        self.subs_reg(Reg64::SP, src1, src2);
    }

    /// NEG Xd, Xm (alias for SUB Xd, XZR, Xm)
    pub fn neg(&mut self, dst: Reg64, src: Reg64) {
        self.sub_reg(dst, Reg64::SP, src);
    }

    /// NEGS Xd, Xm (alias for SUBS Xd, XZR, Xm)
    pub fn negs(&mut self, dst: Reg64, src: Reg64) {
        self.subs_reg(dst, Reg64::SP, src);
    }

    /// AND Xd, Xn, Xm (64-bit)
    pub fn and_reg(&mut self, dst: Reg64, src1: Reg64, src2: Reg64) {
        let insn = 0x8A000000 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// ANDS Xd, Xn, Xm (64-bit, sets flags)
    pub fn ands_reg(&mut self, dst: Reg64, src1: Reg64, src2: Reg64) {
        let insn = 0xEA000000 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// ANDS Xd, Xn, #imm (64-bit with bitmask immediate, sets flags)
    pub fn ands_imm(&mut self, dst: Reg64, src: Reg64, imm: u64) {
        if let Some((n, immr, imms)) = encode_bitmask_imm(imm, true) {
            let insn = 0xF2000000
                | (n << 22)
                | (immr << 16)
                | (imms << 10)
                | (src.encoding() << 5)
                | dst.encoding();
            self.emit(insn);
        } else {
            // Fallback: load immediate into temp register and use register version
            self.mov_imm64(Reg64::X16, imm);
            self.ands_reg(dst, src, Reg64::X16);
        }
    }

    /// TST Xn, Xm (alias for ANDS XZR, Xn, Xm)
    pub fn tst_reg(&mut self, src1: Reg64, src2: Reg64) {
        self.ands_reg(Reg64::SP, src1, src2);
    }

    /// ORR Xd, Xn, Xm (64-bit)
    pub fn orr_reg(&mut self, dst: Reg64, src1: Reg64, src2: Reg64) {
        let insn = 0xAA000000 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// ORN Xd, Xn, Xm (64-bit, OR NOT)
    pub fn orn_reg(&mut self, dst: Reg64, src1: Reg64, src2: Reg64) {
        let insn = 0xAA200000 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// MVN Xd, Xm (alias for ORN Xd, XZR, Xm)
    pub fn mvn(&mut self, dst: Reg64, src: Reg64) {
        self.orn_reg(dst, Reg64::SP, src);
    }

    /// EOR Xd, Xn, Xm (64-bit)
    pub fn eor_reg(&mut self, dst: Reg64, src1: Reg64, src2: Reg64) {
        let insn = 0xCA000000 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// EON Xd, Xn, Xm (64-bit, EOR NOT)
    pub fn eon_reg(&mut self, dst: Reg64, src1: Reg64, src2: Reg64) {
        let insn = 0xCA200000 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// BIC Xd, Xn, Xm (64-bit, AND NOT)
    pub fn bic_reg(&mut self, dst: Reg64, src1: Reg64, src2: Reg64) {
        let insn = 0x8A200000 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    // =========================================================================
    // Shift and Rotate Instructions
    // =========================================================================

    /// LSL Xd, Xn, Xm (64-bit variable shift)
    pub fn lsl_reg(&mut self, dst: Reg64, src: Reg64, amount: Reg64) {
        let insn = 0x9AC02000 | (amount.encoding() << 16) | (src.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// LSL Xd, Xn, #imm (64-bit immediate shift)
    pub fn lsl_imm(&mut self, dst: Reg64, src: Reg64, shift: u8) {
        let immr = (64 - shift) & 0x3F;
        let imms = 63 - shift;
        let insn = 0xD3400000
            | (1 << 22) // N bit for 64-bit
            | ((immr as u32) << 16)
            | ((imms as u32) << 10)
            | (src.encoding() << 5)
            | dst.encoding();
        self.emit(insn);
    }

    /// LSR Xd, Xn, Xm (64-bit variable shift)
    pub fn lsr_reg(&mut self, dst: Reg64, src: Reg64, amount: Reg64) {
        let insn = 0x9AC02400 | (amount.encoding() << 16) | (src.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// LSR Xd, Xn, #imm (64-bit immediate shift)
    pub fn lsr_imm(&mut self, dst: Reg64, src: Reg64, shift: u8) {
        let insn =
            0xD340FC00 | ((shift as u32 & 0x3F) << 16) | (src.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// ASR Xd, Xn, Xm (64-bit variable shift)
    pub fn asr_reg(&mut self, dst: Reg64, src: Reg64, amount: Reg64) {
        let insn = 0x9AC02800 | (amount.encoding() << 16) | (src.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// ASR Xd, Xn, #imm (64-bit immediate shift)
    pub fn asr_imm(&mut self, dst: Reg64, src: Reg64, shift: u8) {
        let insn =
            0x9340FC00 | ((shift as u32 & 0x3F) << 16) | (src.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// ROR Xd, Xn, Xm (64-bit variable rotate)
    pub fn ror_reg(&mut self, dst: Reg64, src: Reg64, amount: Reg64) {
        let insn = 0x9AC02C00 | (amount.encoding() << 16) | (src.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// ROR Xd, Xn, #imm (alias for EXTR Xd, Xn, Xn, #imm)
    pub fn ror_imm(&mut self, dst: Reg64, src: Reg64, shift: u8) {
        let insn = 0x93C00000
            | (src.encoding() << 16)
            | ((shift as u32 & 0x3F) << 10)
            | (src.encoding() << 5)
            | dst.encoding();
        self.emit(insn);
    }

    // =========================================================================
    // Multiply and Divide Instructions
    // =========================================================================

    /// MUL Xd, Xn, Xm (64-bit)
    pub fn mul(&mut self, dst: Reg64, src1: Reg64, src2: Reg64) {
        // MUL is alias for MADD Xd, Xn, Xm, XZR
        let insn = 0x9B007C00 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// MADD Xd, Xn, Xm, Xa (multiply-add: Xa + Xn * Xm)
    pub fn madd(&mut self, dst: Reg64, src1: Reg64, src2: Reg64, addend: Reg64) {
        let insn = 0x9B000000
            | (src2.encoding() << 16)
            | (addend.encoding() << 10)
            | (src1.encoding() << 5)
            | dst.encoding();
        self.emit(insn);
    }

    /// MSUB Xd, Xn, Xm, Xa (multiply-subtract: Xa - Xn * Xm)
    pub fn msub(&mut self, dst: Reg64, src1: Reg64, src2: Reg64, subtrahend: Reg64) {
        let insn = 0x9B008000
            | (src2.encoding() << 16)
            | (subtrahend.encoding() << 10)
            | (src1.encoding() << 5)
            | dst.encoding();
        self.emit(insn);
    }

    /// MNEG Xd, Xn, Xm (alias for MSUB Xd, Xn, Xm, XZR)
    pub fn mneg(&mut self, dst: Reg64, src1: Reg64, src2: Reg64) {
        self.msub(dst, src1, src2, Reg64::SP);
    }

    /// SMULL Xd, Wn, Wm (signed multiply long)
    pub fn smull(&mut self, dst: Reg64, src1: Reg32, src2: Reg32) {
        let insn = 0x9B207C00 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// UMULL Xd, Wn, Wm (unsigned multiply long)
    pub fn umull(&mut self, dst: Reg64, src1: Reg32, src2: Reg32) {
        let insn = 0x9BA07C00 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// SMULH Xd, Xn, Xm (signed multiply high)
    pub fn smulh(&mut self, dst: Reg64, src1: Reg64, src2: Reg64) {
        let insn = 0x9B407C00 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// UMULH Xd, Xn, Xm (unsigned multiply high)
    pub fn umulh(&mut self, dst: Reg64, src1: Reg64, src2: Reg64) {
        let insn = 0x9BC07C00 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// SDIV Xd, Xn, Xm (64-bit signed divide)
    pub fn sdiv(&mut self, dst: Reg64, src1: Reg64, src2: Reg64) {
        let insn = 0x9AC00C00 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// UDIV Xd, Xn, Xm (64-bit unsigned divide)
    pub fn udiv(&mut self, dst: Reg64, src1: Reg64, src2: Reg64) {
        let insn = 0x9AC00800 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    // =========================================================================
    // Load and Store Instructions
    // =========================================================================

    /// LDR Xt, [Xn, #imm] (64-bit load, unsigned offset)
    pub fn ldr(&mut self, dst: Reg64, base: Reg64, offset: u16) {
        let imm12 = (offset / 8) as u32;
        let insn = 0xF9400000 | (imm12 << 10) | (base.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// LDR Wt, [Xn, #imm] (32-bit load, unsigned offset)
    pub fn ldr32(&mut self, dst: Reg32, base: Reg64, offset: u16) {
        let imm12 = (offset / 4) as u32;
        let insn = 0xB9400000 | (imm12 << 10) | (base.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// LDRB Wt, [Xn, #imm] (8-bit load, unsigned offset)
    pub fn ldrb(&mut self, dst: Reg32, base: Reg64, offset: u16) {
        let insn = 0x39400000 | ((offset as u32) << 10) | (base.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// LDRH Wt, [Xn, #imm] (16-bit load, unsigned offset)
    pub fn ldrh(&mut self, dst: Reg32, base: Reg64, offset: u16) {
        let imm12 = (offset / 2) as u32;
        let insn = 0x79400000 | (imm12 << 10) | (base.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// LDRSB Xt, [Xn, #imm] (signed 8-bit load, extend to 64-bit)
    pub fn ldrsb(&mut self, dst: Reg64, base: Reg64, offset: u16) {
        let insn = 0x39800000 | ((offset as u32) << 10) | (base.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// LDRSH Xt, [Xn, #imm] (signed 16-bit load, extend to 64-bit)
    pub fn ldrsh(&mut self, dst: Reg64, base: Reg64, offset: u16) {
        let imm12 = (offset / 2) as u32;
        let insn = 0x79800000 | (imm12 << 10) | (base.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// LDRSW Xt, [Xn, #imm] (signed 32-bit load, extend to 64-bit)
    pub fn ldrsw(&mut self, dst: Reg64, base: Reg64, offset: u16) {
        let imm12 = (offset / 4) as u32;
        let insn = 0xB9800000 | (imm12 << 10) | (base.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// LDR Xt, [Xn, Xm] (register offset)
    pub fn ldr_reg(&mut self, dst: Reg64, base: Reg64, offset: Reg64) {
        let insn = 0xF8606800 | (offset.encoding() << 16) | (base.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// LDUR Xt, [Xn, #simm] (unscaled offset, -256 to 255)
    pub fn ldur(&mut self, dst: Reg64, base: Reg64, offset: i16) {
        let imm9 = (offset as u32) & 0x1FF;
        let insn = 0xF8400000 | (imm9 << 12) | (base.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// LDP Xt1, Xt2, [Xn, #imm] (load pair)
    pub fn ldp(&mut self, dst1: Reg64, dst2: Reg64, base: Reg64, offset: i16) {
        let imm7 = ((offset / 8) as u32) & 0x7F;
        let insn = 0xA9400000
            | (imm7 << 15)
            | (dst2.encoding() << 10)
            | (base.encoding() << 5)
            | dst1.encoding();
        self.emit(insn);
    }

    /// LDP Xt1, Xt2, [Xn], #imm (post-index)
    pub fn ldp_post(&mut self, dst1: Reg64, dst2: Reg64, base: Reg64, offset: i16) {
        let imm7 = ((offset / 8) as u32) & 0x7F;
        let insn = 0xA8C00000
            | (imm7 << 15)
            | (dst2.encoding() << 10)
            | (base.encoding() << 5)
            | dst1.encoding();
        self.emit(insn);
    }

    /// LDP Xt1, Xt2, [Xn, #imm]! (pre-index)
    pub fn ldp_pre(&mut self, dst1: Reg64, dst2: Reg64, base: Reg64, offset: i16) {
        let imm7 = ((offset / 8) as u32) & 0x7F;
        let insn = 0xA9C00000
            | (imm7 << 15)
            | (dst2.encoding() << 10)
            | (base.encoding() << 5)
            | dst1.encoding();
        self.emit(insn);
    }

    /// STR Xt, [Xn, #imm] (64-bit store, unsigned offset)
    pub fn str(&mut self, src: Reg64, base: Reg64, offset: u16) {
        let imm12 = (offset / 8) as u32;
        let insn = 0xF9000000 | (imm12 << 10) | (base.encoding() << 5) | src.encoding();
        self.emit(insn);
    }

    /// STR Wt, [Xn, #imm] (32-bit store, unsigned offset)
    pub fn str32(&mut self, src: Reg32, base: Reg64, offset: u16) {
        let imm12 = (offset / 4) as u32;
        let insn = 0xB9000000 | (imm12 << 10) | (base.encoding() << 5) | src.encoding();
        self.emit(insn);
    }

    /// STRB Wt, [Xn, #imm] (8-bit store)
    pub fn strb(&mut self, src: Reg32, base: Reg64, offset: u16) {
        let insn = 0x39000000 | ((offset as u32) << 10) | (base.encoding() << 5) | src.encoding();
        self.emit(insn);
    }

    /// STRH Wt, [Xn, #imm] (16-bit store)
    pub fn strh(&mut self, src: Reg32, base: Reg64, offset: u16) {
        let imm12 = (offset / 2) as u32;
        let insn = 0x79000000 | (imm12 << 10) | (base.encoding() << 5) | src.encoding();
        self.emit(insn);
    }

    /// STR Xt, [Xn, Xm] (register offset)
    pub fn str_reg(&mut self, src: Reg64, base: Reg64, offset: Reg64) {
        let insn = 0xF8206800 | (offset.encoding() << 16) | (base.encoding() << 5) | src.encoding();
        self.emit(insn);
    }

    /// STUR Xt, [Xn, #simm] (unscaled offset)
    pub fn stur(&mut self, src: Reg64, base: Reg64, offset: i16) {
        let imm9 = (offset as u32) & 0x1FF;
        let insn = 0xF8000000 | (imm9 << 12) | (base.encoding() << 5) | src.encoding();
        self.emit(insn);
    }

    /// STP Xt1, Xt2, [Xn, #imm] (store pair)
    pub fn stp(&mut self, src1: Reg64, src2: Reg64, base: Reg64, offset: i16) {
        let imm7 = ((offset / 8) as u32) & 0x7F;
        let insn = 0xA9000000
            | (imm7 << 15)
            | (src2.encoding() << 10)
            | (base.encoding() << 5)
            | src1.encoding();
        self.emit(insn);
    }

    /// STP Xt1, Xt2, [Xn], #imm (post-index)
    pub fn stp_post(&mut self, src1: Reg64, src2: Reg64, base: Reg64, offset: i16) {
        let imm7 = ((offset / 8) as u32) & 0x7F;
        let insn = 0xA8800000
            | (imm7 << 15)
            | (src2.encoding() << 10)
            | (base.encoding() << 5)
            | src1.encoding();
        self.emit(insn);
    }

    /// STP Xt1, Xt2, [Xn, #imm]! (pre-index)
    pub fn stp_pre(&mut self, src1: Reg64, src2: Reg64, base: Reg64, offset: i16) {
        let imm7 = ((offset / 8) as u32) & 0x7F;
        let insn = 0xA9800000
            | (imm7 << 15)
            | (src2.encoding() << 10)
            | (base.encoding() << 5)
            | src1.encoding();
        self.emit(insn);
    }

    // =========================================================================
    // Branch Instructions
    // =========================================================================

    /// B label (unconditional branch)
    pub fn b(&mut self, offset: i32) {
        let imm26 = ((offset >> 2) as u32) & 0x3FFFFFF;
        let insn = 0x14000000 | imm26;
        self.emit(insn);
    }

    /// B label (branch to label)
    pub fn b_label(&mut self, label: u32) {
        if let Some(&target) = self.labels.get(&label) {
            let offset = (target as i32) - (self.code.len() as i32);
            self.b(offset);
        } else {
            self.label_refs.push(LabelRef {
                offset: self.code.len(),
                label,
                kind: BranchKind::Unconditional,
            });
            self.emit(0x14000000); // Placeholder
        }
    }

    /// BL label (branch with link)
    pub fn bl(&mut self, offset: i32) {
        let imm26 = ((offset >> 2) as u32) & 0x3FFFFFF;
        let insn = 0x94000000 | imm26;
        self.emit(insn);
    }

    /// BL symbol (branch with link to symbol)
    pub fn bl_symbol(&mut self, symbol: &str) {
        self.relocations.push(Relocation {
            offset: self.code.len(),
            symbol: symbol.to_string(),
            kind: RelocKind::Branch26,
            addend: 0,
        });
        self.emit(0x94000000);
    }

    /// BR Xn (branch to register)
    pub fn br(&mut self, target: Reg64) {
        let insn = 0xD61F0000 | (target.encoding() << 5);
        self.emit(insn);
    }

    /// BLR Xn (branch with link to register)
    pub fn blr(&mut self, target: Reg64) {
        let insn = 0xD63F0000 | (target.encoding() << 5);
        self.emit(insn);
    }

    /// RET (return, alias for BR X30)
    pub fn ret(&mut self) {
        self.ret_reg(Reg64::LR);
    }

    /// RET Xn (return to register)
    pub fn ret_reg(&mut self, target: Reg64) {
        let insn = 0xD65F0000 | (target.encoding() << 5);
        self.emit(insn);
    }

    /// B.cond label (conditional branch)
    pub fn b_cond(&mut self, cond: Cond, offset: i32) {
        let imm19 = ((offset >> 2) as u32) & 0x7FFFF;
        let insn = 0x54000000 | (imm19 << 5) | cond.encoding();
        self.emit(insn);
    }

    /// B.cond label (conditional branch to label)
    pub fn b_cond_label(&mut self, cond: Cond, label: u32) {
        if let Some(&target) = self.labels.get(&label) {
            let offset = (target as i32) - (self.code.len() as i32);
            self.b_cond(cond, offset);
        } else {
            self.label_refs.push(LabelRef {
                offset: self.code.len(),
                label,
                kind: BranchKind::Conditional,
            });
            self.emit(0x54000000 | cond.encoding()); // Placeholder
        }
    }

    /// CBZ Xn, label (compare and branch if zero)
    pub fn cbz(&mut self, reg: Reg64, offset: i32) {
        let imm19 = ((offset >> 2) as u32) & 0x7FFFF;
        let insn = 0xB4000000 | (imm19 << 5) | reg.encoding();
        self.emit(insn);
    }

    /// CBZ Xn, label (to label)
    pub fn cbz_label(&mut self, reg: Reg64, label: u32) {
        if let Some(&target) = self.labels.get(&label) {
            let offset = (target as i32) - (self.code.len() as i32);
            self.cbz(reg, offset);
        } else {
            self.label_refs.push(LabelRef {
                offset: self.code.len(),
                label,
                kind: BranchKind::Compare,
            });
            self.emit(0xB4000000 | reg.encoding());
        }
    }

    /// CBNZ Xn, label (compare and branch if not zero)
    pub fn cbnz(&mut self, reg: Reg64, offset: i32) {
        let imm19 = ((offset >> 2) as u32) & 0x7FFFF;
        let insn = 0xB5000000 | (imm19 << 5) | reg.encoding();
        self.emit(insn);
    }

    /// CBNZ Xn, label (to label)
    pub fn cbnz_label(&mut self, reg: Reg64, label: u32) {
        if let Some(&target) = self.labels.get(&label) {
            let offset = (target as i32) - (self.code.len() as i32);
            self.cbnz(reg, offset);
        } else {
            self.label_refs.push(LabelRef {
                offset: self.code.len(),
                label,
                kind: BranchKind::Compare,
            });
            self.emit(0xB5000000 | reg.encoding());
        }
    }

    /// TBZ Xn, #bit, label (test bit and branch if zero)
    pub fn tbz(&mut self, reg: Reg64, bit: u8, offset: i32) {
        let imm14 = ((offset >> 2) as u32) & 0x3FFF;
        let b5 = ((bit >> 5) & 1) as u32;
        let b40 = (bit & 0x1F) as u32;
        let insn = 0x36000000 | (b5 << 31) | (b40 << 19) | (imm14 << 5) | reg.encoding();
        self.emit(insn);
    }

    /// TBNZ Xn, #bit, label (test bit and branch if not zero)
    pub fn tbnz(&mut self, reg: Reg64, bit: u8, offset: i32) {
        let imm14 = ((offset >> 2) as u32) & 0x3FFF;
        let b5 = ((bit >> 5) & 1) as u32;
        let b40 = (bit & 0x1F) as u32;
        let insn = 0x37000000 | (b5 << 31) | (b40 << 19) | (imm14 << 5) | reg.encoding();
        self.emit(insn);
    }

    // =========================================================================
    // Conditional Select Instructions
    // =========================================================================

    /// CSEL Xd, Xn, Xm, cond (conditional select)
    pub fn csel(&mut self, dst: Reg64, src1: Reg64, src2: Reg64, cond: Cond) {
        let insn = 0x9A800000
            | (src2.encoding() << 16)
            | (cond.encoding() << 12)
            | (src1.encoding() << 5)
            | dst.encoding();
        self.emit(insn);
    }

    /// CSINC Xd, Xn, Xm, cond (conditional select increment)
    pub fn csinc(&mut self, dst: Reg64, src1: Reg64, src2: Reg64, cond: Cond) {
        let insn = 0x9A800400
            | (src2.encoding() << 16)
            | (cond.encoding() << 12)
            | (src1.encoding() << 5)
            | dst.encoding();
        self.emit(insn);
    }

    /// CSINV Xd, Xn, Xm, cond (conditional select invert)
    pub fn csinv(&mut self, dst: Reg64, src1: Reg64, src2: Reg64, cond: Cond) {
        let insn = 0xDA800000
            | (src2.encoding() << 16)
            | (cond.encoding() << 12)
            | (src1.encoding() << 5)
            | dst.encoding();
        self.emit(insn);
    }

    /// CSNEG Xd, Xn, Xm, cond (conditional select negate)
    pub fn csneg(&mut self, dst: Reg64, src1: Reg64, src2: Reg64, cond: Cond) {
        let insn = 0xDA800400
            | (src2.encoding() << 16)
            | (cond.encoding() << 12)
            | (src1.encoding() << 5)
            | dst.encoding();
        self.emit(insn);
    }

    /// CSET Xd, cond (alias for CSINC Xd, XZR, XZR, invert(cond))
    pub fn cset(&mut self, dst: Reg64, cond: Cond) {
        self.csinc(dst, Reg64::SP, Reg64::SP, cond.invert());
    }

    /// CSETM Xd, cond (alias for CSINV Xd, XZR, XZR, invert(cond))
    pub fn csetm(&mut self, dst: Reg64, cond: Cond) {
        self.csinv(dst, Reg64::SP, Reg64::SP, cond.invert());
    }

    // =========================================================================
    // Address Generation
    // =========================================================================

    /// ADR Xd, label (form PC-relative address)
    pub fn adr(&mut self, dst: Reg64, offset: i32) {
        let immlo = (offset & 0x3) as u32;
        let immhi = ((offset >> 2) & 0x7FFFF) as u32;
        let insn = 0x10000000 | (immlo << 29) | (immhi << 5) | dst.encoding();
        self.emit(insn);
    }

    /// ADRP Xd, label (form PC-relative address to 4KB page)
    pub fn adrp(&mut self, dst: Reg64, offset: i32) {
        let immlo = ((offset >> 12) & 0x3) as u32;
        let immhi = ((offset >> 14) & 0x7FFFF) as u32;
        let insn = 0x90000000 | (immlo << 29) | (immhi << 5) | dst.encoding();
        self.emit(insn);
    }

    // =========================================================================
    // Floating Point Instructions
    // =========================================================================

    /// FMOV Dd, Dn (copy double)
    pub fn fmov_d(&mut self, dst: RegD, src: RegD) {
        let insn = 0x1E604000 | (src.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// FMOV Sd, Sn (copy single)
    pub fn fmov_s(&mut self, dst: RegS, src: RegS) {
        let insn = 0x1E204000 | (src.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// FMOV Xd, Dn (FP to general register)
    pub fn fmov_to_gpr(&mut self, dst: Reg64, src: RegD) {
        let insn = 0x9E660000 | (src.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// FMOV Dd, Xn (general register to FP)
    pub fn fmov_from_gpr(&mut self, dst: RegD, src: Reg64) {
        let insn = 0x9E670000 | (src.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// FADD Dd, Dn, Dm (double add)
    pub fn fadd_d(&mut self, dst: RegD, src1: RegD, src2: RegD) {
        let insn = 0x1E602800 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// FADD Sd, Sn, Sm (single add)
    pub fn fadd_s(&mut self, dst: RegS, src1: RegS, src2: RegS) {
        let insn = 0x1E202800 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// FSUB Dd, Dn, Dm (double subtract)
    pub fn fsub_d(&mut self, dst: RegD, src1: RegD, src2: RegD) {
        let insn = 0x1E603800 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// FSUB Sd, Sn, Sm (single subtract)
    pub fn fsub_s(&mut self, dst: RegS, src1: RegS, src2: RegS) {
        let insn = 0x1E203800 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// FMUL Dd, Dn, Dm (double multiply)
    pub fn fmul_d(&mut self, dst: RegD, src1: RegD, src2: RegD) {
        let insn = 0x1E600800 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// FMUL Sd, Sn, Sm (single multiply)
    pub fn fmul_s(&mut self, dst: RegS, src1: RegS, src2: RegS) {
        let insn = 0x1E200800 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// FDIV Dd, Dn, Dm (double divide)
    pub fn fdiv_d(&mut self, dst: RegD, src1: RegD, src2: RegD) {
        let insn = 0x1E601800 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// FDIV Sd, Sn, Sm (single divide)
    pub fn fdiv_s(&mut self, dst: RegS, src1: RegS, src2: RegS) {
        let insn = 0x1E201800 | (src2.encoding() << 16) | (src1.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// FNEG Dd, Dn (double negate)
    pub fn fneg_d(&mut self, dst: RegD, src: RegD) {
        let insn = 0x1E614000 | (src.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// FABS Dd, Dn (double absolute)
    pub fn fabs_d(&mut self, dst: RegD, src: RegD) {
        let insn = 0x1E60C000 | (src.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// FSQRT Dd, Dn (double square root)
    pub fn fsqrt_d(&mut self, dst: RegD, src: RegD) {
        let insn = 0x1E61C000 | (src.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// FCMP Dn, Dm (double compare)
    pub fn fcmp_d(&mut self, src1: RegD, src2: RegD) {
        let insn = 0x1E602000 | (src2.encoding() << 16) | (src1.encoding() << 5);
        self.emit(insn);
    }

    /// FCMP Sn, Sm (single compare)
    pub fn fcmp_s(&mut self, src1: RegS, src2: RegS) {
        let insn = 0x1E202000 | (src2.encoding() << 16) | (src1.encoding() << 5);
        self.emit(insn);
    }

    /// FCVTZS Xd, Dn (double to signed int, round toward zero)
    pub fn fcvtzs_d(&mut self, dst: Reg64, src: RegD) {
        let insn = 0x9E780000 | (src.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// SCVTF Dd, Xn (signed int to double)
    pub fn scvtf_d(&mut self, dst: RegD, src: Reg64) {
        let insn = 0x9E620000 | (src.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// UCVTF Dd, Xn (unsigned int to double)
    pub fn ucvtf_d(&mut self, dst: RegD, src: Reg64) {
        let insn = 0x9E630000 | (src.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// FCVT Dd, Sn (single to double)
    pub fn fcvt_s_to_d(&mut self, dst: RegD, src: RegS) {
        let insn = 0x1E22C000 | (src.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// FCVT Sd, Dn (double to single)
    pub fn fcvt_d_to_s(&mut self, dst: RegS, src: RegD) {
        let insn = 0x1E624000 | (src.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// LDR Dt, [Xn, #imm] (load double)
    pub fn ldr_d(&mut self, dst: RegD, base: Reg64, offset: u16) {
        let imm12 = (offset / 8) as u32;
        let insn = 0xFD400000 | (imm12 << 10) | (base.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// LDR St, [Xn, #imm] (load single)
    pub fn ldr_s(&mut self, dst: RegS, base: Reg64, offset: u16) {
        let imm12 = (offset / 4) as u32;
        let insn = 0xBD400000 | (imm12 << 10) | (base.encoding() << 5) | dst.encoding();
        self.emit(insn);
    }

    /// STR Dt, [Xn, #imm] (store double)
    pub fn str_d(&mut self, src: RegD, base: Reg64, offset: u16) {
        let imm12 = (offset / 8) as u32;
        let insn = 0xFD000000 | (imm12 << 10) | (base.encoding() << 5) | src.encoding();
        self.emit(insn);
    }

    /// STR St, [Xn, #imm] (store single)
    pub fn str_s(&mut self, src: RegS, base: Reg64, offset: u16) {
        let imm12 = (offset / 4) as u32;
        let insn = 0xBD000000 | (imm12 << 10) | (base.encoding() << 5) | src.encoding();
        self.emit(insn);
    }

    // =========================================================================
    // System Instructions
    // =========================================================================

    /// NOP (no operation)
    pub fn nop(&mut self) {
        self.emit(0xD503201F);
    }

    /// BRK #imm (breakpoint)
    pub fn brk(&mut self, imm: u16) {
        let insn = 0xD4200000 | ((imm as u32) << 5);
        self.emit(insn);
    }

    /// HLT #imm (halt)
    pub fn hlt(&mut self, imm: u16) {
        let insn = 0xD4400000 | ((imm as u32) << 5);
        self.emit(insn);
    }

    /// SVC #imm (supervisor call)
    pub fn svc(&mut self, imm: u16) {
        let insn = 0xD4000001 | ((imm as u32) << 5);
        self.emit(insn);
    }

    /// DMB (data memory barrier)
    pub fn dmb(&mut self, opt: u8) {
        let insn = 0xD5033000 | ((opt as u32 & 0xF) << 8) | 0xBF;
        self.emit(insn);
    }

    /// DSB (data synchronization barrier)
    pub fn dsb(&mut self, opt: u8) {
        let insn = 0xD5033000 | ((opt as u32 & 0xF) << 8) | 0x9F;
        self.emit(insn);
    }

    /// ISB (instruction synchronization barrier)
    pub fn isb(&mut self) {
        self.emit(0xD5033FDF);
    }

    // =========================================================================
    // Label Fixup
    // =========================================================================

    /// Fix up all forward label references.
    pub fn fixup_labels(&mut self) {
        for label_ref in &self.label_refs {
            if let Some(&target) = self.labels.get(&label_ref.label) {
                let offset = (target as i32) - (label_ref.offset as i32);

                // Read existing instruction
                let mut insn = u32::from_le_bytes([
                    self.code[label_ref.offset],
                    self.code[label_ref.offset + 1],
                    self.code[label_ref.offset + 2],
                    self.code[label_ref.offset + 3],
                ]);

                // Apply fixup based on branch kind
                match label_ref.kind {
                    BranchKind::Unconditional => {
                        let imm26 = ((offset >> 2) as u32) & 0x3FFFFFF;
                        insn = (insn & 0xFC000000) | imm26;
                    }
                    BranchKind::Conditional | BranchKind::Compare => {
                        let imm19 = ((offset >> 2) as u32) & 0x7FFFF;
                        insn = (insn & 0xFF00001F) | (imm19 << 5);
                    }
                    BranchKind::Test => {
                        let imm14 = ((offset >> 2) as u32) & 0x3FFF;
                        insn = (insn & 0xFFF8001F) | (imm14 << 5);
                    }
                    BranchKind::Adr => {
                        let immlo = (offset & 0x3) as u32;
                        let immhi = ((offset >> 2) & 0x7FFFF) as u32;
                        insn = (insn & 0x9F00001F) | (immlo << 29) | (immhi << 5);
                    }
                }

                // Write back
                self.code[label_ref.offset..label_ref.offset + 4]
                    .copy_from_slice(&insn.to_le_bytes());
            } else {
                panic!("Undefined label: {}", label_ref.label);
            }
        }
        self.label_refs.clear();
    }

    /// Align code to given boundary with NOPs.
    pub fn align(&mut self, alignment: usize) {
        while self.code.len() % alignment != 0 {
            self.nop();
        }
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

    /// TST Xn, #imm (test immediate using AND with XZR)
    pub fn tst_imm(&mut self, src: Reg64, imm: u64) {
        self.ands_imm(Reg64::SP, src, imm);
    }

    /// MOV Xd, Xn (alias for ORR Xd, XZR, Xn)
    pub fn mov_reg(&mut self, dst: Reg64, src: Reg64) {
        self.mov(dst, src);
    }

    /// Get the generated code.
    pub fn finish(mut self) -> Vec<u8> {
        self.fixup_labels();
        self.code
    }
}

impl Default for Arm64Encoder {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Encode a bitmask immediate for AND/ORR/EOR.
/// Returns (N, immr, imms) if encodable, None otherwise.
fn encode_bitmask_imm(imm: u64, is_64bit: bool) -> Option<(u32, u32, u32)> {
    if imm == 0 || imm == !0u64 {
        return None;
    }

    let size = if is_64bit { 64 } else { 32 };
    let mask = if is_64bit { !0u64 } else { 0xFFFFFFFF };
    let imm = imm & mask;

    // Try different element sizes
    for log_size in 1..=6 {
        let element_size = 1 << log_size;
        if element_size > size {
            break;
        }

        let element_mask = (1u64 << element_size) - 1;
        let element = imm & element_mask;

        // Check if pattern repeats
        let mut valid = true;
        for i in 1..(size / element_size) {
            if ((imm >> (i * element_size)) & element_mask) != element {
                valid = false;
                break;
            }
        }

        if !valid {
            continue;
        }

        // Count trailing ones
        let ones = element.trailing_ones() as usize;
        if ones == 0 || ones == element_size {
            continue;
        }

        // Count leading zeros in the rotated element
        let rotated = element.rotate_right(ones as u32);
        let zeros = (rotated & element_mask).leading_zeros() as usize - (64 - element_size);

        if ones + zeros == element_size {
            // Encodable
            let n = if element_size == 64 { 1 } else { 0 };
            let immr = (element_size - ones) & (element_size - 1);
            let imms = ((!((element_size << 1) - 1)) | (ones - 1)) & 0x3F;

            return Some((n as u32, immr as u32, imms as u32));
        }
    }

    None
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_imm() {
        let mut enc = Arm64Encoder::new();
        enc.add_imm(Reg64::X0, Reg64::X1, 42);
        let insn = u32::from_le_bytes(enc.code[0..4].try_into().unwrap());
        assert_eq!(insn & 0xFF000000, 0x91000000); // ADD immediate opcode
    }

    #[test]
    fn test_sub_imm() {
        let mut enc = Arm64Encoder::new();
        enc.sub_imm(Reg64::SP, Reg64::SP, 16);
        assert_eq!(enc.code.len(), 4);
    }

    #[test]
    fn test_mov_reg() {
        let mut enc = Arm64Encoder::new();
        enc.mov(Reg64::X0, Reg64::X1);
        let insn = u32::from_le_bytes(enc.code[0..4].try_into().unwrap());
        // MOV is alias for ORR Xd, XZR, Xm
        assert_eq!(insn & 0xFF000000, 0xAA000000);
    }

    #[test]
    fn test_movz() {
        let mut enc = Arm64Encoder::new();
        enc.movz(Reg64::X0, 0x1234, 0);
        let insn = u32::from_le_bytes(enc.code[0..4].try_into().unwrap());
        assert_eq!(insn & 0xFF800000, 0xD2800000); // MOVZ opcode
    }

    #[test]
    fn test_add_reg() {
        let mut enc = Arm64Encoder::new();
        enc.add_reg(Reg64::X0, Reg64::X1, Reg64::X2);
        let insn = u32::from_le_bytes(enc.code[0..4].try_into().unwrap());
        assert_eq!(insn & 0xFF000000, 0x8B000000);
    }

    #[test]
    fn test_sub_reg() {
        let mut enc = Arm64Encoder::new();
        enc.sub_reg(Reg64::X0, Reg64::X1, Reg64::X2);
        let insn = u32::from_le_bytes(enc.code[0..4].try_into().unwrap());
        assert_eq!(insn & 0xFF000000, 0xCB000000);
    }

    #[test]
    fn test_mul() {
        let mut enc = Arm64Encoder::new();
        enc.mul(Reg64::X0, Reg64::X1, Reg64::X2);
        let insn = u32::from_le_bytes(enc.code[0..4].try_into().unwrap());
        assert_eq!(insn & 0xFFE00000, 0x9B000000); // MUL/MADD opcode
    }

    #[test]
    fn test_sdiv() {
        let mut enc = Arm64Encoder::new();
        enc.sdiv(Reg64::X0, Reg64::X1, Reg64::X2);
        let insn = u32::from_le_bytes(enc.code[0..4].try_into().unwrap());
        assert_eq!(insn & 0xFFE0FC00, 0x9AC00C00); // SDIV opcode
    }

    #[test]
    fn test_ldr() {
        let mut enc = Arm64Encoder::new();
        enc.ldr(Reg64::X0, Reg64::SP, 8);
        let insn = u32::from_le_bytes(enc.code[0..4].try_into().unwrap());
        assert_eq!(insn & 0xFFC00000, 0xF9400000); // LDR unsigned offset
    }

    #[test]
    fn test_str() {
        let mut enc = Arm64Encoder::new();
        enc.str(Reg64::X0, Reg64::SP, 16);
        let insn = u32::from_le_bytes(enc.code[0..4].try_into().unwrap());
        assert_eq!(insn & 0xFFC00000, 0xF9000000); // STR unsigned offset
    }

    #[test]
    fn test_stp_pre() {
        let mut enc = Arm64Encoder::new();
        enc.stp_pre(Reg64::FP, Reg64::LR, Reg64::SP, -16);
        assert_eq!(enc.code.len(), 4);
    }

    #[test]
    fn test_ldp_post() {
        let mut enc = Arm64Encoder::new();
        enc.ldp_post(Reg64::FP, Reg64::LR, Reg64::SP, 16);
        assert_eq!(enc.code.len(), 4);
    }

    #[test]
    fn test_b() {
        let mut enc = Arm64Encoder::new();
        enc.b(0x100);
        let insn = u32::from_le_bytes(enc.code[0..4].try_into().unwrap());
        assert_eq!(insn & 0xFC000000, 0x14000000); // B opcode
    }

    #[test]
    fn test_bl() {
        let mut enc = Arm64Encoder::new();
        enc.bl(0x200);
        let insn = u32::from_le_bytes(enc.code[0..4].try_into().unwrap());
        assert_eq!(insn & 0xFC000000, 0x94000000); // BL opcode
    }

    #[test]
    fn test_br() {
        let mut enc = Arm64Encoder::new();
        enc.br(Reg64::X0);
        let insn = u32::from_le_bytes(enc.code[0..4].try_into().unwrap());
        assert_eq!(insn & 0xFFFFFC1F, 0xD61F0000); // BR opcode
    }

    #[test]
    fn test_blr() {
        let mut enc = Arm64Encoder::new();
        enc.blr(Reg64::X0);
        let insn = u32::from_le_bytes(enc.code[0..4].try_into().unwrap());
        assert_eq!(insn & 0xFFFFFC1F, 0xD63F0000); // BLR opcode
    }

    #[test]
    fn test_ret() {
        let mut enc = Arm64Encoder::new();
        enc.ret();
        let insn = u32::from_le_bytes(enc.code[0..4].try_into().unwrap());
        assert_eq!(insn, 0xD65F03C0); // RET (X30)
    }

    #[test]
    fn test_b_cond() {
        let mut enc = Arm64Encoder::new();
        enc.b_cond(Cond::EQ, 0x10);
        let insn = u32::from_le_bytes(enc.code[0..4].try_into().unwrap());
        assert_eq!(insn & 0xFF00001F, 0x54000000); // B.cond opcode + EQ
    }

    #[test]
    fn test_cbz() {
        let mut enc = Arm64Encoder::new();
        enc.cbz(Reg64::X0, 0x20);
        let insn = u32::from_le_bytes(enc.code[0..4].try_into().unwrap());
        assert_eq!(insn & 0xFF000000, 0xB4000000); // CBZ opcode
    }

    #[test]
    fn test_cmp_reg() {
        let mut enc = Arm64Encoder::new();
        enc.cmp_reg(Reg64::X0, Reg64::X1);
        let insn = u32::from_le_bytes(enc.code[0..4].try_into().unwrap());
        assert_eq!(insn & 0xFF00001F, 0xEB00001F); // SUBS XZR opcode
    }

    #[test]
    fn test_cset() {
        let mut enc = Arm64Encoder::new();
        enc.cset(Reg64::X0, Cond::EQ);
        assert_eq!(enc.code.len(), 4);
    }

    #[test]
    fn test_lsl_imm() {
        let mut enc = Arm64Encoder::new();
        enc.lsl_imm(Reg64::X0, Reg64::X1, 4);
        assert_eq!(enc.code.len(), 4);
    }

    #[test]
    fn test_lsr_imm() {
        let mut enc = Arm64Encoder::new();
        enc.lsr_imm(Reg64::X0, Reg64::X1, 4);
        assert_eq!(enc.code.len(), 4);
    }

    #[test]
    fn test_asr_imm() {
        let mut enc = Arm64Encoder::new();
        enc.asr_imm(Reg64::X0, Reg64::X1, 4);
        assert_eq!(enc.code.len(), 4);
    }

    #[test]
    fn test_nop() {
        let mut enc = Arm64Encoder::new();
        enc.nop();
        let insn = u32::from_le_bytes(enc.code[0..4].try_into().unwrap());
        assert_eq!(insn, 0xD503201F);
    }

    #[test]
    fn test_svc() {
        let mut enc = Arm64Encoder::new();
        enc.svc(0);
        let insn = u32::from_le_bytes(enc.code[0..4].try_into().unwrap());
        assert_eq!(insn, 0xD4000001);
    }

    #[test]
    fn test_labels() {
        let mut enc = Arm64Encoder::new();
        let label = enc.new_label();
        enc.b_label(label);
        enc.nop();
        enc.define_label(label);
        enc.nop();
        enc.fixup_labels();

        // Branch should go forward 4 bytes (1 instruction)
        let insn = u32::from_le_bytes(enc.code[0..4].try_into().unwrap());
        let imm26 = insn & 0x3FFFFFF;
        assert_eq!(imm26, 2); // 2 instructions = 8 bytes / 4
    }

    #[test]
    fn test_fadd_d() {
        let mut enc = Arm64Encoder::new();
        enc.fadd_d(RegD::D0, RegD::D1, RegD::D2);
        let insn = u32::from_le_bytes(enc.code[0..4].try_into().unwrap());
        // Check opcode bits (mask extracts bits 10-11 and 21-31)
        assert_eq!(insn & 0xFFE00C00, 0x1E600800);
    }

    #[test]
    fn test_condition_invert() {
        assert_eq!(Cond::EQ.invert(), Cond::NE);
        assert_eq!(Cond::LT.invert(), Cond::GE);
        assert_eq!(Cond::GT.invert(), Cond::LE);
    }

    #[test]
    fn test_prologue_epilogue() {
        let mut enc = Arm64Encoder::new();
        // Standard prologue
        enc.stp_pre(Reg64::FP, Reg64::LR, Reg64::SP, -16);
        enc.mov(Reg64::FP, Reg64::SP);

        // Function body
        enc.nop();

        // Standard epilogue
        enc.ldp_post(Reg64::FP, Reg64::LR, Reg64::SP, 16);
        enc.ret();

        assert_eq!(enc.code.len(), 20); // 5 instructions * 4 bytes
    }

    #[test]
    fn test_mov_imm64_small() {
        let mut enc = Arm64Encoder::new();
        enc.mov_imm64(Reg64::X0, 42);
        // Should use single MOVZ
        assert_eq!(enc.code.len(), 4);
    }

    #[test]
    fn test_mov_imm64_large() {
        let mut enc = Arm64Encoder::new();
        enc.mov_imm64(Reg64::X0, 0x123456789ABCDEF0);
        // Should use MOVZ + MOVKs
        assert!(enc.code.len() >= 8);
    }
}
