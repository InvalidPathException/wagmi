use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Malformed(&'static str),
    Validation(&'static str),
    Trap(&'static str),
    Link(&'static str),
    Uninstantiable(&'static str),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Malformed(s)
            | Error::Validation(s)
            | Error::Trap(s)
            | Error::Link(s)
            | Error::Uninstantiable(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for Error {}

// Malformed errors
pub const END_OPCODE_EXPECTED: &str = "END opcode expected";
pub const FUNC_CODE_INCONSISTENT: &str = "function and code section have inconsistent lengths";
pub const ILLEGAL_OPCODE: &str = "illegal opcode";
pub const INT_TOO_LARGE: &str = "integer too large";
pub const INT_TOO_LONG: &str = "integer representation too long";
pub const INVALID_GLOBAL_TYPE: &str = "invalid global type";
pub const INVALID_MUTABILITY: &str = "invalid mutability";
pub const INVALID_SECTION_ID: &str = "invalid section id";
pub const INVALID_UTF8: &str = "invalid UTF-8 encoding";
pub const INVALID_VALUE_TYPE: &str = "invalid value type";
pub const JUNK_AFTER_LAST_SECTION: &str = "junk after last section";
pub const LENGTH_OUT_OF_BOUNDS: &str = "length out of bounds";
pub const MAGIC_HEADER_NOT_DETECTED: &str = "magic header not detected";
pub const MALFORMED_IMPORT_KIND: &str = "malformed import kind";
pub const MALFORMED_REFERENCE_TYPE: &str = "malformed reference type";
pub const SECTION_SIZE_MISMATCH: &str = "section size mismatch";
pub const TOO_MANY_LOCALS: &str = "too many locals";
pub const UNEXPECTED_END: &str = "unexpected end of section or function";
pub const UNEXPECTED_END_SHORT: &str = "unexpected end";
pub const UNKNOWN_BINARY_VERSION: &str = "unknown binary version";
pub const UNKNOWN_INSTRUCTION: &str = "unknown instruction";
pub const ZERO_FLAG_EXPECTED: &str = "zero flag expected";
// Validation errors
pub const ALIGNMENT_TOO_LARGE: &str = "alignment must not be larger than natural";
pub const CONST_EXP_REQUIRED: &str = "constant expression required";
pub const DUPLICATE_EXPORT_NAME: &str = "duplicate export name";
pub const ELSE_MUST_CLOSE_IF: &str = "else must close an if";
pub const GLOBAL_IS_IMMUTABLE: &str = "global is immutable";
pub const INVALID_DATA_SEGMENT_FLAG: &str = "invalid data segment flag";
pub const INVALID_EXPORT_DESC: &str = "invalid export description";
pub const INVALID_LOCAL_TYPE: &str = "invalid local type";
pub const INVALID_PARAMETER_TYPE: &str = "invalid parameter type";
pub const INVALID_RESULT_ARITY: &str = "invalid result arity";
pub const INVALID_RESULT_TYPE: &str = "invalid result type";
pub const INVALID_TABLE_ELEM_TYPE: &str = "invalid table element type";
pub const MEMORY_SIZE_LIMIT: &str = "memory size must be at most 65536 pages (4GiB)";
pub const MULTIPLE_MEMORIES: &str = "multiple memories";
pub const MULTIPLE_TABLES: &str = "multiple tables";
pub const SIZE_MIN_GREATER_THAN_MAX: &str = "size minimum must not be greater than maximum";
pub const START_FUNC: &str = "start function";
pub const TYPE_MISMATCH: &str = "type mismatch";
pub const UNKNOWN_FUNC: &str = "unknown function";
pub const UNKNOWN_GLOBAL: &str = "unknown global";
pub const UNKNOWN_LABEL: &str = "unknown label";
pub const UNKNOWN_LOCAL: &str = "unknown local";
pub const UNKNOWN_MEMORY: &str = "unknown memory";
pub const UNKNOWN_TABLE: &str = "unknown table";
pub const UNKNOWN_TYPE: &str = "unknown type";
// Trap errors
pub const CALL_STACK_EXHAUSTED: &str = "call stack exhausted";
pub const FUNC_NO_IMPL: &str = "function has no implementation";
pub const INDIRECT_CALL_TYPE_MISMATCH: &str = "indirect call type mismatch";
pub const STACK_UNDERFLOW: &str = "stack underflow";
pub const UNDEF_ELEM: &str = "undefined element";
pub const UNINITIALIZED_ELEM: &str = "uninitialized element";
pub const UNREACHABLE: &str = "unreachable";
// Link errors
pub const DATA_SEGMENT_DOES_NOT_FIT: &str = "data segment does not fit";
pub const ELEM_SEGMENT_DOES_NOT_FIT: &str = "elements segment does not fit";
pub const INCOMPATIBLE_IMPORT_TYPE: &str = "incompatible import type";
pub const UNKNOWN_IMPORT: &str = "unknown import";