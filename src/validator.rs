use crate::error::*;
use crate::leb128::*;
use crate::module::*;
use crate::opcodes::*;
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

#[derive(Clone)]
pub struct ControlFrame {
    pub sig: Signature,
    pub height: usize,
    pub unreachable: bool,
    pub control_type: ControlType,
    pub sig_pc: usize,
}

// ---------------- Stack for Type Checking ----------------
pub struct Stack {
    val_stack: Vec<ValType>,
    ctrl_stack: Vec<ControlFrame>,
}

#[rustfmt::skip]
impl Stack {
    pub fn new() -> Self { Self { val_stack: Vec::with_capacity(1024), ctrl_stack: Vec::with_capacity(64) } }
    pub fn size(&self) -> usize { self.val_stack.len() }
    pub fn push_val(&mut self, ty: ValType) { self.val_stack.push(ty); }
    pub fn push_vals(&mut self, types: &[ValType]) { self.val_stack.extend_from_slice(types); }
    pub fn frame_count(&self) -> usize { self.ctrl_stack.len() }
    pub fn last_frame(&self) -> Option<&ControlFrame> { self.ctrl_stack.last() }
    pub fn get_frame(&self, index: usize) -> Option<&ControlFrame> { self.ctrl_stack.get(index) }
    pub fn push_frame(&mut self, frame: ControlFrame) { self.ctrl_stack.push(frame); }
    pub fn pop_frame(&mut self) -> Option<ControlFrame> { self.ctrl_stack.pop() }
}

impl Stack {
    pub fn pop_val(&mut self) -> Result<ValType, Error> {
        if self.ctrl_stack.is_empty() {
            return Err(Error::validation(TYPE_MISMATCH));
        }
        let frame = self.ctrl_stack.last().unwrap();

        if self.val_stack.len() == frame.height {
            if frame.unreachable {
                return Ok(ValType::Any);
            }
            return Err(Error::validation(TYPE_MISMATCH));
        }

        if self.val_stack.len() < frame.height {
            return Err(Error::validation(TYPE_MISMATCH));
        }
        Ok(self.val_stack.pop().unwrap())
    }

    pub fn pop_val_expect(&mut self, expect: ValType) -> Result<ValType, Error> {
        let actual = self.pop_val()?;
        if actual == ValType::Any {
            return Ok(expect);
        }
        if expect == ValType::Any {
            return Ok(actual);
        }
        if actual != expect {
            return Err(Error::validation(TYPE_MISMATCH));
        }
        Ok(actual)
    }

    pub fn pop_vals(&mut self, types: &[ValType]) -> Result<Vec<ValType>, Error> {
        let mut popped = Vec::with_capacity(types.len());
        for &ty in types.iter().rev() {
            popped.push(self.pop_val_expect(ty)?);
        }
        popped.reverse();
        Ok(popped)
    }

    pub fn push_ctrl(
        &mut self,
        sig: Signature,
        control_type: ControlType,
        sig_pc: usize,
    ) -> Result<(), Error> {
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
pub fn v_const(
    bytes: &[u8],
    i: &mut usize,
    expected: ValType,
    globals: &[Global],
) -> Result<(), Error> {
    let mut stack: Vec<ValType> = Vec::with_capacity(4);
    loop {
        let byte = read_byte(bytes, i)?;
        if byte == END {
            // end
            break;
        }
        match byte {
            GLOBAL_GET => {
                // global.get
                let global_idx: u32 = safe_read_leb128(bytes, i, 32)?;
                if (global_idx as usize) >= globals.len()
                    || globals[global_idx as usize].import.is_none()
                {
                    return Err(Error::validation(UNKNOWN_GLOBAL));
                }
                if globals[global_idx as usize].is_mutable {
                    return Err(Error::validation(CONST_EXP_REQUIRED));
                }
                stack.push(globals[global_idx as usize].ty);
            }
            I32_CONST => {
                // i32.const
                let _val: i32 = safe_read_sleb128(bytes, i, 32)?;
                stack.push(ValType::I32);
            }
            I64_CONST => {
                // i64.const
                let _val: i64 = safe_read_sleb128(bytes, i, 64)?;
                stack.push(ValType::I64);
            }
            F32_CONST => {
                // f32.const
                if *i + 4 > bytes.len() {
                    return Err(Error::malformed(UNEXPECTED_END));
                }
                *i += 4;
                stack.push(ValType::F32);
            }
            F64_CONST => {
                // f64.const
                if *i + 8 > bytes.len() {
                    return Err(Error::malformed(UNEXPECTED_END));
                }
                *i += 8;
                stack.push(ValType::F64);
            }
            I32_ADD..=I32_MUL => {
                // i32 add, sub, mul
                if stack.len() < 2
                    || stack.pop().unwrap() != ValType::I32
                    || *stack.last().unwrap_or(&ValType::Any) != ValType::I32
                {
                    return Err(Error::validation(TYPE_MISMATCH));
                }
            }
            I64_CTZ..=I64_ADD => {
                // i64 add, sub, mul
                if stack.len() < 2
                    || stack.pop().unwrap() != ValType::I64
                    || *stack.last().unwrap_or(&ValType::Any) != ValType::I64
                {
                    return Err(Error::validation(TYPE_MISMATCH));
                }
            }
            other => {
                let is_valid_instruction =
                    get_validators()[other as usize] as *const () != v_missing as *const ();
                return if is_valid_instruction {
                    Err(Error::validation(CONST_EXP_REQUIRED))
                } else {
                    Err(Error::malformed(ILLEGAL_OP))
                };
            }
        }
    }

    if !(stack.len() == 1 && stack[0] == expected) {
        return Err(Error::validation(TYPE_MISMATCH));
    }
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

    pub fn v_function(&mut self, func_idx: usize) -> Result<(), Error> {
        let func = self.module.functions[func_idx].clone();
        let bytes = self.module.bytes.clone();
        let mut i: usize = func.body.start;
        let mut s = Stack::new();

        // Push function parameters onto stack first
        s.push_vals(&func.ty.params);

        // Function frame - special case, doesn't use push_ctrl
        // Height is set after parameters are pushed
        s.push_frame(ControlFrame {
            sig: func.ty.clone(),
            height: func.ty.params.len(), // Stack height after params
            unreachable: false,
            control_type: ControlType::Function,
            sig_pc: func.body.start.saturating_sub(1),
        });

        // Validation loop
        loop {
            let opcode = read_byte(&bytes, &mut i)?;
            get_validators()[opcode as usize](self.module, &mut i, &func, &mut s)?;
            if s.frame_count() == 0 {
                break;
            }
        }

        let last = bytes[i - 1];
        if last != END {
            return Err(Error::malformed(END_EXPECTED));
        }
        if i != func.body.end {
            return Err(Error::malformed(SECTION_SIZE_MISMATCH));
        }
        Ok(())
    }
}

// ---------------- Validator Function Type ----------------
type ValidatorFn = fn(&mut Module, &mut usize, &Function, &mut Stack) -> Result<(), Error>;

fn v_missing(_: &mut Module, _: &mut usize, _: &Function, _: &mut Stack) -> Result<(), Error> {
    Err(Error::malformed(UNKNOWN_INSTRUCTION))
}

// ---------------- Control Flow Validators ----------------
fn v_unreachable(_: &mut Module, _: &mut usize, _: &Function, s: &mut Stack) -> Result<(), Error> {
    s.unreachable();
    Ok(())
}

fn v_nop(_: &mut Module, _: &mut usize, _: &Function, _: &mut Stack) -> Result<(), Error> {
    Ok(())
}

fn v_block(m: &mut Module, i: &mut usize, _: &Function, s: &mut Stack) -> Result<(), Error> {
    let sig_pc = *i;
    let sig = Signature::read(&m.types, &m.bytes, i)?;
    let block_start = *i;
    s.pop_vals(&sig.params)?;
    let params_len = sig.params.len() as u16;
    let has_result = sig.result.is_some();
    s.push_ctrl(sig, ControlType::Block { start: block_start }, sig_pc)?;
    m.side_table.put_sig(sig_pc, block_start, params_len, has_result);
    Ok(())
}

fn v_loop(m: &mut Module, i: &mut usize, _: &Function, s: &mut Stack) -> Result<(), Error> {
    let sig_pc = *i;
    let sig = Signature::read(&m.types, &m.bytes, i)?;
    let loop_body_pc = *i; // body starts here
    s.pop_vals(&sig.params)?;
    let params_len = sig.params.len() as u16;
    let has_result = sig.result.is_some();
    s.push_ctrl(sig, ControlType::Loop, sig_pc)?;
    m.side_table.put_sig(sig_pc, loop_body_pc, params_len, has_result);
    Ok(())
}

fn v_if(m: &mut Module, i: &mut usize, _: &Function, s: &mut Stack) -> Result<(), Error> {
    let sig_pc = *i;
    let sig = Signature::read(&m.types, &m.bytes, i)?;
    s.pop_val_expect(ValType::I32)?;
    s.pop_vals(&sig.params)?;
    let if_body_pc = *i;
    let params_len = sig.params.len() as u16;
    let has_result = sig.result.is_some();
    s.push_ctrl(sig, ControlType::If { start: if_body_pc }, sig_pc)?;
    m.side_table.put_sig(sig_pc, if_body_pc, params_len, has_result);
    Ok(())
}

fn v_else(_: &mut Module, i: &mut usize, _: &Function, s: &mut Stack) -> Result<(), Error> {
    if s.frame_count() == 0 {
        return Err(Error::validation(ELSE_MUST_CLOSE_IF));
    }

    // Check that we're in an if block
    match s.last_frame().unwrap().control_type {
        ControlType::If { .. } => {}
        _ => return Err(Error::validation(ELSE_MUST_CLOSE_IF)),
    }

    // Pop the if block's results and check types
    if let Some(result) = s.last_frame().unwrap().sig.result {
        s.pop_val_expect(result)?;
    }
    let frame = s.pop_frame().unwrap();
    if s.size() != frame.height {
        s.push_frame(frame); // Restore frame on error
        return Err(Error::validation(TYPE_MISMATCH));
    }

    // Update control type for else branch
    let else_start = *i;
    let new_control_type = match frame.control_type {
        ControlType::If { start } => ControlType::IfElse { if_start: start, else_start },
        _ => return Err(Error::validation(ELSE_MUST_CLOSE_IF)),
    };

    s.push_vals(&frame.sig.params);
    s.push_frame(ControlFrame {
        sig: frame.sig,
        height: frame.height,
        unreachable: false,
        control_type: new_control_type,
        sig_pc: frame.sig_pc,
    });
    Ok(())
}

fn v_end(m: &mut Module, i: &mut usize, f: &Function, s: &mut Stack) -> Result<(), Error> {
    if s.frame_count() == 1 {
        // function end
        // Check function results
        if let Some(result) = f.ty.result {
            s.pop_val_expect(result)?;
        }
        // Stack should be back to just the parameters
        if s.size() != f.ty.params.len() {
            return Err(Error::validation(TYPE_MISMATCH));
        }
        s.pop_frame();
        return Ok(());
    }

    // Pop expected results before removing frame
    if let Some(result) = s.last_frame().unwrap().sig.result {
        s.pop_val_expect(result)?;
    }
    let frame = s.pop_frame().unwrap();
    if s.size() != frame.height {
        return Err(Error::validation(TYPE_MISMATCH));
    }

    // Handle jump offset tracking
    match frame.control_type {
        ControlType::Block { .. } => {
            let sig_pc_abs = frame.sig_pc;
            let end_abs = *i;
            m.side_table.fill_end_else(sig_pc_abs, end_abs, end_abs);
        }
        ControlType::Loop => {}
        ControlType::If { .. } => {
            // For if without else, params must equal results
            let results_as_vec: Vec<ValType> = frame.sig.result.into_iter().collect();
            if frame.sig.params != results_as_vec {
                return Err(Error::validation(TYPE_MISMATCH));
            }
            let else_off = *i - 1;
            let end_off = *i;
            m.side_table.fill_end_else(frame.sig_pc, end_off, else_off);
        }
        ControlType::IfElse { else_start, .. } => {
            let end_abs = *i;
            m.side_table.fill_end_else(frame.sig_pc, end_abs, else_start);
        }
        ControlType::Function => {}
    }

    // Push block results
    if let Some(result) = frame.sig.result {
        s.push_val(result);
    }
    Ok(())
}

fn v_br(m: &mut Module, i: &mut usize, _: &Function, s: &mut Stack) -> Result<(), Error> {
    let depth: u32 = safe_read_leb128(&m.bytes, i, 32)?;
    if (depth as usize) >= s.frame_count() {
        return Err(Error::validation(UNKNOWN_LABEL));
    }
    let target = s.get_frame(s.frame_count() - (depth as usize) - 1).unwrap();
    match target.control_type {
        ControlType::Loop => {
            let params = target.sig.params.clone();
            s.pop_vals(&params)?;
        }
        _ => {
            if let Some(result) = target.sig.result {
                s.pop_val_expect(result)?;
            }
        }
    }
    s.unreachable();
    Ok(())
}

fn v_br_if(m: &mut Module, i: &mut usize, _: &Function, s: &mut Stack) -> Result<(), Error> {
    let depth: u32 = safe_read_leb128(&m.bytes, i, 32)?;
    if (depth as usize) >= s.frame_count() {
        return Err(Error::validation(UNKNOWN_LABEL));
    }
    s.pop_val_expect(ValType::I32)?;
    let target = s.get_frame(s.frame_count() - (depth as usize) - 1).unwrap();
    match target.control_type {
        ControlType::Loop => {
            let params = target.sig.params.clone();
            let popped = s.pop_vals(&params)?;
            s.push_vals(&popped);
        }
        _ => {
            if let Some(result) = target.sig.result {
                let popped = s.pop_val_expect(result)?;
                s.push_val(popped);
            }
        }
    }
    Ok(())
}

fn v_br_table(m: &mut Module, i: &mut usize, _: &Function, s: &mut Stack) -> Result<(), Error> {
    let br_pc = *i; // PC right after the 0x0e opcode
    s.pop_val_expect(ValType::I32)?;

    let n_targets: u32 = safe_read_leb128(&m.bytes, i, 32)?;
    let mut targets: Vec<u32> = Vec::with_capacity(n_targets as usize + 1);
    for _ in 0..n_targets {
        let lab: u32 = safe_read_leb128(&m.bytes, i, 32)?;
        targets.push(lab);
    }
    if *i >= m.bytes.len() || m.bytes[*i] == END {
        return Err(Error::malformed(UNEXPECTED_END));
    }
    let default_lab: u32 = safe_read_leb128(&m.bytes, i, 32)?;
    targets.push(default_lab);

    // Check all labels are valid
    for &lab in &targets {
        if (lab as usize) >= s.frame_count() {
            return Err(Error::validation(UNKNOWN_LABEL));
        }
    }

    // Get default label types for consistency check
    let default_frame = s.get_frame(s.frame_count() - (default_lab as usize) - 1).unwrap();
    let expected_types = match default_frame.control_type {
        ControlType::Loop => default_frame.sig.params.clone(),
        _ => default_frame.sig.result.into_iter().collect(),
    };

    // Check all targets have same types
    for &depth in &targets {
        let target = s.get_frame(s.frame_count() - (depth as usize) - 1).unwrap();
        let target_types: Vec<ValType> = match target.control_type {
            ControlType::Loop => target.sig.params.clone(),
            _ => target.sig.result.into_iter().collect(),
        };
        if target_types != expected_types {
            return Err(Error::validation(TYPE_MISMATCH));
        }
    }

    // Pop the verified types and mark unreachable
    s.pop_vals(&expected_types)?;
    s.unreachable();
    m.side_table.put_br_table(br_pc, &targets);
    Ok(())
}

fn v_return(_: &mut Module, _: &mut usize, _: &Function, s: &mut Stack) -> Result<(), Error> {
    // Return targets the function frame (first frame)
    if s.frame_count() == 0 {
        return Err(Error::validation(UNKNOWN_LABEL));
    }
    let target = s.get_frame(0).unwrap(); // Function frame is at index 0
                                          // For return, always use the function's result types (not label types)
    if let Some(result) = target.sig.result {
        s.pop_val_expect(result)?;
    }
    s.unreachable();
    Ok(())
}

// ---------------- Stack Manipulation ----------------
fn v_drop(_: &mut Module, _: &mut usize, _: &Function, s: &mut Stack) -> Result<(), Error> {
    s.pop_val()?;
    Ok(())
}

fn v_select(_: &mut Module, _: &mut usize, _: &Function, s: &mut Stack) -> Result<(), Error> {
    s.pop_val_expect(ValType::I32)?;
    let t1 = s.pop_val()?;
    let t2 = s.pop_val()?;

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
    s.push_val(result_type);
    Ok(())
}

// ---------------- Variable Instructions ----------------
fn v_local_get(m: &mut Module, i: &mut usize, f: &Function, s: &mut Stack) -> Result<(), Error> {
    let local_idx: u32 = safe_read_leb128(&m.bytes, i, 32)?;
    if (local_idx as usize) >= f.locals.len() {
        return Err(Error::validation(UNKNOWN_LOCAL));
    }
    s.push_val(f.locals[local_idx as usize]);
    Ok(())
}

fn v_local_set(m: &mut Module, i: &mut usize, f: &Function, s: &mut Stack) -> Result<(), Error> {
    let local_idx: u32 = safe_read_leb128(&m.bytes, i, 32)?;
    if (local_idx as usize) >= f.locals.len() {
        return Err(Error::validation(UNKNOWN_LOCAL));
    }
    s.pop_val_expect(f.locals[local_idx as usize])?;
    Ok(())
}

fn v_local_tee(m: &mut Module, i: &mut usize, f: &Function, s: &mut Stack) -> Result<(), Error> {
    let local_idx: u32 = safe_read_leb128(&m.bytes, i, 32)?;
    if (local_idx as usize) >= f.locals.len() {
        return Err(Error::validation(UNKNOWN_LOCAL));
    }
    let ty = f.locals[local_idx as usize];
    s.pop_val_expect(ty)?;
    s.push_val(ty);
    Ok(())
}

fn v_global_get(m: &mut Module, i: &mut usize, _: &Function, s: &mut Stack) -> Result<(), Error> {
    let global_idx: u32 = safe_read_leb128(&m.bytes, i, 32)?;
    if (global_idx as usize) >= m.globals.len() {
        return Err(Error::validation(UNKNOWN_GLOBAL));
    }
    s.push_val(m.globals[global_idx as usize].ty);
    Ok(())
}

fn v_global_set(m: &mut Module, i: &mut usize, _: &Function, s: &mut Stack) -> Result<(), Error> {
    let global_idx: u32 = safe_read_leb128(&m.bytes, i, 32)?;
    if (global_idx as usize) >= m.globals.len() {
        return Err(Error::validation(UNKNOWN_GLOBAL));
    } else if !m.globals[global_idx as usize].is_mutable {
        return Err(Error::validation(GLOBAL_IS_IMMUTABLE));
    }
    s.pop_val_expect(m.globals[global_idx as usize].ty)?;
    Ok(())
}

// ---------------- Memory Instructions ----------------
macro_rules! assert_valid_memory {
    ($i:expr, $m:expr) => {
        let flag = read_byte(&$m.bytes, $i)?;
        if flag != 0 {
            return Err(Error::malformed(ZERO_FLAG_EXPECTED));
        } else if $m.memory.is_none() {
            return Err(Error::validation(UNKNOWN_MEMORY));
        }
    };
}

fn v_memory_size(m: &mut Module, i: &mut usize, _: &Function, s: &mut Stack) -> Result<(), Error> {
    assert_valid_memory!(i, m);
    s.push_val(ValType::I32);
    Ok(())
}

fn v_memory_grow(m: &mut Module, i: &mut usize, _: &Function, s: &mut Stack) -> Result<(), Error> {
    assert_valid_memory!(i, m);
    s.pop_val_expect(ValType::I32)?;
    s.push_val(ValType::I32);
    Ok(())
}

// ---------------- Constant Instructions ----------------
fn v_i32const(m: &mut Module, i: &mut usize, _: &Function, s: &mut Stack) -> Result<(), Error> {
    let _val: i32 = safe_read_sleb128(&m.bytes, i, 32)?;
    s.push_val(ValType::I32);
    Ok(())
}

fn v_i64const(m: &mut Module, i: &mut usize, _: &Function, s: &mut Stack) -> Result<(), Error> {
    let _val: i64 = safe_read_sleb128(&m.bytes, i, 64)?;
    s.push_val(ValType::I64);
    Ok(())
}

fn v_f32const(m: &mut Module, i: &mut usize, _: &Function, s: &mut Stack) -> Result<(), Error> {
    if *i + 4 > m.bytes.len() {
        return Err(Error::malformed(UNEXPECTED_END));
    }
    *i += 4;
    s.push_val(ValType::F32);
    Ok(())
}

fn v_f64const(m: &mut Module, i: &mut usize, _: &Function, s: &mut Stack) -> Result<(), Error> {
    if *i + 8 > m.bytes.len() {
        return Err(Error::malformed(UNEXPECTED_END));
    }
    *i += 8;
    s.push_val(ValType::F64);
    Ok(())
}

// ---------------- Numeric Operations ----------------
macro_rules! numeric {
    ($name:ident, $in:expr, $out:expr) => {
        fn $name(_: &mut Module, _: &mut usize, _: &Function, s: &mut Stack) -> Result<(), Error> {
            s.pop_vals($in)?;
            for &t in $out {
                s.push_val(t);
            }
            Ok(())
        }
    };
}

numeric!(v_i32_i32, &[ValType::I32], &[ValType::I32]);
numeric!(v_i64_i64, &[ValType::I64], &[ValType::I64]);
numeric!(v_f32_f32, &[ValType::F32], &[ValType::F32]);
numeric!(v_f64_f64, &[ValType::F64], &[ValType::F64]);
numeric!(v_i32i32_i32, &[ValType::I32, ValType::I32], &[ValType::I32]);
numeric!(v_i64i64_i64, &[ValType::I64, ValType::I64], &[ValType::I64]);
numeric!(v_f32f32_f32, &[ValType::F32, ValType::F32], &[ValType::F32]);
numeric!(v_f64f64_f64, &[ValType::F64, ValType::F64], &[ValType::F64]);
numeric!(v_i64_i32, &[ValType::I64], &[ValType::I32]);
numeric!(v_f32_i32, &[ValType::F32], &[ValType::I32]);
numeric!(v_f64_i32, &[ValType::F64], &[ValType::I32]);
numeric!(v_i64i64_i32, &[ValType::I64, ValType::I64], &[ValType::I32]);
numeric!(v_f32f32_i32, &[ValType::F32, ValType::F32], &[ValType::I32]);
numeric!(v_f64f64_i32, &[ValType::F64, ValType::F64], &[ValType::I32]);
numeric!(v_i32_i64, &[ValType::I32], &[ValType::I64]);
numeric!(v_f32_i64, &[ValType::F32], &[ValType::I64]);
numeric!(v_f64_i64, &[ValType::F64], &[ValType::I64]);
numeric!(v_i32_f32, &[ValType::I32], &[ValType::F32]);
numeric!(v_i64_f32, &[ValType::I64], &[ValType::F32]);
numeric!(v_f64_f32, &[ValType::F64], &[ValType::F32]);
numeric!(v_i32_f64, &[ValType::I32], &[ValType::F64]);
numeric!(v_i64_f64, &[ValType::I64], &[ValType::F64]);
numeric!(v_f32_f64, &[ValType::F32], &[ValType::F64]);

// ---------------- Memory Load/Store Operations ----------------
fn v_load(
    m: &mut Module,
    i: &mut usize,
    val_ty: ValType,
    natural_align: u32,
    _: &Function,
    s: &mut Stack,
) -> Result<(), Error> {
    let align_bits: u32 = safe_read_leb128(&m.bytes, i, 32)?;
    if m.memory.is_none() {
        return Err(Error::validation(UNKNOWN_MEMORY));
    }
    if align_bits >= 32 {
        return Err(Error::malformed(INT_TOO_LARGE));
    }
    let _off: u32 = safe_read_leb128(&m.bytes, i, 32)?;
    let align = 1u64 << align_bits;
    if align > natural_align as u64 {
        return Err(Error::validation(ALIGNMENT_TOO_LARGE));
    }
    s.pop_val_expect(ValType::I32)?;
    s.push_val(val_ty);
    Ok(())
}

fn v_store(
    m: &mut Module,
    i: &mut usize,
    val_ty: ValType,
    natural_align: u32,
    _: &Function,
    s: &mut Stack,
) -> Result<(), Error> {
    let mut align_bits: u32 = safe_read_leb128(&m.bytes, i, 32)?;
    if (1 << 6) & align_bits != 0 {
        align_bits = safe_read_leb128(&m.bytes, i, 32)?;
    } else if m.memory.is_none() {
        return Err(Error::validation(UNKNOWN_MEMORY));
    }
    if align_bits >= 32 {
        return Err(Error::malformed(INT_TOO_LARGE));
    }
    let _off: u32 = safe_read_leb128(&m.bytes, i, 32)?;
    let align = 1u64 << align_bits;
    if align > natural_align as u64 {
        return Err(Error::validation(ALIGNMENT_TOO_LARGE));
    }
    s.pop_val_expect(val_ty)?;
    s.pop_val_expect(ValType::I32)?;
    Ok(())
}

macro_rules! load {
    ($name:ident, $ty:expr, $align:expr) => {
        fn $name(m: &mut Module, i: &mut usize, f: &Function, s: &mut Stack) -> Result<(), Error> {
            v_load(m, i, $ty, $align, f, s)
        }
    };
}

macro_rules! store {
    ($name:ident, $ty:expr, $align:expr) => {
        fn $name(m: &mut Module, i: &mut usize, f: &Function, s: &mut Stack) -> Result<(), Error> {
            v_store(m, i, $ty, $align, f, s)
        }
    };
}

load!(v_i32load, ValType::I32, 4);
load!(v_i64load, ValType::I64, 8);
load!(v_f32load, ValType::F32, 4);
load!(v_f64load, ValType::F64, 8);
load!(v_i32load8_s, ValType::I32, 1);
load!(v_i32load8_u, ValType::I32, 1);
load!(v_i32load16_s, ValType::I32, 2);
load!(v_i32load16_u, ValType::I32, 2);
load!(v_i64load8_s, ValType::I64, 1);
load!(v_i64load8_u, ValType::I64, 1);
load!(v_i64load16_s, ValType::I64, 2);
load!(v_i64load16_u, ValType::I64, 2);
load!(v_i64load32_s, ValType::I64, 4);
load!(v_i64load32_u, ValType::I64, 4);
store!(v_i32store, ValType::I32, 4);
store!(v_i64store, ValType::I64, 8);
store!(v_f32store, ValType::F32, 4);
store!(v_f64store, ValType::F64, 8);
store!(v_i32store8, ValType::I32, 1);
store!(v_i32store16, ValType::I32, 2);
store!(v_i64store8, ValType::I64, 1);
store!(v_i64store16, ValType::I64, 2);
store!(v_i64store32, ValType::I64, 4);

// ---------------- Call Instructions ----------------
fn v_call(m: &mut Module, i: &mut usize, _: &Function, s: &mut Stack) -> Result<(), Error> {
    let func_idx: u32 = safe_read_leb128(&m.bytes, i, 32)?;
    if (func_idx as usize) >= m.functions.len() {
        return Err(Error::validation(UNKNOWN_FUNC));
    }
    let sig = &m.functions[func_idx as usize].ty;
    s.pop_vals(&sig.params)?;
    if let Some(result) = sig.result {
        s.push_val(result);
    }
    Ok(())
}

fn v_call_indirect(
    m: &mut Module,
    i: &mut usize,
    _: &Function,
    s: &mut Stack,
) -> Result<(), Error> {
    let type_idx: u32 = safe_read_leb128(&m.bytes, i, 32)?;
    if (type_idx as usize) >= m.types.len() {
        return Err(Error::validation(UNKNOWN_TYPE));
    }
    let flag = read_byte(&m.bytes, i)?;
    if flag != 0 {
        return Err(Error::malformed(ZERO_FLAG_EXPECTED));
    } else if m.table.is_none() {
        return Err(Error::validation(UNKNOWN_TABLE));
    }
    s.pop_val_expect(ValType::I32)?;
    let sig = &m.types[type_idx as usize];
    s.pop_vals(&sig.params)?;
    if let Some(result) = sig.result {
        s.push_val(result);
    }
    Ok(())
}

// ---------------- Validator Table ----------------
#[rustfmt::skip]
#[allow(clippy::all)]
fn build_validators_table() -> [ValidatorFn; 256] {
    let mut t: [ValidatorFn; 256] = [v_missing; 256];
    macro_rules! op { ($op:expr, $f:expr) => { t[$op as usize] = $f; }; }
    macro_rules! ops { ($lo:expr, $hi:expr, $f:expr) => { for i in $lo as usize..=$hi as usize { t[i] = $f; } }; }

    op!(OP_UNREACHABLE, v_unreachable); op!(NOP, v_nop);
    op!(BLOCK, v_block);                op!(LOOP, v_loop);
    op!(IF, v_if);                      op!(ELSE, v_else);
    op!(END, v_end);                    op!(BR, v_br);
    op!(BR_IF, v_br_if);                op!(BR_TABLE, v_br_table);
    op!(RETURN, v_return);              op!(CALL, v_call);
    op!(CALL_INDIRECT, v_call_indirect);
    op!(DROP, v_drop);                  op!(SELECT, v_select);
    op!(LOCAL_GET, v_local_get);        op!(LOCAL_SET, v_local_set);
    op!(LOCAL_TEE, v_local_tee);        op!(GLOBAL_GET, v_global_get);
    op!(GLOBAL_SET, v_global_set);      op!(MEMORY_SIZE, v_memory_size);
    op!(MEMORY_GROW, v_memory_grow);
    op!(I32_LOAD, v_i32load);           op!(I64_LOAD, v_i64load);
    op!(F32_LOAD, v_f32load);           op!(F64_LOAD, v_f64load);
    op!(I32_LOAD8_S, v_i32load8_s);     op!(I32_LOAD8_U, v_i32load8_u);
    op!(I32_LOAD16_S, v_i32load16_s);   op!(I32_LOAD16_U, v_i32load16_u);
    op!(I64_LOAD8_S, v_i64load8_s);     op!(I64_LOAD8_U, v_i64load8_u);
    op!(I64_LOAD16_S, v_i64load16_s);   op!(I64_LOAD16_U, v_i64load16_u);
    op!(I64_LOAD32_S, v_i64load32_s);   op!(I64_LOAD32_U, v_i64load32_u);
    op!(I32_STORE, v_i32store);         op!(I64_STORE, v_i64store);
    op!(F32_STORE, v_f32store);         op!(F64_STORE, v_f64store);
    op!(I32_STORE8, v_i32store8);       op!(I32_STORE16, v_i32store16);
    op!(I64_STORE8, v_i64store8);       op!(I64_STORE16, v_i64store16);
    op!(I64_STORE32, v_i64store32);
    op!(I32_CONST, v_i32const);         op!(I64_CONST, v_i64const);
    op!(F32_CONST, v_f32const);         op!(F64_CONST, v_f64const);
    op!(I32_EQZ, v_i32_i32);            op!(I64_EQZ, v_i64_i32);
    ops!(I32_EQ,  I32_GE_U,    v_i32i32_i32);
    ops!(I64_EQ,  I64_GE_U,    v_i64i64_i32);
    ops!(F32_EQ,  F32_GE,      v_f32f32_i32);
    ops!(F64_EQ,  F64_GE,      v_f64f64_i32);
    ops!(I32_CLZ, I32_POPCNT,  v_i32_i32);
    ops!(I32_ADD, I32_ROTR,    v_i32i32_i32);
    ops!(I64_CLZ, I64_POPCNT,  v_i64_i64);
    ops!(I64_ADD, I64_ROTR,    v_i64i64_i64);
    ops!(F32_ABS, F32_SQRT,    v_f32_f32);
    ops!(F32_ADD, F32_COPYSIGN,v_f32f32_f32);
    ops!(F64_ABS, F64_SQRT,    v_f64_f64);
    ops!(F64_ADD, F64_COPYSIGN,v_f64f64_f64);
    op!(I32_WRAP_I64, v_i64_i32);       op!(I32_TRUNC_F32_S, v_f32_i32);
    op!(I32_TRUNC_F32_U, v_f32_i32);    op!(I32_TRUNC_F64_S, v_f64_i32);
    op!(I32_TRUNC_F64_U, v_f64_i32);    op!(I64_EXTEND_I32_S, v_i32_i64);
    op!(I64_EXTEND_I32_U, v_i32_i64);   op!(I64_TRUNC_F32_S, v_f32_i64);
    op!(I64_TRUNC_F32_U, v_f32_i64);    op!(I64_TRUNC_F64_S, v_f64_i64);
    op!(I64_TRUNC_F64_U, v_f64_i64);    op!(F32_CONVERT_I32_S, v_i32_f32);
    op!(F32_CONVERT_I32_U, v_i32_f32);  op!(F32_CONVERT_I64_S, v_i64_f32);
    op!(F32_CONVERT_I64_U, v_i64_f32);  op!(F32_DEMOTE_F64, v_f64_f32);
    op!(F64_CONVERT_I32_S, v_i32_f64);  op!(F64_CONVERT_I32_U, v_i32_f64);
    op!(F64_CONVERT_I64_S, v_i64_f64);  op!(F64_CONVERT_I64_U, v_i64_f64);
    op!(F64_PROMOTE_F32, v_f32_f64);    op!(I32_REINTERPRET_F32, v_f32_i32);
    op!(I64_REINTERPRET_F64, v_f64_i64);op!(F32_REINTERPRET_I32, v_i32_f32);
    op!(F64_REINTERPRET_I64, v_i64_f64);
    t
}

fn get_validators() -> &'static [ValidatorFn; 256] {
    static VALIDATORS: std::sync::LazyLock<Box<[ValidatorFn; 256]>> =
        std::sync::LazyLock::new(|| Box::new(build_validators_table()));
    &VALIDATORS
}
