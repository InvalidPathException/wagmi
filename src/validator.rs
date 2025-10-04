use crate::byte_iter::ByteIter;
use crate::error::*;
use crate::leb128::*;
use crate::module::*;
use crate::signature::*;

// ---------------- Control Flow Structures ----------------
#[derive(Clone)]
pub enum ControlType {
    Function,
    Block { start: usize },
    Loop,
    If { start: usize },
    IfElse { if_start: usize, else_start: usize },
}

// ---------------- Validator Control Flow ----------------
pub enum Action { Continue, End }

#[derive(Clone)]
pub struct ControlFrame {
    pub sig: Signature,
    pub height: usize,
    pub unreachable: bool,
    pub control_type: ControlType,
    pub sig_pc: usize,
}

// ---------------- ValidatorStack for Type Checking ----------------
pub struct ValidatorStack {
    val_stack: Vec<ValType>,
    ctrl_stack: Vec<ControlFrame>,
}

impl ValidatorStack {
    pub fn new() -> Self { 
        Self { 
            val_stack: Vec::with_capacity(1024),
            ctrl_stack: Vec::with_capacity(64),
        }
    }
    
    pub fn size(&self) -> usize { self.val_stack.len() }
    pub fn push_val(&mut self, ty: ValType) { self.val_stack.push(ty); }
    pub fn push_vals(&mut self, types: &[ValType]) { self.val_stack.extend_from_slice(types); }
    pub fn frame_count(&self) -> usize { self.ctrl_stack.len() }
    pub fn last_frame(&self) -> Option<&ControlFrame> { self.ctrl_stack.last() }
    pub fn get_frame(&self, index: usize) -> Option<&ControlFrame> { self.ctrl_stack.get(index) }
    pub fn push_frame(&mut self, frame: ControlFrame) { self.ctrl_stack.push(frame); }
    pub fn pop_frame(&mut self) -> Option<ControlFrame> { self.ctrl_stack.pop() }
    
    pub fn pop_val(&mut self) -> Result<ValType, Error> {
        if self.ctrl_stack.is_empty() { return Err(Error::validation(TYPE_MISMATCH)); }
        let frame = self.ctrl_stack.last().unwrap();

        if self.val_stack.len() == frame.height {
            if frame.unreachable { return Ok(ValType::Any); }
            return Err(Error::validation(TYPE_MISMATCH));
        }

        if self.val_stack.len() < frame.height { return Err(Error::validation(TYPE_MISMATCH)); }
        Ok(self.val_stack.pop().unwrap())
    }

    pub fn pop_val_expect(&mut self, expect: ValType) -> Result<ValType, Error> {
        let actual = self.pop_val()?;
        if actual == ValType::Any { return Ok(expect); }
        if expect == ValType::Any { return Ok(actual); }
        if actual != expect { return Err(Error::validation(TYPE_MISMATCH)); }
        Ok(actual)
    }

    pub fn pop_vals(&mut self, types: &[ValType]) -> Result<Vec<ValType>, Error> {
        let mut popped = Vec::new();
        for &ty in types.iter().rev() {
            popped.insert(0, self.pop_val_expect(ty)?);
        }
        Ok(popped)
    }

    pub fn push_ctrl(&mut self, sig: Signature, control_type: ControlType, sig_pc: usize) -> Result<(), Error> {
        let frame = ControlFrame {
            sig: sig.clone(),
            height: self.val_stack.len(),
            unreachable: false,
            control_type,
            sig_pc,
        };
        self.ctrl_stack.push(frame);
        self.push_vals(&sig.params);
        Ok(())
    }


    pub fn unreachable(&mut self) {
        if let Some(frame) = self.ctrl_stack.last_mut() {
            self.val_stack.truncate(frame.height);
            frame.unreachable = true;
        }
    }
}

// ---------------- Constant Expression Validation ----------------
pub fn validate_const(bytes: &[u8], it: &mut ByteIter, expected: ValType, globals: &[Global]) -> Result<(), Error> {
    let mut stack: Vec<ValType> = Vec::new();
    loop {
        let byte = it.read_u8()?;
        if byte == 0x0b { // end
            break;
        }
        match byte {
            0x23 => { // global.get
                let global_idx: u32 = safe_read_leb128(bytes, &mut it.idx, 32)?;
                if (global_idx as usize) >= globals.len() || globals[global_idx as usize].import.is_none() {
                    return Err(Error::validation(UNKNOWN_GLOBAL));
                }
                if globals[global_idx as usize].is_mutable {
                    return Err(Error::validation(CONST_EXP_REQUIRED));
                }
                stack.push(globals[global_idx as usize].ty);
            }
            0x41 => { // i32.const
                let _val: i32 = safe_read_sleb128(bytes, &mut it.idx, 32)?;
                stack.push(ValType::I32);
            }
            0x42 => { // i64.const
                let _val: i64 = safe_read_sleb128(bytes, &mut it.idx, 64)?;
                stack.push(ValType::I64);
            }
            0x43 => { // f32.const
                if !it.has_n_left(4) { return Err(Error::malformed(UNEXPECTED_END)); }
                it.advance(4);
                stack.push(ValType::F32);
            }
            0x44 => { // f64.const
                if !it.has_n_left(8) { return Err(Error::malformed(UNEXPECTED_END)); }
                it.advance(8);
                stack.push(ValType::F64);
            }
            0x6a..=0x6c => { // i32 add, sub, mul
                if stack.len() < 2 || stack.pop().unwrap() != ValType::I32 ||
                    *stack.last().unwrap_or(&ValType::Any) != ValType::I32 {
                    return Err(Error::validation(TYPE_MISMATCH));
                }
            }
            0x7a..=0x7c => { // i64 add, sub, mul
                if stack.len() < 2 || stack.pop().unwrap() != ValType::I64 ||
                    *stack.last().unwrap_or(&ValType::Any) != ValType::I64 {
                    return Err(Error::validation(TYPE_MISMATCH));
                }
            }
            other => {
                let is_valid_instruction = get_validators()[other as usize] as usize != validate_missing as usize;
                return if is_valid_instruction {
                    Err(Error::validation(CONST_EXP_REQUIRED))
                } else {
                    Err(Error::malformed(ILLEGAL_OP))
                }
            }
        }
    }

    if !(stack.len() == 1 && stack[0] == expected) { return Err(Error::validation(TYPE_MISMATCH)); }
    Ok(())
}

// ---------------- Function Validation ----------------
pub struct Validator<'a> {
    module: &'a mut Module,
}

impl<'a> Validator<'a> {
    pub fn new(module: &'a mut Module) -> Self {
        Self { module }
    }
    
    pub fn validate_function(&mut self, func_idx: usize) -> Result<(), Error> {
        let func = self.module.functions[func_idx].clone();
        let bytes = self.module.bytes.clone();
        let mut it = ByteIter::new(&bytes, func.body.start);
        let mut vs = ValidatorStack::new();
        
        // Push function parameters onto stack first
        vs.push_vals(&func.ty.params);
        
        // Function frame - special case, doesn't use push_ctrl
        // Height is set after parameters are pushed
        vs.push_frame(ControlFrame { 
            sig: func.ty.clone(), 
            height: func.ty.params.len(),  // Stack height after params
            unreachable: false,
            control_type: ControlType::Function,
            sig_pc: func.body.start.saturating_sub(1),
        });

        // Validation loop
        loop {
            let opcode = it.read_u8()?;
            match get_validators()[opcode as usize](self.module, &mut it, &func, &mut vs) {
                Ok(Action::Continue) => continue,
                Ok(Action::End) => break,
                Err(e) => return Err(e),
            }
        }

        let last = bytes[it.cur() - 1];
        if last != 0x0b { 
            return Err(Error::malformed(END_EXPECTED)); 
        }
        if it.cur() != func.body.end { 
            return Err(Error::malformed(SECTION_SIZE_MISMATCH)); 
        }
        Ok(())
    }
}

// ---------------- Validator Function Type ----------------
type ValidatorFn = fn(&mut Module, &mut ByteIter, &Function, &mut ValidatorStack) -> Result<Action, Error>;

fn validate_missing(_: &mut Module, _: &mut ByteIter, _: &Function, _: &mut ValidatorStack) -> Result<Action, Error> {
    Err(Error::malformed(UNKNOWN_INSTRUCTION))
}

// ---------------- Control Flow Validators ----------------
fn validate_unreachable(_: &mut Module, _: &mut ByteIter, _: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    vs.unreachable();
    Ok(Action::Continue)
}

fn validate_nop(_: &mut Module, _: &mut ByteIter, _: &Function, _: &mut ValidatorStack) -> Result<Action, Error> {
    Ok(Action::Continue)
}

fn validate_block(m: &mut Module, it: &mut ByteIter, _: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    let sig_pc = it.cur();
    let sig = Signature::read(&m.types, &m.bytes, &mut it.idx)?;
    let block_start = it.cur();
    vs.pop_vals(&sig.params)?;
    let params_len = sig.params.len() as u16;
    let has_result = sig.result.is_some();
    vs.push_ctrl(sig, ControlType::Block { start: block_start }, sig_pc)?;
    m.side_table.put_sig(sig_pc, block_start, params_len, has_result);
    Ok(Action::Continue)
}

fn validate_loop(m: &mut Module, it: &mut ByteIter, _: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    let sig_pc = it.cur();
    let sig = Signature::read(&m.types, &m.bytes, &mut it.idx)?;
    let loop_body_pc = it.cur(); // body starts here
    vs.pop_vals(&sig.params)?;
    let params_len = sig.params.len() as u16;
    let has_result = sig.result.is_some();
    vs.push_ctrl(sig, ControlType::Loop, sig_pc)?;
    m.side_table.put_sig(sig_pc, loop_body_pc, params_len, has_result);
    Ok(Action::Continue)
}

fn validate_if(m: &mut Module, it: &mut ByteIter, _: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    let sig_pc = it.cur();
    let sig = Signature::read(&m.types, &m.bytes, &mut it.idx)?;
    vs.pop_val_expect(ValType::I32)?;
    vs.pop_vals(&sig.params)?;
    let if_body_pc = it.cur();
    let params_len = sig.params.len() as u16;
    let has_result = sig.result.is_some();
    vs.push_ctrl(sig, ControlType::If { start: if_body_pc }, sig_pc)?;
    m.side_table.put_sig(sig_pc, if_body_pc, params_len, has_result);
    Ok(Action::Continue)
}

fn validate_else(_: &mut Module, it: &mut ByteIter, _: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    if vs.frame_count() == 0 {
        return Err(Error::validation(ELSE_MUST_CLOSE_IF));
    }
    
    // Check that we're in an if block
    match vs.last_frame().unwrap().control_type {
        ControlType::If { .. } => {},
        _ => return Err(Error::validation(ELSE_MUST_CLOSE_IF)),
    }
    
    // Pop the if block's results and check types
    if let Some(result) = vs.last_frame().unwrap().sig.result {
        vs.pop_val_expect(result)?;
    }
    let frame = vs.pop_frame().unwrap();
    if vs.size() != frame.height {
        vs.push_frame(frame);  // Restore frame on error
        return Err(Error::validation(TYPE_MISMATCH));
    }
    
    // Update control type for else branch
    let else_start = it.cur();
    let new_control_type = match frame.control_type {
        ControlType::If { start } => ControlType::IfElse { if_start: start, else_start },
        _ => return Err(Error::validation(ELSE_MUST_CLOSE_IF)),
    };
    
    // Push else frame with same signature
    let params = frame.sig.params.clone();
    vs.push_frame(ControlFrame {
        sig: frame.sig,
        height: frame.height,
        unreachable: false,
        control_type: new_control_type,
        sig_pc: frame.sig_pc,
    });
    vs.push_vals(&params);
    Ok(Action::Continue)
}

fn validate_end(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    if vs.frame_count() == 1 { // function end
        // Check function results
        if let Some(result) = f.ty.result {
            vs.pop_val_expect(result)?;
        }
        // Stack should be back to just the parameters
        if vs.size() != f.ty.params.len() {
            return Err(Error::validation(TYPE_MISMATCH));
        }
        return Ok(Action::End);
    }
    
    // Pop expected results before removing frame
    if let Some(result) = vs.last_frame().unwrap().sig.result {
        vs.pop_val_expect(result)?;
    }
    let frame = vs.pop_frame().unwrap();
    if vs.size() != frame.height {
        return Err(Error::validation(TYPE_MISMATCH));
    }
    
    // Handle jump offset tracking
    match frame.control_type {
        ControlType::Block { .. } => {
            let sig_pc_abs = frame.sig_pc;
            let end_abs = it.cur();
            m.side_table.fill_end_else(sig_pc_abs, end_abs, end_abs);
        }
        ControlType::Loop => {}
        ControlType::If { .. } => {
            // For if without else, params must equal results
            let results_as_vec: Vec<ValType> = frame.sig.result.into_iter().collect();
            if frame.sig.params != results_as_vec {
                return Err(Error::validation(TYPE_MISMATCH));
            }
            let else_off = it.cur() - 1;
            let end_off = it.cur();
            m.side_table.fill_end_else(frame.sig_pc, end_off, else_off);
        }
        ControlType::IfElse { else_start, .. } => {
            let end_abs = it.cur();
            m.side_table.fill_end_else(frame.sig_pc, end_abs, else_start);
        }
        ControlType::Function => {}
    }
    
    // Push block results
    if let Some(result) = frame.sig.result {
        vs.push_val(result);
    }
    Ok(Action::Continue)
}

fn validate_br(m: &mut Module, it: &mut ByteIter, _: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    let depth: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    if (depth as usize) >= vs.frame_count() {
        return Err(Error::validation(UNKNOWN_LABEL));
    }
    let target = vs.get_frame(vs.frame_count() - (depth as usize) - 1).unwrap();
    // For loops, pop params; for others, pop result if any
    match target.control_type {
        ControlType::Loop => {
            let params = target.sig.params.clone();
            vs.pop_vals(&params)?;
        },
        _ => {
            if let Some(result) = target.sig.result {
                vs.pop_val_expect(result)?;
            }
        }
    }
    vs.unreachable();
    Ok(Action::Continue)
}

fn validate_br_if(m: &mut Module, it: &mut ByteIter, _: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    let depth: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    if (depth as usize) >= vs.frame_count() {
        return Err(Error::validation(UNKNOWN_LABEL));
    }
    vs.pop_val_expect(ValType::I32)?;
    let target = vs.get_frame(vs.frame_count() - (depth as usize) - 1).unwrap();
    // For loops, pop and push params; for others, pop and push result if any
    match target.control_type {
        ControlType::Loop => {
            let params = target.sig.params.clone();
            let popped = vs.pop_vals(&params)?;
            vs.push_vals(&popped);
        },
        _ => {
            if let Some(result) = target.sig.result {
                let popped = vs.pop_val_expect(result)?;
                vs.push_val(popped);
            }
        }
    }
    Ok(Action::Continue)
}

fn validate_br_table(m: &mut Module, it: &mut ByteIter, _: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    vs.pop_val_expect(ValType::I32)?;
    
    let n_targets: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    let mut targets: Vec<u32> = Vec::with_capacity(n_targets as usize + 1);
    for _ in 0..n_targets {
        let lab: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
        targets.push(lab);
    }
    if it.empty() || m.bytes[it.cur()] == 0x0b {
        return Err(Error::malformed(UNEXPECTED_END));
    }
    let default_lab: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    targets.push(default_lab);

    // Check all labels are valid
    for &lab in &targets {
        if (lab as usize) >= vs.frame_count() {
            return Err(Error::validation(UNKNOWN_LABEL));
        }
    }

    // Get default label types for consistency check
    let default_frame = vs.get_frame(vs.frame_count() - (default_lab as usize) - 1).unwrap();
    let expected_types = match default_frame.control_type {
        ControlType::Loop => default_frame.sig.params.clone(),
        _ => default_frame.sig.result.into_iter().collect(),
    };
    
    // Check all targets have same types
    for &depth in &targets {
        let target = vs.get_frame(vs.frame_count() - (depth as usize) - 1).unwrap();
        let target_types: Vec<ValType> = match target.control_type {
            ControlType::Loop => target.sig.params.clone(),
            _ => target.sig.result.into_iter().collect(),
        };
        if target_types != expected_types {
            return Err(Error::validation(TYPE_MISMATCH));
        }
    }
    
    // Pop the verified types and mark unreachable
    vs.pop_vals(&expected_types)?;
    vs.unreachable();
    Ok(Action::Continue)
}

fn validate_return(_: &mut Module, _: &mut ByteIter, _: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    // Return targets the function frame (first frame)
    if vs.frame_count() == 0 {
        return Err(Error::validation(UNKNOWN_LABEL));
    }
    let target = vs.get_frame(0).unwrap();  // Function frame is at index 0
    // For return, always use the function's result types (not label types)
    if let Some(result) = target.sig.result {
        vs.pop_val_expect(result)?;
    }
    vs.unreachable();
    Ok(Action::Continue)
}

// ---------------- Stack Manipulation ----------------
fn validate_drop(_: &mut Module, _: &mut ByteIter, _: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    vs.pop_val()?;
    Ok(Action::Continue)
}

fn validate_select(_: &mut Module, _: &mut ByteIter, _: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    vs.pop_val_expect(ValType::I32)?;
    let t1 = vs.pop_val()?;
    let t2 = vs.pop_val()?;
    
    // For WASM 1.0, only numeric types are allowed
    if !is_val_type(t1 as u8) && t1 != ValType::Any {
        return Err(Error::validation(TYPE_MISMATCH));
    }
    if !is_val_type(t2 as u8) && t2 != ValType::Any {
        return Err(Error::validation(TYPE_MISMATCH));
    }
    
    // Types must match (or be Unknown)
    if t1 != t2 && t1 != ValType::Any && t2 != ValType::Any {
        return Err(Error::validation(TYPE_MISMATCH));
    }
    
    // Push the known type, or Unknown if both are Unknown
    let result_type = if t1 == ValType::Any { t2 } else { t1 };
    vs.push_val(result_type);
    Ok(Action::Continue)
}

// ---------------- Variable Instructions ----------------
fn validate_local_get(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    let local_idx: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    if (local_idx as usize) >= f.locals.len() {
        return Err(Error::validation(UNKNOWN_LOCAL));
    }
    vs.push_val(f.locals[local_idx as usize]);
    Ok(Action::Continue)
}

fn validate_local_set(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    let local_idx: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    if (local_idx as usize) >= f.locals.len() {
        return Err(Error::validation(UNKNOWN_LOCAL));
    }
    vs.pop_val_expect(f.locals[local_idx as usize])?;
    Ok(Action::Continue)
}

fn validate_local_tee(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    let local_idx: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    if (local_idx as usize) >= f.locals.len() {
        return Err(Error::validation(UNKNOWN_LOCAL));
    }
    let ty = f.locals[local_idx as usize];
    vs.pop_val_expect(ty)?;
    vs.push_val(ty);
    Ok(Action::Continue)
}

fn validate_global_get(m: &mut Module, it: &mut ByteIter, _: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    let global_idx: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    if (global_idx as usize) >= m.globals.len() {
        return Err(Error::validation(UNKNOWN_GLOBAL));
    }
    vs.push_val(m.globals[global_idx as usize].ty);
    Ok(Action::Continue)
}

fn validate_global_set(m: &mut Module, it: &mut ByteIter, _: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    let global_idx: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    if (global_idx as usize) >= m.globals.len() {
        return Err(Error::validation(UNKNOWN_GLOBAL));
    } else if !m.globals[global_idx as usize].is_mutable {
        return Err(Error::validation(GLOBAL_IS_IMMUTABLE));
    }
    vs.pop_val_expect(m.globals[global_idx as usize].ty)?;
    Ok(Action::Continue)
}

// ---------------- Memory Instructions ----------------
macro_rules! assert_valid_memory {
    ($it:expr, $m:expr) => {
        let flag = $it.read_u8()?;
        if flag != 0 {
            return Err(Error::malformed(ZERO_FLAG_EXPECTED));
        } else if $m.memory.is_none() {
            return Err(Error::validation(UNKNOWN_MEMORY));
        }
    };
}

fn validate_memory_size(m: &mut Module, it: &mut ByteIter, _: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    assert_valid_memory!(it, m);
    vs.push_val(ValType::I32);
    Ok(Action::Continue)
}

fn validate_memory_grow(m: &mut Module, it: &mut ByteIter, _: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    assert_valid_memory!(it, m);
    vs.pop_val_expect(ValType::I32)?;
    vs.push_val(ValType::I32);
    Ok(Action::Continue)
}

// ---------------- Constant Instructions ----------------
fn validate_i32const(m: &mut Module, it: &mut ByteIter, _: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    let _val: i32 = safe_read_sleb128(&m.bytes, &mut it.idx, 32)?;
    vs.push_val(ValType::I32);
    Ok(Action::Continue)
}

fn validate_i64const(m: &mut Module, it: &mut ByteIter, _: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    let _val: i64 = safe_read_sleb128(&m.bytes, &mut it.idx, 64)?;
    vs.push_val(ValType::I64);
    Ok(Action::Continue)
}

fn validate_f32const(_: &mut Module, it: &mut ByteIter, _: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    if !it.has_n_left(4) {
        return Err(Error::malformed(UNEXPECTED_END));
    }
    it.advance(4);
    vs.push_val(ValType::F32);
    Ok(Action::Continue)
}

fn validate_f64const(_: &mut Module, it: &mut ByteIter, _: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    if !it.has_n_left(8) {
        return Err(Error::malformed(UNEXPECTED_END));
    }
    it.advance(8);
    vs.push_val(ValType::F64);
    Ok(Action::Continue)
}

// ---------------- Numeric Operations ----------------
macro_rules! numeric {
    ($name:ident, $in:expr, $out:expr) => {
        fn $name(_: &mut Module, _: &mut ByteIter, _: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
            vs.pop_vals($in)?;
            for &t in $out { vs.push_val(t); }
            Ok(Action::Continue)
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
fn validate_load(m: &mut Module, it: &mut ByteIter, val_ty: ValType, natural_align: u32, _: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    let align_bits: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    if m.memory.is_none() {
        return Err(Error::validation(UNKNOWN_MEMORY));
    }
    if align_bits >= 32 {
        return Err(Error::malformed(INT_TOO_LARGE));
    }
    let _off: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    let align = 1u64 << align_bits;
    if align > natural_align as u64 {
        return Err(Error::validation(ALIGNMENT_TOO_LARGE));
    }
    vs.pop_val_expect(ValType::I32)?;
    vs.push_val(val_ty);
    Ok(Action::Continue)
}

fn validate_store(m: &mut Module, it: &mut ByteIter, val_ty: ValType, natural_align: u32, _: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    let mut align_bits: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    if (1 << 6) & align_bits != 0 {
        align_bits = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    } else if m.memory.is_none() {
        return Err(Error::validation(UNKNOWN_MEMORY));
    }
    if align_bits >= 32 {
        return Err(Error::malformed(INT_TOO_LARGE));
    }
    let _off: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    let align = 1u64 << align_bits;
    if align > natural_align as u64 {
        return Err(Error::validation(ALIGNMENT_TOO_LARGE));
    }
    vs.pop_val_expect(val_ty)?;
    vs.pop_val_expect(ValType::I32)?;
    Ok(Action::Continue)
}

macro_rules! load {
    ($name:ident, $ty:expr, $align:expr) => {
        fn $name(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
            validate_load(m, it, $ty, $align, f, vs)
        }
    }
}

macro_rules! store {
    ($name:ident, $ty:expr, $align:expr) => {
        fn $name(m: &mut Module, it: &mut ByteIter, f: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
            validate_store(m, it, $ty, $align, f, vs)
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
fn validate_call(m: &mut Module, it: &mut ByteIter, _: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    let func_idx: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    if (func_idx as usize) >= m.functions.len() {
        return Err(Error::validation(UNKNOWN_FUNC));
    }
    let sig = &m.functions[func_idx as usize].ty;
    vs.pop_vals(&sig.params)?;
    if let Some(result) = sig.result {
        vs.push_val(result);
    }
    Ok(Action::Continue)
}

fn validate_call_indirect(m: &mut Module, it: &mut ByteIter, _: &Function, vs: &mut ValidatorStack) -> Result<Action, Error> {
    let type_idx: u32 = safe_read_leb128(&m.bytes, &mut it.idx, 32)?;
    if (type_idx as usize) >= m.types.len() {
        return Err(Error::validation(UNKNOWN_TYPE));
    }
    let flag = it.read_u8()?;
    if flag != 0 {
        return Err(Error::malformed(ZERO_FLAG_EXPECTED));
    } else if m.table.is_none() {
        return Err(Error::validation(UNKNOWN_TABLE));
    }
    vs.pop_val_expect(ValType::I32)?;
    let sig = &m.types[type_idx as usize];
    vs.pop_vals(&sig.params)?;
    if let Some(result) = sig.result {
        vs.push_val(result);
    }
    Ok(Action::Continue)
}

// ---------------- Validator Table ----------------
#[allow(clippy::all)]
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
        t[0x20] = validate_local_get; t[0x21] = validate_local_set;
        t[0x22] = validate_local_tee; t[0x23] = validate_global_get;
        t[0x24] = validate_global_set;
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
        t[0x3f] = validate_memory_size; t[0x40] = validate_memory_grow;
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
    &VALIDATORS
}