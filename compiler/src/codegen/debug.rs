// ===============================================================================
// QUANTALANG CODE GENERATOR - DEBUG INFO (DWARF)
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! DWARF debug information generation.
//!
//! This module generates DWARF debugging information for native code backends,
//! enabling source-level debugging with tools like GDB and LLDB.
//!
//! ## Supported DWARF Versions
//!
//! - DWARF 4 (default, wide compatibility)
//! - DWARF 5 (optional, better compression)
//!
//! ## Sections Generated
//!
//! - `.debug_info` - Type and variable information
//! - `.debug_abbrev` - Abbreviation tables
//! - `.debug_line` - Line number program
//! - `.debug_str` - String table
//! - `.debug_ranges` - Address ranges
//! - `.debug_aranges` - Address range lookup
//! - `.debug_frame` - Call frame information

use std::collections::HashMap;
use std::fmt::Write;

use crate::codegen::ir::*;

// =============================================================================
// DWARF Constants
// =============================================================================

/// DWARF tag values (DW_TAG_*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum DwarfTag {
    ArrayType = 0x01,
    ClassType = 0x02,
    EntryPoint = 0x03,
    EnumerationType = 0x04,
    FormalParameter = 0x05,
    LabelDef = 0x0a,
    LexicalBlock = 0x0b,
    Member = 0x0d,
    PointerType = 0x0f,
    ReferenceType = 0x10,
    CompileUnit = 0x11,
    StringType = 0x12,
    StructureType = 0x13,
    SubroutineType = 0x15,
    TypeDef = 0x16,
    UnionType = 0x17,
    UnspecifiedParameters = 0x18,
    Variant = 0x19,
    CommonBlock = 0x1a,
    CommonInclusion = 0x1b,
    Inheritance = 0x1c,
    InlinedSubroutine = 0x1d,
    Module = 0x1e,
    PtrToMemberType = 0x1f,
    SetType = 0x20,
    SubrangeType = 0x21,
    WithStmt = 0x22,
    AccessDeclaration = 0x23,
    BaseType = 0x24,
    CatchBlock = 0x25,
    ConstType = 0x26,
    Constant = 0x27,
    Enumerator = 0x28,
    FileType = 0x29,
    Friend = 0x2a,
    Namespace = 0x39,
    Subprogram = 0x2e,
    TemplateTypeParameter = 0x2f,
    TemplateValueParameter = 0x30,
    ThrownType = 0x31,
    TryBlock = 0x32,
    VariantPart = 0x33,
    Variable = 0x34,
    VolatileType = 0x35,
    RestrictType = 0x37,
    InterfaceType = 0x38,
    UnspecifiedType = 0x3b,
    PartialUnit = 0x3c,
    ImportedUnit = 0x3d,
    Condition = 0x3f,
    SharedType = 0x40,
    TypeUnit = 0x41,
    RvalueReferenceType = 0x42,
    TemplateAlias = 0x43,
    CoarrayType = 0x44,
    GenericSubrange = 0x45,
    DynamicType = 0x46,
    AtomicType = 0x47,
    CallSite = 0x48,
    CallSiteParameter = 0x49,
    SkeletonUnit = 0x4a,
    ImmutableType = 0x4b,
}

/// DWARF attribute values (DW_AT_*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum DwarfAttr {
    Sibling = 0x01,
    Location = 0x02,
    Name = 0x03,
    Ordering = 0x09,
    ByteSize = 0x0b,
    BitOffset = 0x0c,
    BitSize = 0x0d,
    StmtList = 0x10,
    LowPc = 0x11,
    HighPc = 0x12,
    Language = 0x13,
    Discr = 0x15,
    DiscrValue = 0x16,
    Visibility = 0x17,
    Import = 0x18,
    StringLength = 0x19,
    CommonReference = 0x1a,
    CompDir = 0x1b,
    ConstValue = 0x1c,
    ContainingType = 0x1d,
    DefaultValue = 0x1e,
    Inline = 0x20,
    IsOptional = 0x21,
    LowerBound = 0x22,
    Producer = 0x25,
    Prototyped = 0x27,
    ReturnAddr = 0x2a,
    StartScope = 0x2c,
    BitStride = 0x2e,
    UpperBound = 0x2f,
    AbstractOrigin = 0x31,
    Accessibility = 0x32,
    AddressClass = 0x33,
    Artificial = 0x34,
    BaseTypes = 0x35,
    CallingConvention = 0x36,
    Count = 0x37,
    DataMemberLocation = 0x38,
    DeclColumn = 0x39,
    DeclFile = 0x3a,
    DeclLine = 0x3b,
    Declaration = 0x3c,
    DiscrList = 0x3d,
    Encoding = 0x3e,
    External = 0x3f,
    FrameBase = 0x40,
    Friend = 0x41,
    IdentifierCase = 0x42,
    MacroInfo = 0x43,
    NamelistItem = 0x44,
    Priority = 0x45,
    Segment = 0x46,
    Specification = 0x47,
    StaticLink = 0x48,
    Type = 0x49,
    UseLocation = 0x4a,
    VariableParameter = 0x4b,
    Virtuality = 0x4c,
    VtableElemLocation = 0x4d,
    Allocated = 0x4e,
    Associated = 0x4f,
    DataLocation = 0x50,
    ByteStride = 0x51,
    EntryPc = 0x52,
    UseUtf8 = 0x53,
    Extension = 0x54,
    Ranges = 0x55,
    Trampoline = 0x56,
    CallColumn = 0x57,
    CallFile = 0x58,
    CallLine = 0x59,
    Description = 0x5a,
    BinaryScale = 0x5b,
    DecimalScale = 0x5c,
    Small = 0x5d,
    DecimalSign = 0x5e,
    DigitCount = 0x5f,
    PictureString = 0x60,
    Mutable = 0x61,
    ThreadsScaled = 0x62,
    Explicit = 0x63,
    ObjectPointer = 0x64,
    Endianity = 0x65,
    Elemental = 0x66,
    Pure = 0x67,
    Recursive = 0x68,
    Signature = 0x69,
    MainSubprogram = 0x6a,
    DataBitOffset = 0x6b,
    ConstExpr = 0x6c,
    EnumClass = 0x6d,
    LinkageName = 0x6e,
    StringLengthBitSize = 0x6f,
    StringLengthByteSize = 0x70,
    Rank = 0x71,
    StrOffsetsBase = 0x72,
    AddrBase = 0x73,
    RnglistsBase = 0x74,
    DwoName = 0x76,
    Reference = 0x77,
    RvalueReference = 0x78,
    Macros = 0x79,
    CallAllCalls = 0x7a,
    CallAllSourceCalls = 0x7b,
    CallAllTailCalls = 0x7c,
    CallReturnPc = 0x7d,
    CallValue = 0x7e,
    CallOrigin = 0x7f,
    CallParameter = 0x80,
    CallPc = 0x81,
    CallTailCall = 0x82,
    CallTarget = 0x83,
    CallTargetClobbered = 0x84,
    CallDataLocation = 0x85,
    CallDataValue = 0x86,
    Noreturn = 0x87,
    Alignment = 0x88,
    ExportSymbols = 0x89,
    Deleted = 0x8a,
    Defaulted = 0x8b,
    LoclistsBase = 0x8c,
}

/// DWARF form values (DW_FORM_*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DwarfForm {
    Addr = 0x01,
    Block2 = 0x03,
    Block4 = 0x04,
    Data2 = 0x05,
    Data4 = 0x06,
    Data8 = 0x07,
    String = 0x08,
    Block = 0x09,
    Block1 = 0x0a,
    Data1 = 0x0b,
    Flag = 0x0c,
    Sdata = 0x0d,
    Strp = 0x0e,
    Udata = 0x0f,
    RefAddr = 0x10,
    Ref1 = 0x11,
    Ref2 = 0x12,
    Ref4 = 0x13,
    Ref8 = 0x14,
    RefUdata = 0x15,
    Indirect = 0x16,
    SecOffset = 0x17,
    ExprLoc = 0x18,
    FlagPresent = 0x19,
    Strx = 0x1a,
    Addrx = 0x1b,
    RefSup4 = 0x1c,
    StrpSup = 0x1d,
    Data16 = 0x1e,
    LineStrp = 0x1f,
    RefSig8 = 0x20,
    ImplicitConst = 0x21,
    Loclistx = 0x22,
    Rnglistx = 0x23,
    RefSup8 = 0x24,
    Strx1 = 0x25,
    Strx2 = 0x26,
    Strx3 = 0x27,
    Strx4 = 0x28,
    Addrx1 = 0x29,
    Addrx2 = 0x2a,
    Addrx3 = 0x2b,
    Addrx4 = 0x2c,
}

/// DWARF base type encoding (DW_ATE_*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DwarfEncoding {
    Address = 0x01,
    Boolean = 0x02,
    ComplexFloat = 0x03,
    Float = 0x04,
    Signed = 0x05,
    SignedChar = 0x06,
    Unsigned = 0x07,
    UnsignedChar = 0x08,
    ImaginaryFloat = 0x09,
    PackedDecimal = 0x0a,
    NumericString = 0x0b,
    Edited = 0x0c,
    SignedFixed = 0x0d,
    UnsignedFixed = 0x0e,
    DecimalFloat = 0x0f,
    Utf = 0x10,
    Ucs = 0x11,
    Ascii = 0x12,
}

/// DWARF language values (DW_LANG_*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum DwarfLang {
    C89 = 0x0001,
    C = 0x0002,
    Ada83 = 0x0003,
    CPlusPlus = 0x0004,
    Cobol74 = 0x0005,
    Cobol85 = 0x0006,
    Fortran77 = 0x0007,
    Fortran90 = 0x0008,
    Pascal83 = 0x0009,
    Modula2 = 0x000a,
    Java = 0x000b,
    C99 = 0x000c,
    Ada95 = 0x000d,
    Fortran95 = 0x000e,
    Pli = 0x000f,
    ObjC = 0x0010,
    ObjCPlusPlus = 0x0011,
    Upc = 0x0012,
    D = 0x0013,
    Python = 0x0014,
    OpenCl = 0x0015,
    Go = 0x0016,
    Modula3 = 0x0017,
    Haskell = 0x0018,
    CPlusPlus03 = 0x0019,
    CPlusPlus11 = 0x001a,
    OCaml = 0x001b,
    Rust = 0x001c,
    C11 = 0x001d,
    Swift = 0x001e,
    Julia = 0x001f,
    Dylan = 0x0020,
    CPlusPlus14 = 0x0021,
    Fortran03 = 0x0022,
    Fortran08 = 0x0023,
    RenderScript = 0x0024,
    Bliss = 0x0025,
    Kotlin = 0x0026,
    Zig = 0x0027,
    Crystal = 0x0028,
    CPlusPlus17 = 0x002a,
    CPlusPlus20 = 0x002b,
    C17 = 0x002c,
    Fortran18 = 0x002d,
    Ada2005 = 0x002e,
    Ada2012 = 0x002f,
    /// Custom language for QuantaLang
    QuantaLang = 0x8001,
}

/// DWARF line number opcodes (DW_LNS_*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DwarfLineOpcode {
    Copy = 0x01,
    AdvancePc = 0x02,
    AdvanceLine = 0x03,
    SetFile = 0x04,
    SetColumn = 0x05,
    NegateStmt = 0x06,
    SetBasicBlock = 0x07,
    ConstAddPc = 0x08,
    FixedAdvancePc = 0x09,
    SetPrologueEnd = 0x0a,
    SetEpilogueBegin = 0x0b,
    SetIsa = 0x0c,
}

/// DWARF extended line number opcodes (DW_LNE_*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DwarfLineExtOpcode {
    EndSequence = 0x01,
    SetAddress = 0x02,
    DefineFile = 0x03,
    SetDiscriminator = 0x04,
}

/// DWARF operation codes for location expressions (DW_OP_*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DwarfOp {
    Addr = 0x03,
    Deref = 0x06,
    Const1u = 0x08,
    Const1s = 0x09,
    Const2u = 0x0a,
    Const2s = 0x0b,
    Const4u = 0x0c,
    Const4s = 0x0d,
    Const8u = 0x0e,
    Const8s = 0x0f,
    Constu = 0x10,
    Consts = 0x11,
    Dup = 0x12,
    Drop = 0x13,
    Over = 0x14,
    Pick = 0x15,
    Swap = 0x16,
    Rot = 0x17,
    Xderef = 0x18,
    Abs = 0x19,
    And = 0x1a,
    Div = 0x1b,
    Minus = 0x1c,
    Mod = 0x1d,
    Mul = 0x1e,
    Neg = 0x1f,
    Not = 0x20,
    Or = 0x21,
    Plus = 0x22,
    PlusUconst = 0x23,
    Shl = 0x24,
    Shr = 0x25,
    Shra = 0x26,
    Xor = 0x27,
    Bra = 0x28,
    Eq = 0x29,
    Ge = 0x2a,
    Gt = 0x2b,
    Le = 0x2c,
    Lt = 0x2d,
    Ne = 0x2e,
    Skip = 0x2f,
    Lit0 = 0x30,
    // Lit1..Lit31 = 0x31..0x4f
    Reg0 = 0x50,
    // Reg1..Reg31 = 0x51..0x6f
    Breg0 = 0x70,
    // Breg1..Breg31 = 0x71..0x8f
    Regx = 0x90,
    Fbreg = 0x91,
    Bregx = 0x92,
    Piece = 0x93,
    DerefSize = 0x94,
    XderefSize = 0x95,
    Nop = 0x96,
    PushObjectAddress = 0x97,
    Call2 = 0x98,
    Call4 = 0x99,
    CallRef = 0x9a,
    FormTlsAddress = 0x9b,
    CallFrameCfa = 0x9c,
    BitPiece = 0x9d,
    ImplicitValue = 0x9e,
    StackValue = 0x9f,
    ImplicitPointer = 0xa0,
    Addrx = 0xa1,
    Constx = 0xa2,
    EntryValue = 0xa3,
    ConstType = 0xa4,
    RegvalType = 0xa5,
    DerefType = 0xa6,
    XderefType = 0xa7,
    Convert = 0xa8,
    Reinterpret = 0xa9,
}

/// DWARF call frame instructions (DW_CFA_*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DwarfCfa {
    Nop = 0x00,
    SetLoc = 0x01,
    AdvanceLoc1 = 0x02,
    AdvanceLoc2 = 0x03,
    AdvanceLoc4 = 0x04,
    OffsetExtended = 0x05,
    RestoreExtended = 0x06,
    Undefined = 0x07,
    SameValue = 0x08,
    Register = 0x09,
    RememberState = 0x0a,
    RestoreState = 0x0b,
    DefCfa = 0x0c,
    DefCfaRegister = 0x0d,
    DefCfaOffset = 0x0e,
    DefCfaExpression = 0x0f,
    Expression = 0x10,
    OffsetExtendedSf = 0x11,
    DefCfaSf = 0x12,
    DefCfaOffsetSf = 0x13,
    ValOffset = 0x14,
    ValOffsetSf = 0x15,
    ValExpression = 0x16,
    // High 2 bits encode instruction type
    AdvanceLoc = 0x40, // 01xxxxxx
    Offset = 0x80,     // 10xxxxxx
    Restore = 0xc0,    // 11xxxxxx
}

// =============================================================================
// DWARF Debug Info Builder
// =============================================================================

/// Entry in debug info.
#[derive(Debug, Clone)]
pub struct DebugInfoEntry {
    /// DWARF tag.
    pub tag: DwarfTag,
    /// Attributes.
    pub attrs: Vec<(DwarfAttr, DwarfAttrValue)>,
    /// Children entries.
    pub children: Vec<DebugInfoEntry>,
}

impl DebugInfoEntry {
    /// Create a new DIE.
    pub fn new(tag: DwarfTag) -> Self {
        Self {
            tag,
            attrs: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Add an attribute.
    pub fn add_attr(&mut self, attr: DwarfAttr, value: DwarfAttrValue) {
        self.attrs.push((attr, value));
    }

    /// Add a child entry.
    pub fn add_child(&mut self, child: DebugInfoEntry) {
        self.children.push(child);
    }

    /// Check if this entry has children.
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }
}

/// DWARF attribute value.
#[derive(Debug, Clone)]
pub enum DwarfAttrValue {
    /// Address value.
    Addr(u64),
    /// String value.
    String(String),
    /// Reference to string table.
    StringRef(u32),
    /// 1-byte unsigned value.
    Data1(u8),
    /// 2-byte unsigned value.
    Data2(u16),
    /// 4-byte unsigned value.
    Data4(u32),
    /// 8-byte unsigned value.
    Data8(u64),
    /// Signed LEB128.
    Sdata(i64),
    /// Unsigned LEB128.
    Udata(u64),
    /// Flag (boolean).
    Flag(bool),
    /// Reference to another DIE.
    Ref4(u32),
    /// Section offset.
    SecOffset(u32),
    /// Expression location.
    ExprLoc(Vec<u8>),
    /// Block of data.
    Block(Vec<u8>),
}

/// Source file information.
#[derive(Debug, Clone)]
pub struct SourceFile {
    /// File name.
    pub name: String,
    /// Directory index (0 = compilation directory).
    pub dir_index: u32,
    /// Last modification time (0 if unknown).
    pub mod_time: u64,
    /// File size (0 if unknown).
    pub size: u64,
}

/// Line number entry.
#[derive(Debug, Clone)]
pub struct LineEntry {
    /// Code address.
    pub address: u64,
    /// Source file index.
    pub file: u32,
    /// Line number.
    pub line: u32,
    /// Column number.
    pub column: u32,
    /// Is statement boundary.
    pub is_stmt: bool,
    /// Is basic block start.
    pub basic_block: bool,
    /// Is prologue end.
    pub prologue_end: bool,
    /// Is epilogue begin.
    pub epilogue_begin: bool,
    /// Instruction set architecture.
    pub isa: u8,
    /// Discriminator for same source location.
    pub discriminator: u32,
}

impl LineEntry {
    /// Create a new line entry.
    pub fn new(address: u64, file: u32, line: u32, column: u32) -> Self {
        Self {
            address,
            file,
            line,
            column,
            is_stmt: true,
            basic_block: false,
            prologue_end: false,
            epilogue_begin: false,
            isa: 0,
            discriminator: 0,
        }
    }
}

/// Address range entry.
#[derive(Debug, Clone)]
pub struct AddressRange {
    /// Start address.
    pub start: u64,
    /// Length in bytes.
    pub length: u64,
}

/// DWARF debug info generator.
pub struct DwarfGenerator {
    /// DWARF version (4 or 5).
    pub version: u8,
    /// Address size in bytes.
    pub address_size: u8,
    /// Producer string.
    pub producer: String,
    /// Compilation directory.
    pub comp_dir: String,
    /// Source files.
    pub files: Vec<SourceFile>,
    /// Directories.
    pub directories: Vec<String>,
    /// Line number entries.
    pub line_entries: Vec<LineEntry>,
    /// Root DIE (compile unit).
    pub root: Option<DebugInfoEntry>,
    /// String table.
    pub string_table: Vec<String>,
    /// String to offset mapping.
    string_offsets: HashMap<String, u32>,
    /// Type DIE cache.
    type_cache: HashMap<String, u32>,
    /// Next type offset.
    next_type_offset: u32,
}

impl DwarfGenerator {
    /// Create a new DWARF generator.
    pub fn new() -> Self {
        Self {
            version: 4,
            address_size: 8,
            producer: "QuantaLang Compiler 1.0".to_string(),
            comp_dir: String::new(),
            files: Vec::new(),
            directories: Vec::new(),
            line_entries: Vec::new(),
            root: None,
            string_table: Vec::new(),
            string_offsets: HashMap::new(),
            type_cache: HashMap::new(),
            next_type_offset: 0,
        }
    }

    /// Set DWARF version.
    pub fn set_version(&mut self, version: u8) {
        self.version = version;
    }

    /// Set producer string.
    pub fn set_producer(&mut self, producer: &str) {
        self.producer = producer.to_string();
    }

    /// Set compilation directory.
    pub fn set_comp_dir(&mut self, dir: &str) {
        self.comp_dir = dir.to_string();
    }

    /// Add a source file.
    pub fn add_file(&mut self, name: &str, dir_index: u32) -> u32 {
        let idx = self.files.len() as u32;
        self.files.push(SourceFile {
            name: name.to_string(),
            dir_index,
            mod_time: 0,
            size: 0,
        });
        idx
    }

    /// Add a directory.
    pub fn add_directory(&mut self, dir: &str) -> u32 {
        let idx = self.directories.len() as u32;
        self.directories.push(dir.to_string());
        idx
    }

    /// Add a line number entry.
    pub fn add_line(&mut self, entry: LineEntry) {
        self.line_entries.push(entry);
    }

    /// Intern a string and return its offset.
    pub fn intern_string(&mut self, s: &str) -> u32 {
        if let Some(&offset) = self.string_offsets.get(s) {
            return offset;
        }
        let offset = self.string_table.iter().map(|s| s.len() + 1).sum::<usize>() as u32;
        self.string_offsets.insert(s.to_string(), offset);
        self.string_table.push(s.to_string());
        offset
    }

    /// Build debug info from MIR module.
    pub fn build_from_mir(&mut self, mir: &MirModule) {
        // Create compile unit DIE
        let mut cu = DebugInfoEntry::new(DwarfTag::CompileUnit);
        cu.add_attr(
            DwarfAttr::Producer,
            DwarfAttrValue::String(self.producer.clone()),
        );
        cu.add_attr(
            DwarfAttr::Language,
            DwarfAttrValue::Data2(DwarfLang::QuantaLang as u16),
        );
        cu.add_attr(
            DwarfAttr::Name,
            DwarfAttrValue::String(mir.name.to_string()),
        );
        cu.add_attr(
            DwarfAttr::CompDir,
            DwarfAttrValue::String(self.comp_dir.clone()),
        );
        cu.add_attr(DwarfAttr::UseUtf8, DwarfAttrValue::Flag(true));

        // Add base types
        self.add_base_types(&mut cu);

        // Add global variables
        for global in &mir.globals {
            let var = self.create_variable_die(global);
            cu.add_child(var);
        }

        // Add functions
        for func in &mir.functions {
            if !func.is_declaration() {
                let subprog = self.create_subprogram_die(func);
                cu.add_child(subprog);
            }
        }

        self.root = Some(cu);
    }

    /// Add base type DIEs.
    fn add_base_types(&mut self, cu: &mut DebugInfoEntry) {
        let base_types = [
            ("void", 0, DwarfEncoding::Unsigned),
            ("bool", 1, DwarfEncoding::Boolean),
            ("i8", 1, DwarfEncoding::Signed),
            ("i16", 2, DwarfEncoding::Signed),
            ("i32", 4, DwarfEncoding::Signed),
            ("i64", 8, DwarfEncoding::Signed),
            ("i128", 16, DwarfEncoding::Signed),
            ("u8", 1, DwarfEncoding::Unsigned),
            ("u16", 2, DwarfEncoding::Unsigned),
            ("u32", 4, DwarfEncoding::Unsigned),
            ("u64", 8, DwarfEncoding::Unsigned),
            ("u128", 16, DwarfEncoding::Unsigned),
            ("f32", 4, DwarfEncoding::Float),
            ("f64", 8, DwarfEncoding::Float),
            ("char", 4, DwarfEncoding::Utf),
        ];

        for (name, size, encoding) in base_types {
            let mut ty = DebugInfoEntry::new(DwarfTag::BaseType);
            ty.add_attr(DwarfAttr::Name, DwarfAttrValue::String(name.to_string()));
            ty.add_attr(DwarfAttr::ByteSize, DwarfAttrValue::Data1(size));
            ty.add_attr(DwarfAttr::Encoding, DwarfAttrValue::Data1(encoding as u8));
            self.type_cache
                .insert(name.to_string(), self.next_type_offset);
            self.next_type_offset += 1;
            cu.add_child(ty);
        }
    }

    /// Create a variable DIE.
    fn create_variable_die(&self, global: &MirGlobal) -> DebugInfoEntry {
        let mut var = DebugInfoEntry::new(DwarfTag::Variable);
        var.add_attr(
            DwarfAttr::Name,
            DwarfAttrValue::String(global.name.to_string()),
        );
        var.add_attr(
            DwarfAttr::External,
            DwarfAttrValue::Flag(global.linkage == crate::codegen::ir::Linkage::External),
        );

        // Location expression (address)
        let mut loc = Vec::new();
        loc.push(DwarfOp::Addr as u8);
        loc.extend_from_slice(&0u64.to_le_bytes()); // Will be fixed up by linker
        var.add_attr(DwarfAttr::Location, DwarfAttrValue::ExprLoc(loc));

        var
    }

    /// Create a subprogram DIE.
    fn create_subprogram_die(&self, func: &MirFunction) -> DebugInfoEntry {
        let mut subprog = DebugInfoEntry::new(DwarfTag::Subprogram);
        subprog.add_attr(
            DwarfAttr::Name,
            DwarfAttrValue::String(func.name.to_string()),
        );
        subprog.add_attr(DwarfAttr::External, DwarfAttrValue::Flag(func.is_public));
        subprog.add_attr(DwarfAttr::Prototyped, DwarfAttrValue::Flag(true));

        // Add formal parameters
        for local in &func.locals {
            if local.is_param {
                let mut param = DebugInfoEntry::new(DwarfTag::FormalParameter);
                if let Some(name) = &local.name {
                    param.add_attr(DwarfAttr::Name, DwarfAttrValue::String(name.to_string()));
                }
                subprog.add_child(param);
            }
        }

        // Add local variables
        for local in &func.locals {
            if !local.is_param {
                let mut var = DebugInfoEntry::new(DwarfTag::Variable);
                if let Some(name) = &local.name {
                    var.add_attr(DwarfAttr::Name, DwarfAttrValue::String(name.to_string()));
                }
                // Frame-relative location
                let mut loc = Vec::new();
                loc.push(DwarfOp::Fbreg as u8);
                loc.extend_from_slice(&encode_sleb128(local.id.0 as i64 * -8));
                var.add_attr(DwarfAttr::Location, DwarfAttrValue::ExprLoc(loc));
                subprog.add_child(var);
            }
        }

        subprog
    }

    /// Generate .debug_info section.
    pub fn generate_debug_info(&self) -> Vec<u8> {
        let mut data = Vec::new();

        if let Some(ref root) = self.root {
            // Write compile unit header
            let unit_length_pos = data.len();
            data.extend_from_slice(&0u32.to_le_bytes()); // Placeholder for unit length
            data.extend_from_slice(&(self.version as u16).to_le_bytes());

            if self.version >= 5 {
                data.push(0x01); // DW_UT_compile
                data.push(self.address_size);
                data.extend_from_slice(&0u32.to_le_bytes()); // debug_abbrev_offset
            } else {
                data.extend_from_slice(&0u32.to_le_bytes()); // debug_abbrev_offset
                data.push(self.address_size);
            }

            // Write DIE tree
            let _start = data.len();
            self.write_die(&mut data, root, 1);
            data.push(0); // End of children

            // Fix up unit length
            let unit_length = (data.len() - unit_length_pos - 4) as u32;
            data[unit_length_pos..unit_length_pos + 4].copy_from_slice(&unit_length.to_le_bytes());
        }

        data
    }

    /// Write a DIE to the buffer.
    fn write_die(&self, data: &mut Vec<u8>, die: &DebugInfoEntry, abbrev_code: u32) {
        // Abbreviation code
        data.extend_from_slice(&encode_uleb128(abbrev_code as u64));

        // Attributes
        for (_, value) in &die.attrs {
            self.write_attr_value(data, value);
        }

        // Children
        for child in &die.children {
            self.write_die(data, child, abbrev_code + 1);
        }

        if die.has_children() {
            data.push(0); // End of children
        }
    }

    /// Write an attribute value.
    fn write_attr_value(&self, data: &mut Vec<u8>, value: &DwarfAttrValue) {
        match value {
            DwarfAttrValue::Addr(v) => data.extend_from_slice(&v.to_le_bytes()),
            DwarfAttrValue::String(s) => {
                data.extend_from_slice(s.as_bytes());
                data.push(0);
            }
            DwarfAttrValue::StringRef(offset) => data.extend_from_slice(&offset.to_le_bytes()),
            DwarfAttrValue::Data1(v) => data.push(*v),
            DwarfAttrValue::Data2(v) => data.extend_from_slice(&v.to_le_bytes()),
            DwarfAttrValue::Data4(v) => data.extend_from_slice(&v.to_le_bytes()),
            DwarfAttrValue::Data8(v) => data.extend_from_slice(&v.to_le_bytes()),
            DwarfAttrValue::Sdata(v) => data.extend_from_slice(&encode_sleb128(*v)),
            DwarfAttrValue::Udata(v) => data.extend_from_slice(&encode_uleb128(*v)),
            DwarfAttrValue::Flag(b) => data.push(if *b { 1 } else { 0 }),
            DwarfAttrValue::Ref4(v) => data.extend_from_slice(&v.to_le_bytes()),
            DwarfAttrValue::SecOffset(v) => data.extend_from_slice(&v.to_le_bytes()),
            DwarfAttrValue::ExprLoc(bytes) => {
                data.extend_from_slice(&encode_uleb128(bytes.len() as u64));
                data.extend_from_slice(bytes);
            }
            DwarfAttrValue::Block(bytes) => {
                data.extend_from_slice(&encode_uleb128(bytes.len() as u64));
                data.extend_from_slice(bytes);
            }
        }
    }

    /// Generate .debug_abbrev section.
    pub fn generate_debug_abbrev(&self) -> Vec<u8> {
        let mut data = Vec::new();

        if let Some(ref root) = self.root {
            self.write_abbrev(&mut data, root, 1);
        }

        data.push(0); // End of abbreviations
        data
    }

    /// Write abbreviation for a DIE.
    fn write_abbrev(&self, data: &mut Vec<u8>, die: &DebugInfoEntry, abbrev_code: u32) {
        // Abbreviation code
        data.extend_from_slice(&encode_uleb128(abbrev_code as u64));
        // Tag
        data.extend_from_slice(&encode_uleb128(die.tag as u64));
        // Has children
        data.push(if die.has_children() { 0x01 } else { 0x00 });

        // Attributes
        for (attr, value) in &die.attrs {
            data.extend_from_slice(&encode_uleb128(*attr as u64));
            data.extend_from_slice(&encode_uleb128(self.form_for_value(value) as u64));
        }
        data.extend_from_slice(&[0, 0]); // End of attributes

        // Recurse for children
        for child in &die.children {
            self.write_abbrev(data, child, abbrev_code + 1);
        }
    }

    /// Get the form for an attribute value.
    fn form_for_value(&self, value: &DwarfAttrValue) -> DwarfForm {
        match value {
            DwarfAttrValue::Addr(_) => DwarfForm::Addr,
            DwarfAttrValue::String(_) => DwarfForm::String,
            DwarfAttrValue::StringRef(_) => DwarfForm::Strp,
            DwarfAttrValue::Data1(_) => DwarfForm::Data1,
            DwarfAttrValue::Data2(_) => DwarfForm::Data2,
            DwarfAttrValue::Data4(_) => DwarfForm::Data4,
            DwarfAttrValue::Data8(_) => DwarfForm::Data8,
            DwarfAttrValue::Sdata(_) => DwarfForm::Sdata,
            DwarfAttrValue::Udata(_) => DwarfForm::Udata,
            DwarfAttrValue::Flag(_) => DwarfForm::Flag,
            DwarfAttrValue::Ref4(_) => DwarfForm::Ref4,
            DwarfAttrValue::SecOffset(_) => DwarfForm::SecOffset,
            DwarfAttrValue::ExprLoc(_) => DwarfForm::ExprLoc,
            DwarfAttrValue::Block(_) => DwarfForm::Block,
        }
    }

    /// Generate .debug_line section.
    pub fn generate_debug_line(&self) -> Vec<u8> {
        let mut data = Vec::new();

        // Line number program header
        let unit_length_pos = data.len();
        data.extend_from_slice(&0u32.to_le_bytes()); // Placeholder

        data.extend_from_slice(&(self.version as u16).to_le_bytes());

        if self.version >= 5 {
            data.push(self.address_size);
            data.push(0); // Segment selector size
        }

        let header_length_pos = data.len();
        data.extend_from_slice(&0u32.to_le_bytes()); // Header length placeholder

        let header_start = data.len();

        // Standard opcode lengths
        let min_instruction_length = 1u8;
        let max_ops_per_instruction = 1u8;
        let default_is_stmt = 1u8;
        let line_base: i8 = -5;
        let line_range = 14u8;
        let opcode_base = 13u8;

        data.push(min_instruction_length);
        data.push(max_ops_per_instruction);
        data.push(default_is_stmt);
        data.push(line_base as u8);
        data.push(line_range);
        data.push(opcode_base);

        // Standard opcode lengths
        let std_opcode_lengths = [0u8, 1, 1, 1, 1, 0, 0, 0, 1, 0, 0, 1];
        for &len in &std_opcode_lengths {
            data.push(len);
        }

        if self.version >= 5 {
            // DWARF 5 directory/file format
            // Directory entry format count
            data.push(1);
            data.extend_from_slice(&encode_uleb128(DwarfAttr::Name as u64));
            data.extend_from_slice(&encode_uleb128(DwarfForm::String as u64));

            // Directory count
            data.extend_from_slice(&encode_uleb128(self.directories.len() as u64 + 1));
            // Compilation directory
            data.extend_from_slice(self.comp_dir.as_bytes());
            data.push(0);
            // Other directories
            for dir in &self.directories {
                data.extend_from_slice(dir.as_bytes());
                data.push(0);
            }

            // File entry format count
            data.push(2);
            data.extend_from_slice(&encode_uleb128(DwarfAttr::Name as u64));
            data.extend_from_slice(&encode_uleb128(DwarfForm::String as u64));
            data.extend_from_slice(&encode_uleb128(DwarfForm::Data1 as u64)); // dir index form
            data.extend_from_slice(&encode_uleb128(DwarfForm::Udata as u64));

            // File count
            data.extend_from_slice(&encode_uleb128(self.files.len() as u64));
            for file in &self.files {
                data.extend_from_slice(file.name.as_bytes());
                data.push(0);
                data.extend_from_slice(&encode_uleb128(file.dir_index as u64));
            }
        } else {
            // DWARF 4 directory/file format
            // Directories (null-terminated list)
            for dir in &self.directories {
                data.extend_from_slice(dir.as_bytes());
                data.push(0);
            }
            data.push(0); // End of directories

            // Files (null-terminated list)
            for file in &self.files {
                data.extend_from_slice(file.name.as_bytes());
                data.push(0);
                data.extend_from_slice(&encode_uleb128(file.dir_index as u64));
                data.extend_from_slice(&encode_uleb128(file.mod_time));
                data.extend_from_slice(&encode_uleb128(file.size));
            }
            data.push(0); // End of files
        }

        // Fix up header length
        let header_length = (data.len() - header_start) as u32;
        data[header_length_pos..header_length_pos + 4]
            .copy_from_slice(&header_length.to_le_bytes());

        // Line number program
        self.write_line_program(&mut data, line_base, line_range, opcode_base);

        // Fix up unit length
        let unit_length = (data.len() - unit_length_pos - 4) as u32;
        data[unit_length_pos..unit_length_pos + 4].copy_from_slice(&unit_length.to_le_bytes());

        data
    }

    /// Write the line number program.
    fn write_line_program(
        &self,
        data: &mut Vec<u8>,
        line_base: i8,
        line_range: u8,
        opcode_base: u8,
    ) {
        let mut current_addr = 0u64;
        let mut current_file = 1u32;
        let mut current_line = 1u32;
        let mut current_column = 0u32;

        for entry in &self.line_entries {
            // Set address
            if entry.address != current_addr {
                data.push(0); // Extended opcode
                let addr_bytes = if self.address_size == 8 { 9 } else { 5 };
                data.extend_from_slice(&encode_uleb128(addr_bytes as u64));
                data.push(DwarfLineExtOpcode::SetAddress as u8);
                if self.address_size == 8 {
                    data.extend_from_slice(&entry.address.to_le_bytes());
                } else {
                    data.extend_from_slice(&(entry.address as u32).to_le_bytes());
                }
                current_addr = entry.address;
            }

            // Set file
            if entry.file != current_file {
                data.push(DwarfLineOpcode::SetFile as u8);
                data.extend_from_slice(&encode_uleb128(entry.file as u64));
                current_file = entry.file;
            }

            // Set column
            if entry.column != current_column {
                data.push(DwarfLineOpcode::SetColumn as u8);
                data.extend_from_slice(&encode_uleb128(entry.column as u64));
                current_column = entry.column;
            }

            // Advance line
            let line_diff = entry.line as i64 - current_line as i64;
            if line_diff != 0 {
                // Try special opcode
                let adjusted = line_diff - line_base as i64;
                if adjusted >= 0 && adjusted < line_range as i64 {
                    let special = (adjusted as u8) + opcode_base;
                    data.push(special);
                } else {
                    data.push(DwarfLineOpcode::AdvanceLine as u8);
                    data.extend_from_slice(&encode_sleb128(line_diff));
                    data.push(DwarfLineOpcode::Copy as u8);
                }
            } else {
                data.push(DwarfLineOpcode::Copy as u8);
            }

            current_line = entry.line;
        }

        // End sequence
        data.push(0);
        data.push(1);
        data.push(DwarfLineExtOpcode::EndSequence as u8);
    }

    /// Generate .debug_str section.
    pub fn generate_debug_str(&self) -> Vec<u8> {
        let mut data = Vec::new();
        for s in &self.string_table {
            data.extend_from_slice(s.as_bytes());
            data.push(0);
        }
        data
    }

    /// Generate .debug_aranges section.
    pub fn generate_debug_aranges(&self, ranges: &[AddressRange]) -> Vec<u8> {
        let mut data = Vec::new();

        // Header
        let unit_length_pos = data.len();
        data.extend_from_slice(&0u32.to_le_bytes()); // Placeholder
        data.extend_from_slice(&2u16.to_le_bytes()); // Version
        data.extend_from_slice(&0u32.to_le_bytes()); // debug_info offset
        data.push(self.address_size);
        data.push(0); // Segment size

        // Align to 2x address size
        let align = (self.address_size * 2) as usize;
        while data.len() % align != 0 {
            data.push(0);
        }

        // Address ranges
        for range in ranges {
            if self.address_size == 8 {
                data.extend_from_slice(&range.start.to_le_bytes());
                data.extend_from_slice(&range.length.to_le_bytes());
            } else {
                data.extend_from_slice(&(range.start as u32).to_le_bytes());
                data.extend_from_slice(&(range.length as u32).to_le_bytes());
            }
        }

        // Terminator
        if self.address_size == 8 {
            data.extend_from_slice(&0u64.to_le_bytes());
            data.extend_from_slice(&0u64.to_le_bytes());
        } else {
            data.extend_from_slice(&0u32.to_le_bytes());
            data.extend_from_slice(&0u32.to_le_bytes());
        }

        // Fix up unit length
        let unit_length = (data.len() - unit_length_pos - 4) as u32;
        data[unit_length_pos..unit_length_pos + 4].copy_from_slice(&unit_length.to_le_bytes());

        data
    }

    /// Generate .debug_frame section (call frame information).
    pub fn generate_debug_frame(&self, funcs: &[(String, u64, u64)]) -> Vec<u8> {
        let mut data = Vec::new();

        // Common Information Entry (CIE)
        let cie_length_pos = data.len();
        data.extend_from_slice(&0u32.to_le_bytes()); // Length placeholder
        data.extend_from_slice(&0xffffffffu32.to_le_bytes()); // CIE_id
        data.push(4); // Version
        data.push(0); // Augmentation (empty string)
        data.push(self.address_size); // Address size
        data.push(0); // Segment size
        data.extend_from_slice(&encode_uleb128(1)); // Code alignment factor
        data.extend_from_slice(&encode_sleb128(-8)); // Data alignment factor
        data.extend_from_slice(&encode_uleb128(16)); // Return address register (x86-64: rip)

        // Initial instructions
        // def_cfa rsp, 8
        data.push(DwarfCfa::DefCfa as u8);
        data.extend_from_slice(&encode_uleb128(7)); // rsp
        data.extend_from_slice(&encode_uleb128(8));
        // offset rip, -8
        data.push(DwarfCfa::Offset as u8 | 16); // rip
        data.extend_from_slice(&encode_uleb128(1)); // -8 / -8 = 1

        // Align to pointer size
        while (data.len() - cie_length_pos - 4) % (self.address_size as usize) != 0 {
            data.push(DwarfCfa::Nop as u8);
        }

        // Fix up CIE length
        let cie_length = (data.len() - cie_length_pos - 4) as u32;
        data[cie_length_pos..cie_length_pos + 4].copy_from_slice(&cie_length.to_le_bytes());

        let cie_offset = 0u32;

        // Frame Description Entries (FDEs)
        for (_name, start, length) in funcs {
            let fde_length_pos = data.len();
            data.extend_from_slice(&0u32.to_le_bytes()); // Length placeholder
            data.extend_from_slice(&cie_offset.to_le_bytes()); // CIE pointer

            // Initial location
            if self.address_size == 8 {
                data.extend_from_slice(&start.to_le_bytes());
                data.extend_from_slice(&length.to_le_bytes());
            } else {
                data.extend_from_slice(&(*start as u32).to_le_bytes());
                data.extend_from_slice(&(*length as u32).to_le_bytes());
            }

            // Call frame instructions for function
            // advance_loc 1: push rbp
            data.push(DwarfCfa::AdvanceLoc as u8 | 1);
            // def_cfa_offset 16
            data.push(DwarfCfa::DefCfaOffset as u8);
            data.extend_from_slice(&encode_uleb128(16));
            // offset rbp, -16
            data.push(DwarfCfa::Offset as u8 | 6); // rbp
            data.extend_from_slice(&encode_uleb128(2)); // -16 / -8 = 2

            // advance_loc 3: mov rbp, rsp
            data.push(DwarfCfa::AdvanceLoc as u8 | 3);
            // def_cfa_register rbp
            data.push(DwarfCfa::DefCfaRegister as u8);
            data.extend_from_slice(&encode_uleb128(6)); // rbp

            // Align to pointer size
            while (data.len() - fde_length_pos - 4) % (self.address_size as usize) != 0 {
                data.push(DwarfCfa::Nop as u8);
            }

            // Fix up FDE length
            let fde_length = (data.len() - fde_length_pos - 4) as u32;
            data[fde_length_pos..fde_length_pos + 4].copy_from_slice(&fde_length.to_le_bytes());
        }

        data
    }

    /// Generate assembly directives for debug sections.
    pub fn generate_assembly(&self) -> String {
        let mut output = String::new();

        writeln!(output, "# DWARF Debug Information").unwrap();
        writeln!(output, "# Generated by QuantaLang Compiler").unwrap();
        writeln!(output).unwrap();

        // .debug_info section
        writeln!(output, ".section .debug_info,\"\",@progbits").unwrap();
        let info = self.generate_debug_info();
        self.emit_bytes(&mut output, &info);

        // .debug_abbrev section
        writeln!(output, "\n.section .debug_abbrev,\"\",@progbits").unwrap();
        let abbrev = self.generate_debug_abbrev();
        self.emit_bytes(&mut output, &abbrev);

        // .debug_line section
        writeln!(output, "\n.section .debug_line,\"\",@progbits").unwrap();
        let line = self.generate_debug_line();
        self.emit_bytes(&mut output, &line);

        // .debug_str section
        if !self.string_table.is_empty() {
            writeln!(output, "\n.section .debug_str,\"MS\",@progbits,1").unwrap();
            for s in &self.string_table {
                writeln!(output, "    .asciz \"{}\"", escape_string(s)).unwrap();
            }
        }

        output
    }

    /// Emit bytes as assembly directives.
    fn emit_bytes(&self, output: &mut String, bytes: &[u8]) {
        for chunk in bytes.chunks(16) {
            write!(output, "    .byte ").unwrap();
            for (i, b) in chunk.iter().enumerate() {
                if i > 0 {
                    write!(output, ", ").unwrap();
                }
                write!(output, "0x{:02x}", b).unwrap();
            }
            writeln!(output).unwrap();
        }
    }
}

impl Default for DwarfGenerator {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Encode an unsigned LEB128 value.
pub fn encode_uleb128(mut value: u64) -> Vec<u8> {
    let mut result = Vec::new();
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        result.push(byte);
        if value == 0 {
            break;
        }
    }
    result
}

/// Encode a signed LEB128 value.
pub fn encode_sleb128(mut value: i64) -> Vec<u8> {
    let mut result = Vec::new();
    let _negative = value < 0;
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        let done = (value == 0 && (byte & 0x40) == 0) || (value == -1 && (byte & 0x40) != 0);
        if !done {
            byte |= 0x80;
        }
        result.push(byte);
        if done {
            break;
        }
    }
    result
}

/// Decode an unsigned LEB128 value.
pub fn decode_uleb128(bytes: &[u8]) -> (u64, usize) {
    let mut result = 0u64;
    let mut shift = 0;
    let mut count = 0;
    for &byte in bytes {
        count += 1;
        result |= ((byte & 0x7f) as u64) << shift;
        if (byte & 0x80) == 0 {
            break;
        }
        shift += 7;
    }
    (result, count)
}

/// Decode a signed LEB128 value.
pub fn decode_sleb128(bytes: &[u8]) -> (i64, usize) {
    let mut result = 0i64;
    let mut shift = 0;
    let mut count = 0;
    let mut byte = 0u8;
    for &b in bytes {
        byte = b;
        count += 1;
        result |= ((byte & 0x7f) as i64) << shift;
        shift += 7;
        if (byte & 0x80) == 0 {
            break;
        }
    }
    // Sign extend
    if shift < 64 && (byte & 0x40) != 0 {
        result |= !0i64 << shift;
    }
    (result, count)
}

/// Escape a string for assembly output.
fn escape_string(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\\' => result.push_str("\\\\"),
            '"' => result.push_str("\\\""),
            c if c.is_ascii_control() => {
                result.push_str(&format!("\\{:03o}", c as u8));
            }
            c => result.push(c),
        }
    }
    result
}

// =============================================================================
// DWARF Expression Builder
// =============================================================================

/// Builder for DWARF location expressions.
///
/// Location expressions describe how to compute the location of a variable
/// or the value of an expression using a stack-based virtual machine.
#[derive(Debug, Clone, Default)]
pub struct DwarfExprBuilder {
    /// The expression bytes.
    bytes: Vec<u8>,
}

impl DwarfExprBuilder {
    /// Create a new expression builder.
    pub fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    /// Consume the builder and return the expression bytes.
    pub fn build(self) -> Vec<u8> {
        self.bytes
    }

    /// Get the current length of the expression.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Check if the expression is empty.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Push a raw byte.
    fn push(&mut self, byte: u8) {
        self.bytes.push(byte);
    }

    /// Push multiple bytes.
    fn extend(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    // =========================================================================
    // Literal Encodings
    // =========================================================================

    /// Push an address (DW_OP_addr).
    pub fn addr(mut self, address: u64) -> Self {
        self.push(DwarfOp::Addr as u8);
        self.extend(&address.to_le_bytes());
        self
    }

    /// Push a small literal (0-31) (DW_OP_lit0..DW_OP_lit31).
    pub fn lit(mut self, value: u8) -> Self {
        assert!(value < 32, "lit value must be 0-31");
        self.push(DwarfOp::Lit0 as u8 + value);
        self
    }

    /// Push an unsigned 1-byte constant (DW_OP_const1u).
    pub fn const1u(mut self, value: u8) -> Self {
        self.push(DwarfOp::Const1u as u8);
        self.push(value);
        self
    }

    /// Push a signed 1-byte constant (DW_OP_const1s).
    pub fn const1s(mut self, value: i8) -> Self {
        self.push(DwarfOp::Const1s as u8);
        self.push(value as u8);
        self
    }

    /// Push an unsigned 2-byte constant (DW_OP_const2u).
    pub fn const2u(mut self, value: u16) -> Self {
        self.push(DwarfOp::Const2u as u8);
        self.extend(&value.to_le_bytes());
        self
    }

    /// Push a signed 2-byte constant (DW_OP_const2s).
    pub fn const2s(mut self, value: i16) -> Self {
        self.push(DwarfOp::Const2s as u8);
        self.extend(&value.to_le_bytes());
        self
    }

    /// Push an unsigned 4-byte constant (DW_OP_const4u).
    pub fn const4u(mut self, value: u32) -> Self {
        self.push(DwarfOp::Const4u as u8);
        self.extend(&value.to_le_bytes());
        self
    }

    /// Push a signed 4-byte constant (DW_OP_const4s).
    pub fn const4s(mut self, value: i32) -> Self {
        self.push(DwarfOp::Const4s as u8);
        self.extend(&value.to_le_bytes());
        self
    }

    /// Push an unsigned 8-byte constant (DW_OP_const8u).
    pub fn const8u(mut self, value: u64) -> Self {
        self.push(DwarfOp::Const8u as u8);
        self.extend(&value.to_le_bytes());
        self
    }

    /// Push a signed 8-byte constant (DW_OP_const8s).
    pub fn const8s(mut self, value: i64) -> Self {
        self.push(DwarfOp::Const8s as u8);
        self.extend(&value.to_le_bytes());
        self
    }

    /// Push an unsigned LEB128 constant (DW_OP_constu).
    pub fn constu(mut self, value: u64) -> Self {
        self.push(DwarfOp::Constu as u8);
        self.extend(&encode_uleb128(value));
        self
    }

    /// Push a signed LEB128 constant (DW_OP_consts).
    pub fn consts(mut self, value: i64) -> Self {
        self.push(DwarfOp::Consts as u8);
        self.extend(&encode_sleb128(value));
        self
    }

    // =========================================================================
    // Register Locations
    // =========================================================================

    /// Register location (0-31) (DW_OP_reg0..DW_OP_reg31).
    pub fn reg(mut self, reg: u8) -> Self {
        assert!(reg < 32, "reg must be 0-31");
        self.push(DwarfOp::Reg0 as u8 + reg);
        self
    }

    /// Register location (any register) (DW_OP_regx).
    pub fn regx(mut self, reg: u64) -> Self {
        self.push(DwarfOp::Regx as u8);
        self.extend(&encode_uleb128(reg));
        self
    }

    /// Register-relative location (0-31) (DW_OP_breg0..DW_OP_breg31).
    pub fn breg(mut self, reg: u8, offset: i64) -> Self {
        assert!(reg < 32, "breg must be 0-31");
        self.push(DwarfOp::Breg0 as u8 + reg);
        self.extend(&encode_sleb128(offset));
        self
    }

    /// Register-relative location (any register) (DW_OP_bregx).
    pub fn bregx(mut self, reg: u64, offset: i64) -> Self {
        self.push(DwarfOp::Bregx as u8);
        self.extend(&encode_uleb128(reg));
        self.extend(&encode_sleb128(offset));
        self
    }

    /// Frame base relative (DW_OP_fbreg).
    pub fn fbreg(mut self, offset: i64) -> Self {
        self.push(DwarfOp::Fbreg as u8);
        self.extend(&encode_sleb128(offset));
        self
    }

    // =========================================================================
    // Stack Operations
    // =========================================================================

    /// Duplicate top of stack (DW_OP_dup).
    pub fn dup(mut self) -> Self {
        self.push(DwarfOp::Dup as u8);
        self
    }

    /// Drop top of stack (DW_OP_drop).
    pub fn drop(mut self) -> Self {
        self.push(DwarfOp::Drop as u8);
        self
    }

    /// Copy second item to top (DW_OP_over).
    pub fn over(mut self) -> Self {
        self.push(DwarfOp::Over as u8);
        self
    }

    /// Pick item at index (DW_OP_pick).
    pub fn pick(mut self, index: u8) -> Self {
        self.push(DwarfOp::Pick as u8);
        self.push(index);
        self
    }

    /// Swap top two items (DW_OP_swap).
    pub fn swap(mut self) -> Self {
        self.push(DwarfOp::Swap as u8);
        self
    }

    /// Rotate top three items (DW_OP_rot).
    pub fn rot(mut self) -> Self {
        self.push(DwarfOp::Rot as u8);
        self
    }

    // =========================================================================
    // Arithmetic/Logical Operations
    // =========================================================================

    /// Absolute value (DW_OP_abs).
    pub fn abs(mut self) -> Self {
        self.push(DwarfOp::Abs as u8);
        self
    }

    /// Bitwise AND (DW_OP_and).
    pub fn and(mut self) -> Self {
        self.push(DwarfOp::And as u8);
        self
    }

    /// Division (DW_OP_div).
    pub fn div(mut self) -> Self {
        self.push(DwarfOp::Div as u8);
        self
    }

    /// Subtraction (DW_OP_minus).
    pub fn minus(mut self) -> Self {
        self.push(DwarfOp::Minus as u8);
        self
    }

    /// Modulo (DW_OP_mod).
    pub fn mod_(mut self) -> Self {
        self.push(DwarfOp::Mod as u8);
        self
    }

    /// Multiplication (DW_OP_mul).
    pub fn mul(mut self) -> Self {
        self.push(DwarfOp::Mul as u8);
        self
    }

    /// Negation (DW_OP_neg).
    pub fn neg(mut self) -> Self {
        self.push(DwarfOp::Neg as u8);
        self
    }

    /// Bitwise NOT (DW_OP_not).
    pub fn not(mut self) -> Self {
        self.push(DwarfOp::Not as u8);
        self
    }

    /// Bitwise OR (DW_OP_or).
    pub fn or(mut self) -> Self {
        self.push(DwarfOp::Or as u8);
        self
    }

    /// Addition (DW_OP_plus).
    pub fn plus(mut self) -> Self {
        self.push(DwarfOp::Plus as u8);
        self
    }

    /// Add unsigned constant (DW_OP_plus_uconst).
    pub fn plus_uconst(mut self, value: u64) -> Self {
        self.push(DwarfOp::PlusUconst as u8);
        self.extend(&encode_uleb128(value));
        self
    }

    /// Left shift (DW_OP_shl).
    pub fn shl(mut self) -> Self {
        self.push(DwarfOp::Shl as u8);
        self
    }

    /// Logical right shift (DW_OP_shr).
    pub fn shr(mut self) -> Self {
        self.push(DwarfOp::Shr as u8);
        self
    }

    /// Arithmetic right shift (DW_OP_shra).
    pub fn shra(mut self) -> Self {
        self.push(DwarfOp::Shra as u8);
        self
    }

    /// Bitwise XOR (DW_OP_xor).
    pub fn xor(mut self) -> Self {
        self.push(DwarfOp::Xor as u8);
        self
    }

    // =========================================================================
    // Comparison Operations
    // =========================================================================

    /// Equal (DW_OP_eq).
    pub fn eq(mut self) -> Self {
        self.push(DwarfOp::Eq as u8);
        self
    }

    /// Greater or equal (DW_OP_ge).
    pub fn ge(mut self) -> Self {
        self.push(DwarfOp::Ge as u8);
        self
    }

    /// Greater than (DW_OP_gt).
    pub fn gt(mut self) -> Self {
        self.push(DwarfOp::Gt as u8);
        self
    }

    /// Less or equal (DW_OP_le).
    pub fn le(mut self) -> Self {
        self.push(DwarfOp::Le as u8);
        self
    }

    /// Less than (DW_OP_lt).
    pub fn lt(mut self) -> Self {
        self.push(DwarfOp::Lt as u8);
        self
    }

    /// Not equal (DW_OP_ne).
    pub fn ne(mut self) -> Self {
        self.push(DwarfOp::Ne as u8);
        self
    }

    // =========================================================================
    // Memory Operations
    // =========================================================================

    /// Dereference (DW_OP_deref).
    pub fn deref(mut self) -> Self {
        self.push(DwarfOp::Deref as u8);
        self
    }

    /// Dereference with size (DW_OP_deref_size).
    pub fn deref_size(mut self, size: u8) -> Self {
        self.push(DwarfOp::DerefSize as u8);
        self.push(size);
        self
    }

    // =========================================================================
    // Composite Locations
    // =========================================================================

    /// Piece of specified size (DW_OP_piece).
    pub fn piece(mut self, size: u64) -> Self {
        self.push(DwarfOp::Piece as u8);
        self.extend(&encode_uleb128(size));
        self
    }

    /// Bit piece (DW_OP_bit_piece).
    pub fn bit_piece(mut self, bit_size: u64, bit_offset: u64) -> Self {
        self.push(DwarfOp::BitPiece as u8);
        self.extend(&encode_uleb128(bit_size));
        self.extend(&encode_uleb128(bit_offset));
        self
    }

    // =========================================================================
    // Special Operations
    // =========================================================================

    /// Value is on stack, not at a memory location (DW_OP_stack_value).
    pub fn stack_value(mut self) -> Self {
        self.push(DwarfOp::StackValue as u8);
        self
    }

    /// No operation (DW_OP_nop).
    pub fn nop(mut self) -> Self {
        self.push(DwarfOp::Nop as u8);
        self
    }

    /// Call frame address (DW_OP_call_frame_cfa).
    pub fn call_frame_cfa(mut self) -> Self {
        self.push(DwarfOp::CallFrameCfa as u8);
        self
    }

    /// Implicit value (DW_OP_implicit_value).
    pub fn implicit_value(mut self, value: &[u8]) -> Self {
        self.push(DwarfOp::ImplicitValue as u8);
        self.extend(&encode_uleb128(value.len() as u64));
        self.extend(value);
        self
    }

    /// Entry value (DW_OP_entry_value) - value at function entry.
    pub fn entry_value(mut self, expr: &[u8]) -> Self {
        self.push(DwarfOp::EntryValue as u8);
        self.extend(&encode_uleb128(expr.len() as u64));
        self.extend(expr);
        self
    }
}

/// Common location expressions for x86-64.
pub mod x86_64_locations {
    use super::DwarfExprBuilder;

    /// RAX register (register 0).
    pub fn rax() -> DwarfExprBuilder {
        DwarfExprBuilder::new().reg(0)
    }

    /// RDX register (register 1).
    pub fn rdx() -> DwarfExprBuilder {
        DwarfExprBuilder::new().reg(1)
    }

    /// RCX register (register 2).
    pub fn rcx() -> DwarfExprBuilder {
        DwarfExprBuilder::new().reg(2)
    }

    /// RBX register (register 3).
    pub fn rbx() -> DwarfExprBuilder {
        DwarfExprBuilder::new().reg(3)
    }

    /// RSI register (register 4).
    pub fn rsi() -> DwarfExprBuilder {
        DwarfExprBuilder::new().reg(4)
    }

    /// RDI register (register 5).
    pub fn rdi() -> DwarfExprBuilder {
        DwarfExprBuilder::new().reg(5)
    }

    /// RBP register (register 6).
    pub fn rbp() -> DwarfExprBuilder {
        DwarfExprBuilder::new().reg(6)
    }

    /// RSP register (register 7).
    pub fn rsp() -> DwarfExprBuilder {
        DwarfExprBuilder::new().reg(7)
    }

    /// Stack location relative to RBP.
    pub fn stack_var(offset: i64) -> DwarfExprBuilder {
        DwarfExprBuilder::new().breg(6, offset) // RBP + offset
    }

    /// Stack location relative to RSP.
    pub fn stack_arg(offset: i64) -> DwarfExprBuilder {
        DwarfExprBuilder::new().breg(7, offset) // RSP + offset
    }

    /// Location relative to frame base.
    pub fn frame_var(offset: i64) -> DwarfExprBuilder {
        DwarfExprBuilder::new().fbreg(offset)
    }
}

/// Common location expressions for AArch64.
pub mod aarch64_locations {
    use super::DwarfExprBuilder;

    /// X0-X30 registers.
    pub fn xreg(n: u8) -> DwarfExprBuilder {
        assert!(n <= 30, "x register must be 0-30");
        DwarfExprBuilder::new().reg(n)
    }

    /// Frame pointer (X29).
    pub fn fp() -> DwarfExprBuilder {
        DwarfExprBuilder::new().reg(29)
    }

    /// Link register (X30).
    pub fn lr() -> DwarfExprBuilder {
        DwarfExprBuilder::new().reg(30)
    }

    /// Stack pointer (register 31).
    pub fn sp() -> DwarfExprBuilder {
        DwarfExprBuilder::new().reg(31)
    }

    /// Stack location relative to frame pointer.
    pub fn stack_var(offset: i64) -> DwarfExprBuilder {
        DwarfExprBuilder::new().breg(29, offset) // FP + offset
    }

    /// Location relative to frame base.
    pub fn frame_var(offset: i64) -> DwarfExprBuilder {
        DwarfExprBuilder::new().fbreg(offset)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_uleb128() {
        assert_eq!(encode_uleb128(0), vec![0x00]);
        assert_eq!(encode_uleb128(1), vec![0x01]);
        assert_eq!(encode_uleb128(127), vec![0x7f]);
        assert_eq!(encode_uleb128(128), vec![0x80, 0x01]);
        assert_eq!(encode_uleb128(129), vec![0x81, 0x01]);
        assert_eq!(encode_uleb128(16383), vec![0xff, 0x7f]);
        assert_eq!(encode_uleb128(16384), vec![0x80, 0x80, 0x01]);
    }

    #[test]
    fn test_encode_sleb128() {
        assert_eq!(encode_sleb128(0), vec![0x00]);
        assert_eq!(encode_sleb128(1), vec![0x01]);
        assert_eq!(encode_sleb128(-1), vec![0x7f]);
        assert_eq!(encode_sleb128(63), vec![0x3f]);
        assert_eq!(encode_sleb128(64), vec![0xc0, 0x00]);
        assert_eq!(encode_sleb128(-64), vec![0x40]);
        assert_eq!(encode_sleb128(-65), vec![0xbf, 0x7f]);
    }

    #[test]
    fn test_decode_uleb128() {
        assert_eq!(decode_uleb128(&[0x00]), (0, 1));
        assert_eq!(decode_uleb128(&[0x01]), (1, 1));
        assert_eq!(decode_uleb128(&[0x7f]), (127, 1));
        assert_eq!(decode_uleb128(&[0x80, 0x01]), (128, 2));
        assert_eq!(decode_uleb128(&[0x81, 0x01]), (129, 2));
    }

    #[test]
    fn test_decode_sleb128() {
        assert_eq!(decode_sleb128(&[0x00]), (0, 1));
        assert_eq!(decode_sleb128(&[0x01]), (1, 1));
        assert_eq!(decode_sleb128(&[0x7f]), (-1, 1));
        assert_eq!(decode_sleb128(&[0x40]), (-64, 1));
        assert_eq!(decode_sleb128(&[0xbf, 0x7f]), (-65, 2));
    }

    #[test]
    fn test_dwarf_generator_new() {
        let gen = DwarfGenerator::new();
        assert_eq!(gen.version, 4);
        assert_eq!(gen.address_size, 8);
        assert!(gen.files.is_empty());
        assert!(gen.line_entries.is_empty());
    }

    #[test]
    fn test_dwarf_generator_add_file() {
        let mut gen = DwarfGenerator::new();
        let idx = gen.add_file("main.qta", 0);
        assert_eq!(idx, 0);
        assert_eq!(gen.files.len(), 1);
        assert_eq!(gen.files[0].name, "main.qta");
    }

    #[test]
    fn test_dwarf_generator_add_directory() {
        let mut gen = DwarfGenerator::new();
        let idx = gen.add_directory("/src");
        assert_eq!(idx, 0);
        assert_eq!(gen.directories.len(), 1);
        assert_eq!(gen.directories[0], "/src");
    }

    #[test]
    fn test_dwarf_generator_intern_string() {
        let mut gen = DwarfGenerator::new();
        let offset1 = gen.intern_string("hello");
        let offset2 = gen.intern_string("world");
        let offset3 = gen.intern_string("hello"); // Should return same offset
        assert_eq!(offset1, 0);
        assert_eq!(offset2, 6); // "hello\0" = 6 bytes
        assert_eq!(offset3, offset1);
    }

    #[test]
    fn test_line_entry_new() {
        let entry = LineEntry::new(0x1000, 1, 10, 5);
        assert_eq!(entry.address, 0x1000);
        assert_eq!(entry.file, 1);
        assert_eq!(entry.line, 10);
        assert_eq!(entry.column, 5);
        assert!(entry.is_stmt);
    }

    #[test]
    fn test_debug_info_entry() {
        let mut die = DebugInfoEntry::new(DwarfTag::Subprogram);
        die.add_attr(DwarfAttr::Name, DwarfAttrValue::String("main".to_string()));
        die.add_attr(DwarfAttr::External, DwarfAttrValue::Flag(true));

        let mut param = DebugInfoEntry::new(DwarfTag::FormalParameter);
        param.add_attr(DwarfAttr::Name, DwarfAttrValue::String("argc".to_string()));
        die.add_child(param);

        assert_eq!(die.tag, DwarfTag::Subprogram);
        assert_eq!(die.attrs.len(), 2);
        assert!(die.has_children());
    }

    #[test]
    fn test_generate_debug_info() {
        let mut gen = DwarfGenerator::new();
        gen.set_producer("Test Producer");
        gen.set_comp_dir("/test");

        let mut cu = DebugInfoEntry::new(DwarfTag::CompileUnit);
        cu.add_attr(
            DwarfAttr::Name,
            DwarfAttrValue::String("test.qta".to_string()),
        );
        gen.root = Some(cu);

        let info = gen.generate_debug_info();
        assert!(!info.is_empty());
        // Check version
        assert_eq!(info[4], 4); // DWARF version 4
    }

    #[test]
    fn test_generate_debug_abbrev() {
        let mut gen = DwarfGenerator::new();
        let mut cu = DebugInfoEntry::new(DwarfTag::CompileUnit);
        cu.add_attr(DwarfAttr::Name, DwarfAttrValue::String("test".to_string()));
        gen.root = Some(cu);

        let abbrev = gen.generate_debug_abbrev();
        assert!(!abbrev.is_empty());
        // Should end with terminator
        assert_eq!(*abbrev.last().unwrap(), 0);
    }

    #[test]
    fn test_generate_debug_line() {
        let mut gen = DwarfGenerator::new();
        gen.add_file("test.qta", 0);
        gen.add_line(LineEntry::new(0x1000, 1, 1, 0));
        gen.add_line(LineEntry::new(0x1010, 1, 2, 0));

        let line = gen.generate_debug_line();
        assert!(!line.is_empty());
    }

    #[test]
    fn test_generate_debug_str() {
        let mut gen = DwarfGenerator::new();
        gen.intern_string("hello");
        gen.intern_string("world");

        let str_section = gen.generate_debug_str();
        assert_eq!(str_section, b"hello\0world\0");
    }

    #[test]
    fn test_generate_debug_aranges() {
        let gen = DwarfGenerator::new();
        let ranges = vec![
            AddressRange {
                start: 0x1000,
                length: 0x100,
            },
            AddressRange {
                start: 0x2000,
                length: 0x200,
            },
        ];

        let aranges = gen.generate_debug_aranges(&ranges);
        assert!(!aranges.is_empty());
    }

    #[test]
    fn test_generate_debug_frame() {
        let gen = DwarfGenerator::new();
        let funcs = vec![
            ("main".to_string(), 0x1000u64, 0x100u64),
            ("foo".to_string(), 0x2000u64, 0x50u64),
        ];

        let frame = gen.generate_debug_frame(&funcs);
        assert!(!frame.is_empty());
    }

    #[test]
    fn test_dwarf_tag_values() {
        assert_eq!(DwarfTag::CompileUnit as u16, 0x11);
        assert_eq!(DwarfTag::Subprogram as u16, 0x2e);
        assert_eq!(DwarfTag::Variable as u16, 0x34);
        assert_eq!(DwarfTag::BaseType as u16, 0x24);
    }

    #[test]
    fn test_dwarf_attr_values() {
        assert_eq!(DwarfAttr::Name as u16, 0x03);
        assert_eq!(DwarfAttr::Type as u16, 0x49);
        assert_eq!(DwarfAttr::LowPc as u16, 0x11);
        assert_eq!(DwarfAttr::HighPc as u16, 0x12);
    }

    #[test]
    fn test_dwarf_form_values() {
        assert_eq!(DwarfForm::Addr as u8, 0x01);
        assert_eq!(DwarfForm::String as u8, 0x08);
        assert_eq!(DwarfForm::Data4 as u8, 0x06);
        assert_eq!(DwarfForm::ExprLoc as u8, 0x18);
    }

    #[test]
    fn test_dwarf_encoding_values() {
        assert_eq!(DwarfEncoding::Boolean as u8, 0x02);
        assert_eq!(DwarfEncoding::Signed as u8, 0x05);
        assert_eq!(DwarfEncoding::Unsigned as u8, 0x07);
        assert_eq!(DwarfEncoding::Float as u8, 0x04);
    }

    #[test]
    fn test_dwarf_lang_quantalang() {
        assert_eq!(DwarfLang::QuantaLang as u16, 0x8001);
    }

    #[test]
    fn test_escape_string() {
        assert_eq!(escape_string("hello"), "hello");
        assert_eq!(escape_string("hello\nworld"), "hello\\nworld");
        assert_eq!(escape_string("tab\there"), "tab\\there");
        assert_eq!(escape_string("quote\"test"), "quote\\\"test");
    }

    #[test]
    fn test_address_range() {
        let range = AddressRange {
            start: 0x401000,
            length: 0x1000,
        };
        assert_eq!(range.start, 0x401000);
        assert_eq!(range.length, 0x1000);
    }

    #[test]
    fn test_source_file() {
        let file = SourceFile {
            name: "main.qta".to_string(),
            dir_index: 0,
            mod_time: 0,
            size: 1024,
        };
        assert_eq!(file.name, "main.qta");
        assert_eq!(file.dir_index, 0);
        assert_eq!(file.size, 1024);
    }
}
