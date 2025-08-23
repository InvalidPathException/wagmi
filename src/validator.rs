use crate::byte_iter::ByteIter;
use crate::error_msg::*;
use crate::leb128::*;
use crate::module::*;
use crate::spec::*;
use crate::debug_println;

// ---------------- Control Flow Structures ----------------
#[derive(Clone)]
pub enum ControlType {
    Function,
    Block { start: usize },
    Loop,
    If { start: usize },
    IfElse { if_start: usize, else_start: usize },
}

#[derive(Clone)]
pub struct ControlFrame {
    pub expected: Vec<ValType>,
    pub sig: Signature,
    pub polymorphic: bool,
    pub control_type: ControlType,
}

// ---------------- ValidatorStack for Type Checking ----------------
pub struct ValidatorStack {
    polymorphic: bool,
    buf: Vec<ValType>,
}

impl ValidatorStack {
    pub fn new() -> Self {
        let mut buf = Vec::with_capacity(1024);
        buf.resize(1024, ValType::Null);
        Self { polymorphic: false, buf }
    }
    
    fn polymorphism(&self) -> bool { self.polymorphic }
    fn set_polymorphism(&mut self, poly: bool) { self.polymorphic = poly; }
    
    fn polymorphize(&mut self) {
        self.polymorphic = true;
        if let Some(pos) = self.buf.iter().rposition(|&ty| ty == ValType::Null) {
            self.buf.truncate(pos + 1);
        }
    }
    
    fn depolymorphize(&mut self) { self.polymorphic = false; }
    fn push_slice(&mut self, vals: &[ValType]) { self.buf.extend_from_slice(vals); }
    fn push(&mut self, ty: ValType) { self.buf.push(ty); }
    
    fn back(&self) -> Result<ValType, Error> {
        let top_val = self.buf.last().copied().unwrap_or(ValType::Null);
        if top_val != ValType::Null {
            Ok(top_val)
        } else if self.polymorphic {
            Ok(ValType::Any)
        } else {
            Err(Error::Validation(TYPE_MISMATCH))
        }
    }

    fn count_matching_suffix(&self, expected: &[ValType]) -> usize {
        let expected_len = expected.len();
        let available = self.buf.len();
        let to_compare = expected_len.min(available);
        let mut matched = 0usize;
        while matched < to_compare {
            let stack_ty = self.buf[available - 1 - matched];
            let expected_ty = expected[expected_len - 1 - matched];
            if stack_ty != expected_ty && stack_ty != ValType::Any { break; }
            matched += 1;
        }
        matched
    }

    fn check_slice(&self, expected: &[ValType]) -> bool {
        let matched = self.count_matching_suffix(expected);
        if matched == expected.len() { return true; }
        let available = self.buf.len();
        if matched == available { return self.polymorphic; }
        let idx = available - 1 - matched;
        self.polymorphic && self.buf.get(idx).copied() == Some(ValType::Null)
    }

    fn can_be_anything(&self) -> bool {
        self.polymorphic && self.buf.last().copied() == Some(ValType::Null)
    }

    fn equals_slice(&self, expected: &[ValType]) -> bool {
        let matched = self.count_matching_suffix(expected);
        let available = self.buf.len();
        let expected_len = expected.len();
        if matched == available { return self.polymorphic; }
        let next_idx = available - 1 - matched;
        if self.buf[next_idx] != ValType::Null { return false; }
        if matched == expected_len { return true; }
        self.polymorphic
    }
    
    fn pop_slice(&mut self, expected: &[ValType]) -> Result<(), Error> {
        if !self.check_slice(expected) { return Err(Error::Validation(TYPE_MISMATCH)); }
        
        let matched = self.count_matching_suffix(expected);
        if matched > 0 {
            let new_len = self.buf.len() - matched;
            self.buf.truncate(new_len);
        }
        Ok(())
    }
    
    fn apply_sig(&mut self, sig: &Signature) -> Result<(), Error> {
        self.pop_slice(&sig.params)?;
        self.push_slice(sig.results_view());
        Ok(())
    }
    
    fn enter_flow(&mut self, expected: &[ValType]) -> Result<(), Error> {
        self.pop_slice(expected)?;
        self.push(ValType::Null);
        self.push_slice(expected);
        Ok(())
    }
    
    fn check_br(&mut self, control_stack: &Vec<ControlFrame>, depth: u32) -> Result<(), Error> {
        if (depth as usize) >= control_stack.len() { 
            return Err(Error::Validation(UNKNOWN_LABEL)); 
        }
        let target = &control_stack[control_stack.len() - (depth as usize) - 1];
        self.pop_slice(&target.expected)?;
        self.push_slice(&target.expected);
        Ok(())
    }
}

// ---------------- Function Validation ----------------
pub struct Validator<'a> {
    module: &'a mut Module,
}

impl<'a> Validator<'a> {
    pub fn new(module: &'a mut Module) -> Self {
        Self { module }
    }
    
    pub fn validate_function(&mut self, fn_index: usize) -> Result<(), Error> {
        let func = self.module.functions[fn_index].clone();
        let bytes = self.module.bytes.clone();
        let mut it = ByteIter::new(&bytes, func.body.start);
        let mut vs = ValidatorStack::new();
        let mut cs: Vec<ControlFrame> = Vec::with_capacity(64);
        
        // Function frame
        cs.push(ControlFrame { 
            expected: func.ty.results_view().to_vec(), 
            sig: func.ty.clone(), 
            polymorphic: false, 
            control_type: ControlType::Function 
        });

        // Dispatch loop
        self.dispatch_next(&mut it, &func, &mut vs, &mut cs)?;
        
        let last = bytes[it.cur() - 1];
        if last != 0x0b { 
            return Err(Error::Malformed(END_OPCODE_EXPECTED)); 
        }
        if it.cur() != func.body.end { 
            return Err(Error::Malformed(SECTION_SIZE_MISMATCH)); 
        }
        Ok(())
    }

    fn dispatch_next(&mut self, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
        let byte = it.read_u8()?;
        get_validators()[byte as usize](self.module, it, f, vs, cs)
    }
}

// ---------------- Validator Function Type ----------------
type ValidatorFn = fn(&mut Module, &mut ByteIter, &Function, &mut ValidatorStack, &mut Vec<ControlFrame>) -> Result<(), Error>;

fn validate_missing(_: &mut Module, _: &mut ByteIter, _: &Function, _: &mut ValidatorStack, _: &mut Vec<ControlFrame>) -> Result<(), Error> {
    Err(Error::Malformed(UNKNOWN_INSTRUCTION))
}

fn nextop(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    let byte = it.read_u8()?;
    get_validators()[byte as usize](m, it, f, vs, cs)
}

// ---------------- Control Flow Validators ----------------
fn validate_unreachable(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    vs.polymorphize();
    nextop(m, it, f, vs, cs)
}

fn validate_nop(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    nextop(m, it, f, vs, cs)
}

fn validate_block(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    let sig = Signature::read_blocktype(&m.types, &m.bytes, &mut it.idx)?;
    let block_start = it.cur();
    debug_println!("[val] block start key={} (0x{:x})", block_start, block_start);
    vs.enter_flow(&sig.params)?;
    cs.push(ControlFrame {
        expected: sig.results_view().to_vec(),
        sig: sig.clone(),
        polymorphic: vs.polymorphism(),
        control_type: ControlType::Block { start: block_start }
    });
    vs.depolymorphize();
    nextop(m, it, f, vs, cs)
}

fn validate_loop(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    let sig = Signature::read_blocktype(&m.types, &m.bytes, &mut it.idx)?;
    vs.enter_flow(&sig.params)?;
    cs.push(ControlFrame {
        expected: sig.params.clone(),
        sig: sig.clone(),
        polymorphic: vs.polymorphism(),
        control_type: ControlType::Loop
    });
    vs.depolymorphize();
    nextop(m, it, f, vs, cs)
}

fn validate_if(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    let sig = Signature::read_blocktype(&m.types, &m.bytes, &mut it.idx)?;
    vs.pop_slice(&[ValType::I32])?;
    vs.enter_flow(&sig.params)?;
    let if_start = it.cur();
    debug_println!("[val] if start key={} (0x{:x})", if_start, if_start);
    cs.push(ControlFrame {
        expected: sig.results_view().to_vec(),
        sig: sig.clone(),
        polymorphic: vs.polymorphism(),
        control_type: ControlType::If { start: if_start }
    });
    vs.depolymorphize();
    nextop(m, it, f, vs, cs)
}

fn validate_else(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    if let Some(top) = cs.last_mut() {
        match top.control_type {
            ControlType::If { start } => {
                if !vs.equals_slice(top.sig.results_view()) {
                    return Err(Error::Validation(TYPE_MISMATCH));
                }
                vs.pop_slice(top.sig.results_view())?;
                vs.push_slice(&top.sig.params);
                let else_start = it.cur();
                top.control_type = ControlType::IfElse { if_start: start, else_start };
                vs.depolymorphize();
                nextop(m, it, f, vs, cs)
            }
            _ => Err(Error::Validation(ELSE_MUST_CLOSE_IF)),
        }
    } else {
        Err(Error::Validation(ELSE_MUST_CLOSE_IF))
    }
}

fn validate_end(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    if cs.len() == 1 { // function end
        if !vs.equals_slice(f.ty.results_view()) {
            return Err(Error::Validation(TYPE_MISMATCH_STACK_VS_RESULTS));
        }
        return Ok(());
    }
    let top = cs.pop().unwrap();
    if !vs.equals_slice(top.sig.results_view()) {
        return Err(Error::Validation(TYPE_MISMATCH_STACK_VS_RESULTS));
    }
    vs.pop_slice(top.sig.results_view())?;
    match top.control_type {
        ControlType::Block { start } => {
            debug_println!("[val] block end map: key={} (0x{:x}) -> end={} (0x{:x})", start, start, it.cur(), it.cur());
            debug_assert!(!m.block_ends.contains_key(&start), "duplicate block end key {}", start);
            m.block_ends.insert(start, it.cur());
        }
        ControlType::Loop => {}
        ControlType::If { start } => {
            if top.sig.params != top.sig.results_view() {
                return Err(Error::Validation(TYPE_MISMATCH_PARAMS_VS_RESULTS));
            }
            let else_off = it.cur() - 1;
            let end_off = it.cur();
            debug_println!("[val] if map: key={} (0x{:x}) -> else={} (0x{:x}), end={} (0x{:x})", start, start, else_off, else_off, end_off, end_off);
            debug_assert!(!m.if_jumps.contains_key(&start), "duplicate if_jumps key {}", start);
            m.if_jumps.insert(start, IfJump { else_offset: else_off, end_offset: end_off });
        }
        ControlType::IfElse { if_start, else_start } => {
            debug_println!("[val] if-else map: key={} (0x{:x}) -> else={} (0x{:x}), end={} (0x{:x})", if_start, if_start, else_start, else_start, it.cur(), it.cur());
            debug_assert!(!m.if_jumps.contains_key(&if_start), "duplicate if_jumps key {}", if_start);
            m.if_jumps.insert(if_start, IfJump { else_offset: else_start, end_offset: it.cur() });
        }
        ControlType::Function => {}
    }
    vs.pop_slice(&[ValType::Null])?;
    vs.set_polymorphism(top.polymorphic);
    vs.push_slice(top.sig.results_view());
    nextop(m, it, f, vs, cs)
}

fn validate_br(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    let depth: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    vs.check_br(cs, depth)?;
    vs.polymorphize();
    nextop(m, it, f, vs, cs)
}

fn validate_br_if(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    vs.pop_slice(&[ValType::I32])?;
    let depth: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    vs.check_br(cs, depth)?;
    nextop(m, it, f, vs, cs)
}

fn validate_br_table(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    vs.pop_slice(&[ValType::I32])?;
    let n_targets: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    let mut targets: Vec<u32> = Vec::with_capacity(n_targets as usize + 1);
    for _ in 0..n_targets {
        let lab: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
        targets.push(lab);
    }
    if it.empty() || m.bytes[it.cur()] == 0x0b {
        return Err(Error::Malformed(UNEXPECTED_END));
    }
    let default_lab: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    targets.push(default_lab);

    for &lab in &targets {
        if (lab as usize) >= cs.len() {
            return Err(Error::Validation(UNKNOWN_LABEL));
        }
    }

    let base = cs.len() - 1;
    let default_expected = &cs[base - targets[n_targets as usize] as usize].expected;
    for &depth in &targets {
        let target_expected = &cs[base - depth as usize].expected;
        if vs.can_be_anything() {
            if !vs.check_slice(target_expected) {
                return Err(Error::Validation(TYPE_MISMATCH));
            }
            if default_expected != target_expected {
                return Err(Error::Validation(TYPE_MISMATCH));
            }
        } else {
            vs.check_br(cs, depth)?;
            if default_expected != target_expected {
                return Err(Error::Validation(TYPE_MISMATCH));
            }
        }
    }
    vs.polymorphize();
    nextop(m, it, f, vs, cs)
}

fn validate_return(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    let depth = (cs.len() - 1) as u32;
    vs.check_br(cs, depth)?;
    vs.polymorphize();
    nextop(m, it, f, vs, cs)
}

// ---------------- Stack Manipulation ----------------
fn validate_drop(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    let _ = vs.back()?;
    vs.pop_slice(&[vs.back()?])?;
    nextop(m, it, f, vs, cs)
}

fn validate_select(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    vs.pop_slice(&[ValType::I32])?;
    let ty = vs.back()?;
    if ty != ValType::Any && !crate::spec::is_val_type(ty as u8) {
        return Err(Error::Validation(TYPE_MISMATCH));
    }
    vs.pop_slice(&[ty, ty])?;
    vs.push(ty);
    nextop(m, it, f, vs, cs)
}

// ---------------- Variable Instructions ----------------
fn validate_localget(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    let local_idx: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    if (local_idx as usize) >= f.locals.len() {
        return Err(Error::Validation(UNKNOWN_LOCAL));
    }
    vs.push(f.locals[local_idx as usize]);
    nextop(m, it, f, vs, cs)
}

fn validate_localset(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    let local_idx: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    if (local_idx as usize) >= f.locals.len() {
        return Err(Error::Validation(UNKNOWN_LOCAL));
    }
    vs.pop_slice(&[f.locals[local_idx as usize]])?;
    nextop(m, it, f, vs, cs)
}

fn validate_localtee(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    let local_idx: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    if (local_idx as usize) >= f.locals.len() {
        return Err(Error::Validation(UNKNOWN_LOCAL));
    }
    let ty = f.locals[local_idx as usize];
    vs.pop_slice(&[ty])?;
    vs.push(ty);
    nextop(m, it, f, vs, cs)
}

fn validate_globalget(m: &mut Module, it: &mut ByteIter, _f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    let global_idx: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    if (global_idx as usize) >= m.globals.len() {
        return Err(Error::Validation(UNKNOWN_GLOBAL));
    }
    vs.push(m.globals[global_idx as usize].ty);
    nextop(m, it, _f, vs, cs)
}

fn validate_globalset(m: &mut Module, it: &mut ByteIter, _f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    let global_idx: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    if (global_idx as usize) >= m.globals.len() {
        return Err(Error::Validation(UNKNOWN_GLOBAL));
    } else if !m.globals[global_idx as usize].is_mutable {
        return Err(Error::Validation(GLOBAL_IS_IMMUTABLE));
    }
    vs.pop_slice(&[m.globals[global_idx as usize].ty])?;
    nextop(m, it, _f, vs, cs)
}

// ---------------- Memory Instructions ----------------
fn validate_memorysize(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    let flag = it.read_u8()?;
    if flag != 0 {
        return Err(Error::Malformed(ZERO_FLAG_EXPECTED));
    } else if m.memory.is_none() {
        return Err(Error::Validation(UNKNOWN_MEMORY));
    }
    vs.push(ValType::I32);
    nextop(m, it, f, vs, cs)
}

fn validate_memorygrow(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    let flag = it.read_u8()?;
    if flag != 0 {
        return Err(Error::Malformed(ZERO_FLAG_EXPECTED));
    } else if m.memory.is_none() {
        return Err(Error::Validation(UNKNOWN_MEMORY));
    }
    vs.pop_slice(&[ValType::I32])?;
    vs.push(ValType::I32);
    nextop(m, it, f, vs, cs)
}

// ---------------- Constant Instructions ----------------
fn validate_i32const(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    let _val: i32 = safe_read_sleb128(&m.bytes, &mut it.idx, 32)?;
    vs.push(ValType::I32);
    nextop(m, it, f, vs, cs)
}

fn validate_i64const(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    let _val: i64 = safe_read_sleb128(&m.bytes, &mut it.idx, 64)?;
    vs.push(ValType::I64);
    nextop(m, it, f, vs, cs)
}

fn validate_f32const(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    if !it.has_n_left(4) {
        return Err(Error::Malformed(UNEXPECTED_END));
    }
    it.advance(4);
    vs.push(ValType::F32);
    nextop(m, it, f, vs, cs)
}

fn validate_f64const(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    if !it.has_n_left(8) {
        return Err(Error::Malformed(UNEXPECTED_END));
    }
    it.advance(8);
    vs.push(ValType::F64);
    nextop(m, it, f, vs, cs)
}

// ---------------- Numeric Operations ----------------
macro_rules! numeric {
    ($name:ident, $in:expr, $out:expr) => {
        fn $name(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
            vs.pop_slice($in)?;
            for &t in $out { vs.push(t); }
            nextop(m, it, f, vs, cs)
        }
    }
}

numeric!(validate_i32_i32, &[ValType::I32], &[ValType::I32]);
numeric!(validate_i64_i64, &[ValType::I64], &[ValType::I64]);
numeric!(validate_f32_f32, &[ValType::F32], &[ValType::F32]);
numeric!(validate_f64_f64, &[ValType::F64], &[ValType::F64]);
numeric!(validate_i32i32_i32, &[ValType::I32, ValType::I32], &[ValType::I32]);
numeric!(validate_i64i64_i64, &[ValType::I64, ValType::I64], &[ValType::I64]);
numeric!(validate_f32f32_f32, &[ValType::F32, ValType::F32], &[ValType::F32]);
numeric!(validate_f64f64_f64, &[ValType::F64, ValType::F64], &[ValType::F64]);
numeric!(validate_i64_i32, &[ValType::I64], &[ValType::I32]);
numeric!(validate_f32_i32, &[ValType::F32], &[ValType::I32]);
numeric!(validate_f64_i32, &[ValType::F64], &[ValType::I32]);
numeric!(validate_i64i64_i32, &[ValType::I64, ValType::I64], &[ValType::I32]);
numeric!(validate_f32f32_i32, &[ValType::F32, ValType::F32], &[ValType::I32]);
numeric!(validate_f64f64_i32, &[ValType::F64, ValType::F64], &[ValType::I32]);
numeric!(validate_i32_i64, &[ValType::I32], &[ValType::I64]);
numeric!(validate_f32_i64, &[ValType::F32], &[ValType::I64]);
numeric!(validate_f64_i64, &[ValType::F64], &[ValType::I64]);
numeric!(validate_i32_f32, &[ValType::I32], &[ValType::F32]);
numeric!(validate_i64_f32, &[ValType::I64], &[ValType::F32]);
numeric!(validate_f64_f32, &[ValType::F64], &[ValType::F32]);
numeric!(validate_i32_f64, &[ValType::I32], &[ValType::F64]);
numeric!(validate_i64_f64, &[ValType::I64], &[ValType::F64]);
numeric!(validate_f32_f64, &[ValType::F32], &[ValType::F64]);

// ---------------- Memory Load/Store Operations ----------------
fn validate_load(m: &mut Module, it: &mut ByteIter, val_ty: ValType, natural_align: u32, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    let align_bits: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    if m.memory.is_none() {
        return Err(Error::Validation(UNKNOWN_MEMORY));
    }
    if align_bits >= 32 {
        return Err(Error::Malformed(INT_TOO_LARGE));
    }
    let _off: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    let align = 1u64 << align_bits;
    if align > natural_align as u64 {
        return Err(Error::Validation(ALIGNMENT_TOO_LARGE));
    }
    vs.pop_slice(&[ValType::I32])?;
    vs.push(val_ty);
    nextop(m, it, f, vs, cs)
}

fn validate_store(m: &mut Module, it: &mut ByteIter, val_ty: ValType, natural_align: u32, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    let mut align_bits: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    if (1 << 6) & align_bits != 0 {
        align_bits = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    } else {
        if m.memory.is_none() {
            return Err(Error::Validation(UNKNOWN_MEMORY));
        }
    }
    if align_bits >= 32 {
        return Err(Error::Malformed(INT_TOO_LARGE));
    }
    let _off: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    let align = 1u64 << align_bits;
    if align > natural_align as u64 {
        return Err(Error::Validation(ALIGNMENT_TOO_LARGE));
    }
    vs.pop_slice(&[ValType::I32, val_ty])?;
    nextop(m, it, f, vs, cs)
}

macro_rules! load {
    ($name:ident, $ty:expr, $align:expr) => {
        fn $name(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
            validate_load(m, it, $ty, $align, f, vs, cs)
        }
    }
}

macro_rules! store {
    ($name:ident, $ty:expr, $align:expr) => {
        fn $name(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
            validate_store(m, it, $ty, $align, f, vs, cs)
        }
    }
}

load!(validate_i32load, ValType::I32, 4); load!(validate_i64load, ValType::I64, 8);
load!(validate_f32load, ValType::F32, 4); load!(validate_f64load, ValType::F64, 8);
load!(validate_i32load8_s, ValType::I32, 1); load!(validate_i32load8_u, ValType::I32, 1);
load!(validate_i32load16_s, ValType::I32, 2); load!(validate_i32load16_u, ValType::I32, 2);
load!(validate_i64load8_s, ValType::I64, 1); load!(validate_i64load8_u, ValType::I64, 1);
load!(validate_i64load16_s, ValType::I64, 2); load!(validate_i64load16_u, ValType::I64, 2);
load!(validate_i64load32_s, ValType::I64, 4); load!(validate_i64load32_u, ValType::I64, 4);
store!(validate_i32store, ValType::I32, 4); store!(validate_i64store, ValType::I64, 8);
store!(validate_f32store, ValType::F32, 4); store!(validate_f64store, ValType::F64, 8);
store!(validate_i32store8, ValType::I32, 1); store!(validate_i32store16, ValType::I32, 2);
store!(validate_i64store8, ValType::I64, 1); store!(validate_i64store16, ValType::I64, 2);
store!(validate_i64store32, ValType::I64, 4);

// ---------------- Call Instructions ----------------
fn validate_call(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    let func_idx: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    if (func_idx as usize) >= m.functions.len() {
        return Err(Error::Validation(UNKNOWN_FUNC));
    }
    let sig = &m.functions[func_idx as usize].ty;
    vs.apply_sig(sig)?;
    nextop(m, it, f, vs, cs)
}

fn validate_call_indirect(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack, cs: &mut Vec<ControlFrame>) -> Result<(), Error> {
    vs.pop_slice(&[ValType::I32])?;
    let type_idx: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    if (type_idx as usize) >= m.types.len() {
        return Err(Error::Validation(UNKNOWN_TYPE));
    }
    let flag = it.read_u8()?;
    if flag != 0 {
        return Err(Error::Malformed(ZERO_FLAG_EXPECTED));
    } else if m.table.is_none() {
        return Err(Error::Validation(UNKNOWN_TABLE));
    }
    let sig = &m.types[type_idx as usize];
    vs.apply_sig(sig)?;
    nextop(m, it, f, vs, cs) 
}

// ---------------- Validator Table ----------------
fn build_validators_table() -> [ValidatorFn; 256] {
    let mut t: [ValidatorFn; 256] = [validate_missing; 256];
        // Control flow
        t[0x00] = validate_unreachable; t[0x01] = validate_nop;
        t[0x02] = validate_block; t[0x03] = validate_loop;
        t[0x04] = validate_if; t[0x05] = validate_else;
        t[0x0b] = validate_end; t[0x0c] = validate_br;
        t[0x0d] = validate_br_if; t[0x0e] = validate_br_table;
        t[0x0f] = validate_return;
        // Call instructions
        t[0x10] = validate_call; t[0x11] = validate_call_indirect;
        // Stack manipulation
        t[0x1a] = validate_drop; t[0x1b] = validate_select;
        // Variable instructions
        t[0x20] = validate_localget; t[0x21] = validate_localset;
        t[0x22] = validate_localtee; t[0x23] = validate_globalget;
        t[0x24] = validate_globalset;
        // Memory loads
        t[0x28] = validate_i32load; t[0x29] = validate_i64load;
        t[0x2a] = validate_f32load; t[0x2b] = validate_f64load;
        t[0x2c] = validate_i32load8_s; t[0x2d] = validate_i32load8_u;
        t[0x2e] = validate_i32load16_s; t[0x2f] = validate_i32load16_u;
        t[0x30] = validate_i64load8_s; t[0x31] = validate_i64load8_u;
        t[0x32] = validate_i64load16_s; t[0x33] = validate_i64load16_u;
        t[0x34] = validate_i64load32_s; t[0x35] = validate_i64load32_u;
        // Memory stores
        t[0x36] = validate_i32store; t[0x37] = validate_i64store;
        t[0x38] = validate_f32store; t[0x39] = validate_f64store;
        t[0x3a] = validate_i32store8; t[0x3b] = validate_i32store16;
        t[0x3c] = validate_i64store8; t[0x3d] = validate_i64store16;
        t[0x3e] = validate_i64store32;
        // Memory size/grow
        t[0x3f] = validate_memorysize; t[0x40] = validate_memorygrow;
        // Constants
        t[0x41] = validate_i32const; t[0x42] = validate_i64const;
        t[0x43] = validate_f32const; t[0x44] = validate_f64const;
        // Numeric operations
        t[0x45] = validate_i32_i32; // i32.eqz
        for i in 0x46..=0x4f { t[i] = validate_i32i32_i32; } // i32 comparisons
        t[0x50] = validate_i64_i32; // i64.eqz
        for i in 0x51..=0x5a { t[i] = validate_i64i64_i32; } // i64 comparisons
        for i in 0x5b..=0x60 { t[i] = validate_f32f32_i32; } // f32 comparisons
        for i in 0x61..=0x66 { t[i] = validate_f64f64_i32; } // f64 comparisons
        for i in 0x67..=0x69 { t[i] = validate_i32_i32; } // i32 unary
        for i in 0x6a..=0x78 { t[i] = validate_i32i32_i32; } // i32 binary
        for i in 0x79..=0x7b { t[i] = validate_i64_i64; } // i64 unary
        for i in 0x7c..=0x8a { t[i] = validate_i64i64_i64; } // i64 binary
        for i in 0x8b..=0x91 { t[i] = validate_f32_f32; } // f32 unary
        for i in 0x92..=0x98 { t[i] = validate_f32f32_f32; } // f32 binary
        for i in 0x99..=0x9f { t[i] = validate_f64_f64; } // f64 unary
        for i in 0xa0..=0xa6 { t[i] = validate_f64f64_f64; } // f64 binary
        // Conversions
        t[0xa7] = validate_i64_i32; t[0xa8] = validate_f32_i32;
        t[0xa9] = validate_f32_i32; t[0xaa] = validate_f64_i32;
        t[0xab] = validate_f64_i32; t[0xac] = validate_i32_i64;
        t[0xad] = validate_i32_i64; t[0xae] = validate_f32_i64;
        t[0xaf] = validate_f32_i64; t[0xb0] = validate_f64_i64;
        t[0xb1] = validate_f64_i64; t[0xb2] = validate_i32_f32;
        t[0xb3] = validate_i32_f32; t[0xb4] = validate_i64_f32;
        t[0xb5] = validate_i64_f32; t[0xb6] = validate_f64_f32;
        t[0xb7] = validate_i32_f64; t[0xb8] = validate_i32_f64;
        t[0xb9] = validate_i64_f64; t[0xba] = validate_i64_f64;
        t[0xbb] = validate_f32_f64; t[0xbc] = validate_f32_i32;
        t[0xbd] = validate_f64_i64; t[0xbe] = validate_i32_f32;
        t[0xbf] = validate_i64_f64;
    t
}

fn get_validators() -> &'static [ValidatorFn; 256] {
    static VALIDATORS: std::sync::LazyLock<Box<[ValidatorFn; 256]>> = std::sync::LazyLock::new(|| {
        Box::new(build_validators_table())
    });
    &**VALIDATORS
}