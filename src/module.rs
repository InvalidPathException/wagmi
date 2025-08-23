use std::collections::HashMap;
use std::rc::Rc;

use crate::byte_iter::*;
use crate::error_msg::*;
use crate::leb128::*;
use crate::spec::*;
use crate::validator::Validator;

// ---------------- Import/Export related ----------------
#[derive(Clone, Debug)]
pub struct ImportRef { pub module: String, pub field: String }

#[derive(Clone, Copy)]
pub enum ExternKind {
    Func = 0, 
    Table = 1, 
    Mem = 2, 
    Global = 3 
}

impl ExternKind {
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(ExternKind::Func),
            1 => Some(ExternKind::Table),
            2 => Some(ExternKind::Mem),
            3 => Some(ExternKind::Global),
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
    pub ty: ValType, 
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
pub struct Export { pub kind: ExternKind, pub idx: u32 }

#[derive(Clone)]
pub struct Element { pub ty: ValType }

#[derive(Clone)]
pub struct DataSegment { pub data_range: std::ops::Range<usize>, pub initializer_offset: usize }

#[derive(Clone, Copy)]
pub struct IfJump { pub else_offset: usize, pub end_offset: usize }

// ---------------- Module Structure ----------------
pub struct Module {
    pub bytes: Rc<Vec<u8>>,
    pub types: Vec<Signature>,
    pub imports: HashMap<String, HashMap<String, ExternKind>>,
    pub table: Option<Table>,
    pub memory: Option<Memory>,
    pub globals: Vec<Global>,
    pub exports: HashMap<String, Export>,
    pub start: u32,
    pub element_start: usize,
    pub elements: Vec<Element>,
    pub functions: Vec<Function>,
    pub n_data: u32,
    pub data_segments: Vec<DataSegment>,
    pub if_jumps: HashMap<usize, IfJump>,
    pub block_ends: HashMap<usize, usize>,
}

macro_rules! assert_not_empty {
    ($it:expr) => { if $it.empty() { return Err(Error::Malformed(UNEXPECTED_END)); } };
}

impl Module {
    pub const MAX_PAGES: u32 = 65536;
    pub const MAX_LOCALS: usize = 50000;

    pub fn compile(bytes: Vec<u8>) -> Result<Self, Error> {
        let mut m = Module {
            bytes: Rc::new(bytes),
            types: Vec::new(),
            imports: HashMap::new(),
            table: None,
            memory: None,
            globals: Vec::new(),
            exports: HashMap::new(),
            start: u32::MAX,
            element_start: 0,
            elements: Vec::new(),
            functions: Vec::new(),
            n_data: 0,
            data_segments: Vec::new(),
            if_jumps: HashMap::new(),
            block_ends: HashMap::new(),
        };
        m.initialize()?;
        Ok(m)
    }

    fn initialize(&mut self) -> Result<(), Error> {
        let bytes_arc = self.bytes.clone();
        let bytes: &[u8] = &bytes_arc[..];
        
        // Check magic number and version
        if bytes.len() < 4 { return Err(Error::Malformed(UNEXPECTED_END)); }
        if &bytes[0..4] != MAGIC_HEADER {
            return Err(Error::Malformed(MAGIC_HEADER_NOT_DETECTED));
        }
        
        let mut it = ByteIter::new(bytes, 4);
        if bytes.len() < 8 { return Err(Error::Malformed(UNEXPECTED_END)); }
        if u32::from_le_bytes(bytes[4..8].try_into().unwrap()) != 1 {
            return Err(Error::Malformed(UNKNOWN_BINARY_VERSION));
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
        
        if !it.empty() { return Err(Error::Malformed(JUNK_AFTER_LAST_SECTION)); }
        Ok(())
    }

    fn parse_type_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        let n_types: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
        self.types.reserve_exact(n_types as usize);

        for i in 0..n_types as usize {
            assert_not_empty!(it);
            let byte = it.read_u8()?;
            if byte != 0x60 {
                return Err(Error::Malformed(INT_TOO_LONG));
            }

            let n_params: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
            let mut sig = Signature::default();
            sig.params.reserve_exact(n_params as usize);

            for _ in 0..n_params {
                let ty = it.read_u8()?;
                if !is_val_type(ty) {
                    return Err(Error::Malformed(INVALID_VALUE_TYPE));
                }
                sig.params.push(valtype_from_byte(ty).unwrap());
            }

            let n_results: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
            if n_results > 1 {
                return Err(Error::Malformed(INVALID_RESULT_ARITY));
            }
            if n_results == 1 {
                let ty = it.read_u8()?;
                if !is_val_type(ty) {
                    return Err(Error::Malformed(INVALID_RESULT_TYPE));
                }
                sig.result = valtype_from_byte(ty).unwrap();
                sig.result_count = 1;
            }

            self.types.push(sig);
        }

        Ok(())
    }

    fn parse_import_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        let n_imports: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;

        for _ in 0..n_imports {
            assert_not_empty!(it);

            // Module name
            let module_len: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
            let module_start = it.idx;
            if module_start + module_len as usize > bytes.len() {
                return Err(Error::Malformed(UNEXPECTED_END));
            }
            if !is_valid_utf8(&bytes[module_start..module_start + module_len as usize]) {
                return Err(Error::Malformed(INVALID_UTF8));
            }
            let module_name = String::from_utf8(bytes[module_start..module_start + module_len as usize].to_vec()).unwrap();
            it.idx = module_start + module_len as usize;

            // Field name
            let field_len: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
            let field_start = it.idx;
            if field_start + field_len as usize > bytes.len() {
                return Err(Error::Malformed(UNEXPECTED_END));
            }
            if !is_valid_utf8(&bytes[field_start..field_start + field_len as usize]) {
                return Err(Error::Malformed(INVALID_UTF8));
            }
            let field_name = String::from_utf8(bytes[field_start..field_start + field_len as usize].to_vec()).unwrap();
            it.idx = field_start + field_len as usize;

            let byte = it.read_u8()?;
            let kind = ExternKind::from_byte(byte)
                .ok_or(Error::Malformed(MALFORMED_IMPORT_KIND))?;

            self.imports.entry(module_name.clone()).or_default().insert(field_name.clone(), kind);
            let import = Some(ImportRef {
                module: module_name.clone(),
                field: field_name.clone()
            });

            match kind {
                ExternKind::Func => {
                    let type_idx: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
                    if (type_idx as usize) >= self.types.len() {
                        return Err(Error::Validation(UNKNOWN_TYPE));
                    }
                    self.functions.push(Function {
                        body: 0..0,
                        ty: self.types[type_idx as usize].clone(),
                        locals: vec![],
                        import,
                        is_declared: false
                    });
                }
                ExternKind::Table => {
                    if self.table.is_some() {
                        return Err(Error::Validation(MULTIPLE_TABLES));
                    }
                    // Only 0x70 in 1.0 MVP
                    let reftype: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
                    if reftype != 0x70 {
                        return Err(Error::Malformed(MALFORMED_REFERENCE_TYPE));
                    }
                    let (min, max) = get_table_limits(bytes, it)?;
                    self.table = Some(Table { min, max, ty: ValType::F64, import });
                }
                ExternKind::Mem => {
                    if self.memory.is_some() {
                        return Err(Error::Validation(MULTIPLE_MEMORIES));
                    }
                    let (min, max) = get_memory_limits(bytes, it)?;
                    self.memory = Some(Memory { min, max, import });
                }
                ExternKind::Global => {
                    let ty: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
                    if !is_val_type(ty as u8) {
                        return Err(Error::Malformed(INVALID_GLOBAL_TYPE));
                    }
                    let mut_byte = it.read_u8()?;
                    let is_mutable = mutability_from_byte(mut_byte)
                        .ok_or(Error::Malformed(INVALID_MUTABILITY))?;
                    self.globals.push(Global {
                        ty: valtype_from_byte(ty as u8).unwrap(),
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
            assert_not_empty!(it);
            let type_idx: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
            if (type_idx as usize) >= self.types.len() {
                return Err(Error::Validation(UNKNOWN_TYPE));
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
            return Err(Error::Validation(MULTIPLE_TABLES));
        }

        if n_tables == 1 {
            assert_not_empty!(it);
            let elem_type = it.read_u8()?;
            if elem_type != 0x70 {
                return Err(Error::Validation(INVALID_TABLE_ELEM_TYPE));
            }
            let (min, max) = get_table_limits(bytes, it)?;
            self.table = Some(Table { min, max, ty: ValType::F64, import: None });
        }
        Ok(())
    }

    fn parse_memory_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        let n_memories: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
        if n_memories > 1 || (n_memories == 1 && self.memory.is_some()) {
            return Err(Error::Validation(MULTIPLE_MEMORIES));
        }

        if n_memories == 1 {
            assert_not_empty!(it);
            let (min, max) = get_memory_limits(bytes, it)?;
            self.memory = Some(Memory { min, max, import: None });
        }
        Ok(())
    }

    fn parse_global_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        let n_globals: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;

        for _ in 0..n_globals {
            assert_not_empty!(it);
            let ty = it.read_u8()?;
            if !is_val_type(ty) {
                return Err(Error::Malformed(INVALID_GLOBAL_TYPE));
            }
            let mut_byte = it.read_u8()?;
            let is_mutable = mutability_from_byte(mut_byte)
                .ok_or(Error::Malformed(INVALID_MUTABILITY))?;
            let initializer_offset = it.cur();
            self.globals.push(Global {
                ty: valtype_from_byte(ty).unwrap(),
                is_mutable,
                initializer_offset,
                import: None
            });
            Self::validate_const(bytes, it, valtype_from_byte(ty).unwrap(), &self.globals)?;
        }
        Ok(())
    }

    fn parse_export_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        let n_exports: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;

        for _ in 0..n_exports {
            assert_not_empty!(it);

            let name_len: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
            let name_start = it.idx;
            if name_start + name_len as usize > bytes.len() {
                return Err(Error::Malformed(UNEXPECTED_END));
            }
            let name = String::from_utf8(bytes[name_start..name_start + name_len as usize].to_vec()).unwrap();
            it.idx = name_start + name_len as usize;

            let byte = it.read_u8()?;
            let kind = ExternKind::from_byte(byte)
                .ok_or(Error::Validation(INVALID_EXPORT_DESC))?;

            let export_idx: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;

            if self.exports.contains_key(&name) {
                return Err(Error::Validation(DUPLICATE_EXPORT_NAME));
            }

            match kind {
                ExternKind::Func => {
                    if (export_idx as usize) >= self.functions.len() {
                        return Err(Error::Validation(UNKNOWN_FUNC));
                    }
                    self.functions[export_idx as usize].is_declared = true;
                }
                ExternKind::Table => {
                    if export_idx != 0 {
                        return Err(Error::Validation(UNKNOWN_TABLE));
                    }
                }
                ExternKind::Mem => {
                    if export_idx != 0 || self.memory.is_some() {
                        return Err(Error::Validation(UNKNOWN_MEMORY));
                    }
                }
                ExternKind::Global => {
                    if (export_idx as usize) >= self.globals.len() {
                        return Err(Error::Validation(UNKNOWN_GLOBAL));
                    }
                }
            }

            self.exports.insert(name, Export {
                kind,
                idx: export_idx
            });
        }
        Ok(())
    }

    fn parse_start_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        let start: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
        if (start as usize) >= self.functions.len() {
            return Err(Error::Validation(UNKNOWN_FUNC));
        }
        self.start = start;
        Ok(())
    }

    // TODO: section parsing
    fn parse_element_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        Ok(())
    }

    fn parse_code_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        let n_functions: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
        let n_imports = self.functions.iter().filter(|f| f.import.is_some()).count() as u32;
        if (n_functions + n_imports) as usize != self.functions.len() {
            return Err(Error::Malformed(FUNC_CODE_INCONSISTENT));
        }

        for i in 0..self.functions.len() {
            if self.functions[i].import.is_some() {
                continue;
            }

            // Initialize locals with params
            {
                let function = &mut self.functions[i];
                self.functions[i].locals = function.ty.params.clone();
            }

            let function_length: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
            let func_start = it.cur();

            // Parse local declarations
            let mut n_local_decls: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
            while n_local_decls > 0 {
                n_local_decls -= 1;
                let n_locals: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
                let ty = it.read_u8()?;
                if !is_val_type(ty) {
                    return Err(Error::Validation(INVALID_LOCAL_TYPE));
                }
                for _ in 0..n_locals {
                    let vt = valtype_from_byte(ty).unwrap();
                    let function = &mut self.functions[i];
                    function.locals.push(vt);
                    if function.locals.len() > Module::MAX_LOCALS {
                        return Err(Error::Malformed(TOO_MANY_LOCALS));
                    }
                }
            }

            let body_start = it.cur();
            let body_length = function_length as usize - (body_start - func_start);
            let body_end_expected = body_start + body_length;

            {
                let function = &mut self.functions[i];
                function.body = body_start..body_end_expected;
            }

            // Validate function body immediately
            // TODO: check if this can be a member
            Validator::new(self).validate_function(i)?;
            // Advance outer iterator to end of validated body
            it.advance(body_length);
        }
        Ok(())
    }

    fn parse_data_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        let n_data_segments: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;

        for _ in 0..n_data_segments {
            assert_not_empty!(it);
            let segment_flag: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
            if segment_flag != 0 {
                return Err(Error::Validation(INVALID_DATA_SEGMENT_FLAG));
            }
            if self.memory.is_none() {
                return Err(Error::Validation(UNKNOWN_MEMORY));
            }

            let initializer_offset = it.cur();
            Self::validate_const(bytes, it, ValType::I32, &self.globals)?;

            let data_length: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
            if !it.has_n_left(data_length as usize) {
                return Err(Error::Malformed(UNEXPECTED_END));
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

    // TODO: validation for const
    fn validate_const(bytes: &[u8], it: &mut ByteIter, expected: ValType, globals: &Vec<Global>) -> Result<(), Error> {
        Ok(())
    }
}

// ---------------- Helper Functions ----------------
fn ignore_custom_section(bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
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
            return Err(Error::Malformed(UNEXPECTED_END)); 
        }
        reader(it)?;
        if it.cur() - section_start != section_length as usize {
            return Err(Error::Malformed(SECTION_SIZE_MISMATCH));
        }
    } else if !it.empty() && it.peek_u8()? > 11 {
        return Err(Error::Malformed(INVALID_SECTION_ID))
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

    if max < initial { return Err(Error::Validation(SIZE_MIN_GREATER_THAN_MAX)); }
    Ok((initial, max))
}

fn get_memory_limits(bytes: &[u8], it: &mut ByteIter) -> Result<(u32, u32), Error> {
    let (initial, max) = get_limits(bytes, it, Module::MAX_PAGES)?;
    if initial > Module::MAX_PAGES || max > Module::MAX_PAGES {
        return Err(Error::Validation(MEMORY_SIZE_LIMIT));
    }
    Ok((initial, max))
}

fn get_table_limits(bytes: &[u8], it: &mut ByteIter) -> Result<(u32, u32), Error> {
    get_limits(bytes, it, u32::MAX)
}