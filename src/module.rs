use std::collections::HashMap;
use std::rc::Rc;

use crate::spec::{Error, Signature, ValType, MAGIC_HEADER};
use crate::byte_iter::ByteIter;
use crate::leb128::safe_read_leb128;
use crate::error_msg::*;

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
    pub exists: bool, 
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
pub struct Export { pub desc: ExternKind, pub idx: u32 }

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
    pub tables: Vec<Table>,
    pub memory: Memory,
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

impl Module {
    pub const MAX_PAGES: u32 = 65536;
    pub const MAX_LOCALS: usize = 50000;

    pub fn compile(bytes: Vec<u8>) -> Result<Self, Error> {
        let mut m = Module {
            bytes: Rc::new(bytes),
            types: Vec::new(),
            imports: HashMap::new(),
            tables: Vec::new(),
            memory: Memory { min: 0, max: 0, exists: false, import: None },
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

        // Skip custom sections and parse standard sections
        skip_custom_section(bytes, &mut it)?;
        section(&mut it, bytes, 1, |it: &mut ByteIter| { self.parse_type_section(bytes, it) }, None::<fn()>)?;
        skip_custom_section(bytes, &mut it)?;
        section(&mut it, bytes, 2, |it: &mut ByteIter| { self.parse_import_section(bytes, it) }, None::<fn()>)?;
        skip_custom_section(bytes, &mut it)?;
        section(&mut it, bytes, 3, |it: &mut ByteIter| { self.parse_function_section(bytes, it) }, None::<fn()>)?;
        skip_custom_section(bytes, &mut it)?;
        section(&mut it, bytes, 4, |it: &mut ByteIter| { self.parse_table_section(bytes, it) }, None::<fn()>)?;
        skip_custom_section(bytes, &mut it)?;
        section(&mut it, bytes, 5, |it: &mut ByteIter| { self.parse_memory_section(bytes, it) }, None::<fn()>)?;
        skip_custom_section(bytes, &mut it)?;
        section(&mut it, bytes, 6, |it: &mut ByteIter| { self.parse_global_section(bytes, it) }, None::<fn()>)?;
        skip_custom_section(bytes, &mut it)?;
        section(&mut it, bytes, 7, |it: &mut ByteIter| { self.parse_export_section(bytes, it) }, None::<fn()>)?;
        skip_custom_section(bytes, &mut it)?;
        section(&mut it, bytes, 8, |it: &mut ByteIter| { self.parse_start_section(bytes, it) }, None::<fn()>)?;
        skip_custom_section(bytes, &mut it)?;
        section(&mut it, bytes, 9, |it: &mut ByteIter| { self.parse_element_section(bytes, it) }, None::<fn()>)?;
        skip_custom_section(bytes, &mut it)?;
        section(&mut it, bytes, 10, |it: &mut ByteIter| { self.parse_code_section(bytes, it) }, None::<fn()>)?;
        skip_custom_section(bytes, &mut it)?;
        section(&mut it, bytes, 11, |it: &mut ByteIter| { self.parse_data_section(bytes, it) }, None::<fn()>)?;
        skip_custom_section(bytes, &mut it)?;
        
        if !it.empty() { return Err(Error::Malformed(JUNK_AFTER_LAST_SECTION)); }
        Ok(())
    }

    // TODO: section parsing
    fn parse_type_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        Ok(())
    }

    fn parse_import_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        Ok(())
    }

    fn parse_function_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        Ok(())
    }

    fn parse_table_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        Ok(())
    }

    fn parse_memory_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        Ok(())
    }

    fn parse_global_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        Ok(())
    }

    fn parse_export_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        Ok(())
    }

    fn parse_start_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        Ok(())
    }

    fn parse_element_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        Ok(())
    }

    fn parse_code_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        Ok(())
    }

    fn parse_data_section(&mut self, bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
        Ok(())
    }

    fn validate_const(bytes: &[u8], it: &mut ByteIter, expected: ValType, globals: &Vec<Global>) -> Result<(), Error> {
        Ok(())
    }
}

// ---------------- Helper Functions ----------------
fn skip_custom_section(bytes: &[u8], it: &mut ByteIter) -> Result<(), Error> {
    Ok(())
}

fn section<F, E>(it: &mut ByteIter, bytes: &[u8], id: u8, mut body: F, mut else_fn: Option<E>) -> Result<(), Error>
where
    F: FnMut(&mut ByteIter) -> Result<(), Error>,
    E: FnMut(),
{
    if !it.empty() && it.peek_u8()? == id {
        it.advance(1);
        let section_length: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
        let section_start = it.cur();
        if section_start + section_length as usize > bytes.len() { 
            return Err(Error::Malformed(UNEXPECTED_END)); 
        }
        body(it)?;
        if it.cur() - section_start != section_length as usize {
            return Err(Error::Malformed(SECTION_SIZE_MISMATCH));
        }
        Ok(())
    } else if !it.empty() && it.peek_u8()? > 11 {
        Err(Error::Malformed(INVALID_SECTION_ID))
    } else {
        if let Some(ref mut else_fn) = else_fn { 
            else_fn(); 
        }
        Ok(())
    }
}