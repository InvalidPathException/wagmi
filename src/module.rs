use std::collections::HashMap;
use std::rc::Rc;

use crate::byte_iter::*;
use crate::error::Error::*;
use crate::error::*;
use crate::leb128::*;
use crate::signature::*;
use crate::validator::{validate_const, Validator};

const MAGIC_HEADER: &[u8; 4] = b"\0asm";

// ---------------- Import/Export related ----------------
#[derive(Clone, Debug)]
pub struct ImportRef { pub module: String, pub field: String }

#[derive(Clone, Copy)]
pub enum ExternType { Func = 0, Table = 1, Mem = 2, Global = 3 }

impl ExternType {
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(ExternType::Func),
            1 => Some(ExternType::Table),
            2 => Some(ExternType::Mem),
            3 => Some(ExternType::Global),
            _ => None,
        }
    }
}

// ---------------- Structures ----------------
#[derive(Clone)]
pub struct Function {
    pub body: std::ops::Range<usize>,
    pub ty: Signature,
    pub locals: Vec<ValType>,
    pub import: Option<ImportRef>,
    pub is_declared: bool,
}

#[derive(Clone)]
pub struct Table {
    pub min: u32,
    pub max: u32,
    pub import: Option<ImportRef>
}

#[derive(Clone)]
pub struct Memory {
    pub min: u32,
    pub max: u32,
    pub import: Option<ImportRef>
}

#[derive(Clone)]
pub struct Global {
    pub ty: ValType,
    pub is_mutable: bool,
    pub initializer_offset: usize,
    pub import: Option<ImportRef>
}

#[derive(Clone)]
pub struct Export { pub extern_type: ExternType, pub idx: u32 }

#[derive(Clone)]
pub struct DataSegment { pub data_range: std::ops::Range<usize>, pub initializer_offset: usize }

#[derive(Clone, Copy)]
pub struct IfJump { pub else_offset: usize, pub end_offset: usize }

// ---------------- Module Structure ----------------
#[derive(Default)]
pub struct Module {
    pub bytes: Rc<Vec<u8>>,
    pub types: Vec<Signature>,
    pub imports: HashMap<String, HashMap<String, ExternType>>,
    pub table: Option<Table>,
    pub memory: Option<Memory>,
    pub globals: Vec<Global>,
    pub exports: HashMap<String, Export>,
    pub start: u32,
    pub element_start: usize,
    pub element_count: u32,
    pub functions: Vec<Function>,
    pub n_data: u32,
    pub data_segments: Vec<DataSegment>,
    pub if_jumps: HashMap<usize, IfJump>,
    pub block_ends: HashMap<usize, usize>,
}

impl Module {
    pub const MAX_PAGES: u32 = 65536;
    pub const MAX_LOCALS: usize = 50000;

    pub fn compile(bytes: Vec<u8>) -> Result<Self, Error> {
        // Other than bytecode and default start cursor, everything starts as empty/None
        let mut m = Module {
            bytes: Rc::new(bytes),
            start: u32::MAX,
            ..Default::default()
        };
        m.initialize()?;
        Ok(m)
    }

    fn initialize(&mut self) -> Result<(), Error> {
        // Copy to get around borrow checker
        let bytes: &[u8] = &self.bytes.clone()[..];
        
        // Check magic number and version
        if bytes.len() < 4 { return Err(Malformed(UNEXPECTED_END_SHORT)); }
        if &bytes[0..4] != MAGIC_HEADER {
            return Err(Malformed(NO_MAGIC_HEADER));
        }
        
        let mut it = ByteIter::new(bytes, 4);
        if bytes.len() < 8 { return Err(Malformed(UNEXPECTED_END_SHORT)); }
        if u32::from_le_bytes(bytes[4..8].try_into().unwrap()) != 1 {
            return Err(Malformed(UNKNOWN_BINARY_VERSION));
        }
        it.advance(4);

        section(&mut it, bytes, 1, |it: &mut ByteIter| { self.parse_type_section(bytes, it) })?;
        section(&mut it, bytes, 2, |it: &mut ByteIter| { self.parse_import_section(bytes, it) })?;
        section(&mut it, bytes, 3, |it: &mut ByteIter| { self.parse_function_section(bytes, it) })?;
        section(&mut it, bytes, 4, |it: &mut ByteIter| { self.parse_table_section(bytes, it) })?;
        section(&mut it, bytes, 5, |it: &mut ByteIter| { self.parse_memory_section(bytes, it) })?;
        section(&mut it, bytes, 6, |it: &mut ByteIter| { self.parse_global_section(bytes, it) })?;
        section(&mut it, bytes, 7, |it: &mut ByteIter| { self.parse_export_section(bytes, it) })?;
        section(&mut it, bytes, 8, |it: &mut ByteIter| { self.parse_start_section(bytes, it) })?;
        section(&mut it, bytes, 9, |it: &mut ByteIter| { self.parse_element_section(bytes, it) })?;
        section(&mut it, bytes, 10, |it: &mut ByteIter| { self.parse_code_section(bytes, it) })?;
        section(&mut it, bytes, 11, |it: &mut ByteIter| { self.parse_data_section(bytes, it) })?;

        // Check that all non-imported functions have code
        for func in &self.functions {
            if func.import.is_none() && func.body.start == 0 && func.body.end == 0 {
                return Err(Malformed(FUNC_CODE_INCONSISTENT));
            }
        }

        if !it.empty() { return Err(Malformed(LENGTH_OUT_OF_BOUNDS)); }
        Ok(())
    }

    fn parse_type_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        let n_types: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
        self.types.reserve_exact(n_types as usize);

        for _i in 0..n_types as usize {
            if it.empty() { return Err(Malformed(UNEXPECTED_END)); }
            let byte = it.read_u8()?;
            if byte != 0x60 {
                return Err(Malformed(INT_TOO_LONG));
            }

            let n_params: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
            let mut sig = Signature::default();
            sig.params.reserve_exact(n_params as usize);

            for _ in 0..n_params {
                let ty = it.read_u8()?;
                if !is_val_type(ty) {
                    return Err(Malformed(INVALID_VALUE_TYPE));
                }
                sig.params.push(val_type_from_byte(ty).unwrap());
            }

            let n_results: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
            if n_results > 1 {
                return Err(Validation(INVALID_RESULT_ARITY));
            }
            if n_results == 1 {
                let ty = it.read_u8()?;
                if !is_val_type(ty) {
                    return Err(Malformed(INVALID_RESULT_TYPE));
                }
                sig.result = Some(val_type_from_byte(ty).unwrap());
            }

            self.types.push(sig);
        }

        Ok(())
    }

    fn parse_import_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        let n_imports: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;

        for _ in 0..n_imports {
            if it.empty() { return Err(Malformed(UNEXPECTED_END)); }

            // Module name
            let module_len: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
            let module_start = it.idx;
            if module_start + module_len as usize > bytes.len() {
                return Err(Malformed(UNEXPECTED_END));
            }
            let module_name = String::from_utf8(bytes[module_start..module_start + module_len as usize].to_vec())
                .map_err(|_| Malformed(INVALID_UTF8))?;
            it.idx = module_start + module_len as usize;

            // Field name
            let field_len: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
            let field_start = it.idx;
            if field_start + field_len as usize > bytes.len() {
                return Err(Malformed(UNEXPECTED_END));
            }
            let field_name = String::from_utf8(bytes[field_start..field_start + field_len as usize].to_vec())
                .map_err(|_| Malformed(INVALID_UTF8))?;
            it.idx = field_start + field_len as usize;

            let extern_type = ExternType::from_byte(it.read_u8()?)
                .ok_or(Malformed(MALFORMED_IMPORT_KIND))?;

            self.imports.entry(module_name.clone()).or_default().insert(field_name.clone(), extern_type);
            let import = Some(ImportRef {
                module: module_name,
                field: field_name
            });

            match extern_type {
                ExternType::Func => {
                    let type_idx: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
                    if (type_idx as usize) >= self.types.len() {
                        return Err(Validation(UNKNOWN_TYPE));
                    }
                    self.functions.push(Function {
                        body: 0..0,
                        ty: self.types[type_idx as usize].clone(),
                        locals: vec![],
                        import,
                        is_declared: false
                    });
                }
                ExternType::Table => {
                    if self.table.is_some() {
                        return Err(Validation(MULTIPLE_TABLES));
                    }
                    // Only 0x70 in 1.0 MVP
                    let reftype: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
                    if reftype != 0x70 {
                        return Err(Malformed(MALFORMED_REF_TYPE));
                    }
                    let (min, max) = get_table_limits(bytes, it)?;
                    self.table = Some(Table { min, max, import });
                }
                ExternType::Mem => {
                    if self.memory.is_some() {
                        return Err(Validation(MULTIPLE_MEMORIES));
                    }
                    let (min, max) = get_memory_limits(bytes, it)?;
                    self.memory = Some(Memory { min, max, import });
                }
                ExternType::Global => {
                    let ty: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
                    if !is_val_type(ty as u8) {
                        return Err(Malformed(INVALID_GLOBAL_TYPE));
                    }
                    let mut_byte = it.read_u8()?;
                    let is_mutable = mutability_from_byte(mut_byte)?;
                    self.globals.push(Global {
                        ty: val_type_from_byte(ty as u8).unwrap(),
                        is_mutable,
                        initializer_offset: 0,
                        import
                    });
                }
            }
        }
        Ok(())
    }

    fn parse_function_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        let n_functions: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
        self.functions.reserve(n_functions as usize);

        for _ in 0..n_functions {
            if it.empty() { return Err(Malformed(UNEXPECTED_END)); }
            let type_idx: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
            if (type_idx as usize) >= self.types.len() {
                return Err(Validation(UNKNOWN_TYPE));
            }
            self.functions.push(Function {
                body: 0..0,
                ty: self.types[type_idx as usize].clone(),
                locals: vec![],
                import: None,
                is_declared: false
            });
        }
        Ok(())
    }

    fn parse_table_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        let n_tables: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
        if n_tables > 1 || (n_tables == 1 && self.table.is_some()) {
            return Err(Validation(MULTIPLE_TABLES));
        }

        if n_tables == 1 {
            if it.empty() { return Err(Malformed(UNEXPECTED_END)); }
            let elem_type = it.read_u8()?;
            if elem_type != 0x70 {
                return Err(Validation(INVALID_ELEM_TYPE));
            }
            let (min, max) = get_table_limits(bytes, it)?;
            self.table = Some(Table { min, max, import: None });
        }
        Ok(())
    }

    fn parse_memory_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        let n_memories: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
        if n_memories > 1 || (n_memories == 1 && self.memory.is_some()) {
            return Err(Validation(MULTIPLE_MEMORIES));
        }

        if n_memories == 1 {
            if it.empty() { return Err(Malformed(UNEXPECTED_END)); }
            let (min, max) = get_memory_limits(bytes, it)?;
            self.memory = Some(Memory { min, max, import: None });
        }
        Ok(())
    }

    fn parse_global_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        let n_globals: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;

        for _ in 0..n_globals {
            if it.empty() { return Err(Malformed(UNEXPECTED_END)); }
            let ty = it.read_u8()?;
            if !is_val_type(ty) {
                return Err(Malformed(INVALID_GLOBAL_TYPE));
            }
            let mut_byte = it.read_u8()?;
            let is_mutable = mutability_from_byte(mut_byte)?;
            let initializer_offset = it.cur();
            self.globals.push(Global {
                ty: val_type_from_byte(ty).unwrap(),
                is_mutable,
                initializer_offset,
                import: None
            });
            validate_const(bytes, it, val_type_from_byte(ty).unwrap(), &self.globals)?;
        }
        Ok(())
    }

    fn parse_export_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        let n_exports: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;

        for _ in 0..n_exports {
            if it.empty() { return Err(Malformed(UNEXPECTED_END)); }

            let name_len: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
            let name_start = it.idx;
            if name_start + name_len as usize > bytes.len() {
                return Err(Malformed(UNEXPECTED_END));
            }
            let name = String::from_utf8(bytes[name_start..name_start + name_len as usize].to_vec()).unwrap();
            it.idx = name_start + name_len as usize;

            let byte = it.read_u8()?;
            let extern_type = ExternType::from_byte(byte)
                .ok_or(Validation(INVALID_EXPORT_DESC))?;

            let export_idx: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;

            if self.exports.contains_key(&name) {
                return Err(Validation(DUP_EXPORT_NAME));
            }

            match extern_type {
                ExternType::Func => {
                    if (export_idx as usize) >= self.functions.len() {
                        return Err(Validation(UNKNOWN_FUNC));
                    }
                    self.functions[export_idx as usize].is_declared = true;
                }
                ExternType::Table => {
                    if export_idx != 0 {
                        return Err(Validation(UNKNOWN_TABLE));
                    }
                }
                ExternType::Mem => {
                    if export_idx != 0 || self.memory.is_none() {
                        return Err(Validation(UNKNOWN_MEMORY));
                    }
                }
                ExternType::Global => {
                    if (export_idx as usize) >= self.globals.len() {
                        return Err(Validation(UNKNOWN_GLOBAL));
                    }
                }
            }

            self.exports.insert(name, Export {
                extern_type,
                idx: export_idx
            });
        }
        Ok(())
    }

    fn parse_start_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        let start: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
        if (start as usize) >= self.functions.len() {
            return Err(Validation(UNKNOWN_FUNC));
        }
        self.start = start;
        Ok(())
    }

    fn parse_element_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        let n_elements: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
        self.element_start = it.cur();
        self.element_count = n_elements;

        for _ in 0..n_elements {
            if it.empty() { return Err(Malformed(UNEXPECTED_END)); }
            let flags: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
            if flags != 0 {
                return Err(Malformed(INVALID_VALUE_TYPE));
            }
            if self.table.is_none() {
                return Err(Validation(UNKNOWN_TABLE));
            }
            validate_const(bytes, it, ValType::I32, &self.globals)?;

            let n_elems: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
            for _ in 0..n_elems {
                let elem_idx: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
                if (elem_idx as usize) >= self.functions.len() {
                    return Err(Validation(UNKNOWN_FUNC));
                }
                self.functions[elem_idx as usize].is_declared = true;
            }
        }
        Ok(())
    }

    fn parse_code_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        let n_functions: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
        let n_imports = self.functions.iter().filter(|f| f.import.is_some()).count() as u32;
        if (n_functions + n_imports) as usize != self.functions.len() {
            return Err(Malformed(FUNC_CODE_INCONSISTENT));
        }

        for i in 0..self.functions.len() {
            if self.functions[i].import.is_some() {
                continue;
            }

            // Initialize locals with params
            self.functions[i].locals = self.functions[i].ty.params.clone();

            let function_length: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
            let func_start = it.cur();

            // Parse local declarations
            let mut n_local_decls: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
            while n_local_decls > 0 {
                n_local_decls -= 1;
                let n_locals: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
                let ty = it.read_u8()?;
                if !is_val_type(ty) {
                    return Err(Validation(INVALID_LOCAL_TYPE));
                }
                for _ in 0..n_locals {
                    let vt = val_type_from_byte(ty).unwrap();
                    let function = &mut self.functions[i];
                    function.locals.push(vt);
                    if function.locals.len() > Module::MAX_LOCALS {
                        return Err(Malformed(TOO_MANY_LOCALS));
                    }
                }
            }

            let body_start = it.cur();
            let body_length = function_length as usize - (body_start - func_start);
            let body_end_expected = body_start + body_length;

            self.functions[i].body = body_start..body_end_expected;

            // Validate function body immediately
            Validator::new(self).validate_function(i)?;
            // Advance outer iterator to end of validated body
            it.advance(body_length);
        }
        Ok(())
    }

    fn parse_data_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        let n_data_segments: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;

        for _ in 0..n_data_segments {
            if it.empty() { return Err(Malformed(UNEXPECTED_END)); }
            let segment_flag: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
            if segment_flag != 0 {
                return Err(Validation(INVALID_DATA_SEG_FLAG));
            }
            if self.memory.is_none() {
                return Err(Validation(UNKNOWN_MEMORY));
            }

            let initializer_offset = it.cur();
            validate_const(bytes, it, ValType::I32, &self.globals)?;

            let data_length: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
            if !it.has_n_left(data_length as usize) {
                return Err(Malformed(UNEXPECTED_END));
            }

            let data_start = it.cur();
            it.advance(data_length as usize);
            let data_end = it.cur();

            self.data_segments.push(DataSegment {
                data_range: data_start..data_end,
                initializer_offset
            });
        }
        Ok(())
    }

}

// ---------------- Helper Functions ----------------
fn ignore_custom_section(bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
    while !it.empty() && it.peek_u8()? == 0 {
        // Guard: concatenated module (a new "\0asm" at current position)
        if it.has_n_left(4) {
            let idx = it.cur();
            if &bytes[idx..idx + 4] == MAGIC_HEADER {
                return Ok(());
            }
        }
        it.advance(1);
        let section_length: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
        if it.cur() + section_length as usize > bytes.len() {
            return Err(Malformed(UNEXPECTED_END));
        }
        let section_start = it.cur();

        // Read name length
        let name_len: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
        if it.cur() + name_len as usize > bytes.len() {
            return Err(Malformed(UNEXPECTED_END));
        }
        let name_start = it.cur();
        it.advance(name_len as usize);

        // Validate UTF-8 encoding
        if std::str::from_utf8(&bytes[name_start..name_start + name_len as usize]).is_err() {
            return Err(Malformed(INVALID_UTF8));
        }

        // Ensure we didn't overrun the declared section length
        if section_start + (section_length as usize) < it.cur() {
            return Err(Malformed(UNEXPECTED_END));
        }

        // Advance to end of section
        it.idx = section_start + section_length as usize;
    }
    Ok(())
}

fn section<F>(it: &mut ByteIter, bytes: &[u8], id: u8, mut reader: F) -> Result<(), Error>
where
    F: FnMut(&mut ByteIter) -> Result<(), Error>
{
    if !it.empty() && it.peek_u8()? == id {
        it.advance(1);
        let section_length: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
        let section_start = it.cur();
        if section_start + section_length as usize > bytes.len() {
            return Err(Malformed(UNEXPECTED_END));
        }
        reader(it)?;
        if it.cur() - section_start != section_length as usize {
            return Err(Malformed(SECTION_SIZE_MISMATCH));
        }
        if !it.empty() && it.peek_u8()? == id {
            return Err(Malformed(JUNK_AFTER_LAST));
        }
    } else if !it.empty() && it.peek_u8()? > 11 {
        return Err(Malformed(INVALID_SECTION_ID))
    }
    ignore_custom_section(bytes, it)?;
    Ok(())
}

fn get_limits(bytes: &[u8], it: &mut ByteIter, upper: u32) -> Result<(u32, u32), Error> {
    let flags: u32 = safe_read_leb128(bytes, &mut it.idx, 1)?;
    let initial: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
    let max = if flags == 1 {
        safe_read_leb128::<u32>(bytes, &mut it.idx, 32)?
    } else {
        upper
    };
    Ok((initial, max))
}

fn get_memory_limits(bytes: &[u8], it: &mut ByteIter) -> Result<(u32, u32), Error> {
    let (initial, max) = get_limits(bytes, it, Module::MAX_PAGES)?;
    if initial > Module::MAX_PAGES || max > Module::MAX_PAGES {
        return Err(Validation(MEMORY_SIZE_LIMIT));
    }
    if max < initial {
        return Err(Validation(MIN_GREATER_THAN_MAX));
    }
    Ok((initial, max))
}

fn get_table_limits(bytes: &[u8], it: &mut ByteIter) -> Result<(u32, u32), Error> {
    let (initial, max) = get_limits(bytes, it, u32::MAX)?;
    if max < initial {
        return Err(Validation(MIN_GREATER_THAN_MAX));
    }
    Ok((initial, max))
}

fn mutability_from_byte(byte: u8) -> Result<bool, Error> {
    match byte {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(Malformed(INVALID_MUTABILITY)),
    }
}