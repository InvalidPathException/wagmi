use crate::leb128;
use crate::specs::{opcodes::Opcode, WasmValue};
use crate::{memory_load, memory_store, binary_fn, unary_fn, trunc};

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct ControlFrame {
    arity: u32,
    continuation: usize,
    is_loop: bool,
}
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct StackFrame {
    locals: Vec<WasmValue>,
    control_stack: Vec<ControlFrame>,
    return_address: usize,
}

#[allow(dead_code)]
fn execute_opcode(
    opcode: Opcode,
    stack: &mut Vec<WasmValue>,
    ctrl_stack: &mut Vec<ControlFrame>,
    memory: &mut Vec<u8>,
    iter: &mut &[u8],
) {
    match opcode {
        Opcode::UNREACHABLE => {
            panic!("Unreachable executed");
        }
        Opcode::NOP => {
            // DO NOTHING
        }
        Opcode::BLOCK => {
            // Code for BLOCK
        }
        Opcode::LOOP => {
            // Code for LOOP
        }
        Opcode::IF => {
            // Code for IF
        }
        Opcode::ELSE => {
            // Code for ELSE
        }
        Opcode::END => {
            // Code for END
        }
        Opcode::BR => {    
            let depth = leb128::read_leb128_u(iter).expect("Failed to readLEB128 value");
            let target_frame = &ctrl_stack[ctrl_stack.len() - depth as usize - 1];
            *iter = &iter[target_frame.continuation..];
            ctrl_stack.truncate(ctrl_stack.len() - depth as usize);
        }
        Opcode::BR_IF => {
            let depth = leb128::read_leb128_s(iter).expect("Failed to read LEB128 value");
            let cond = stack.pop().expect("Stack underflow from select")
                .as_i32().expect("Condition must be of type i32");

            if cond != 0 {
                let target_frame = &ctrl_stack[ctrl_stack.len() - depth as usize - 1];
                *iter = &iter[target_frame.continuation..];
                ctrl_stack.truncate(ctrl_stack.len() - depth as usize);
            }
        }
        Opcode::BR_TABLE => {
            // Code for BR_TABLE
        }
        Opcode::RETURN => {
            // Code for RETURN
        }
        Opcode::CALL => {
            // Code for CALL
        }
        Opcode::CALL_INDIRECT => {
            // Code for CALL_INDIRECT
        }
        Opcode::DROP => {
            stack.pop().expect("Stack underflow from drop");
        }
        Opcode::SELECT => {
            let cond = stack.pop().expect("Stack underflow from select")
                .as_i32().expect("Condition must be of type i32");
            let val2 = stack.pop().expect("Stack underflow from select");
            let val1 = stack.pop().expect("Stack underflow from select");

            match (&val1, &val2) {
                (WasmValue::I32(_), WasmValue::I32(_))
                | (WasmValue::I64(_), WasmValue::I64(_))
                | (WasmValue::F32(_), WasmValue::F32(_))
                | (WasmValue::F64(_), WasmValue::F64(_)) => {
                    stack.push(if cond != 0 { val1 } else { val2 });
                }
                _ => {
                    panic!("Type mismatch in select");
                }
            }
        }
        Opcode::LOCAL_GET => {
            // Code for LOCAL_GET
        }
        Opcode::LOCAL_SET => {
            // Code for LOCAL_SET
        }
        Opcode::LOCAL_TEE => {
            // Code for LOCAL_TEE
        }
        Opcode::GLOBAL_GET => {
            // Code for GLOBAL_GET
        }
        Opcode::GLOBAL_SET => {
            // Code for GLOBAL_SET
        }
        Opcode::I32_LOAD => {
            memory_load!(stack, memory, I32, leb128::i32_load, leb128::read_offset(iter));
        }
        Opcode::I64_LOAD => {
            memory_load!(stack, memory, I64, leb128::i64_load, leb128::read_offset(iter));
        }
        Opcode::F32_LOAD => {
            memory_load!(stack, memory, F32, leb128::f32_load, leb128::read_offset(iter));
        }
        Opcode::F64_LOAD => {
            memory_load!(stack, memory, F64, leb128::f64_load, leb128::read_offset(iter));
        }
        Opcode::I32_LOAD8_S => {
            memory_load!(stack, memory, I32, leb128::i32_load8_s, leb128::read_offset(iter));
        }
        Opcode::I32_LOAD8_U => {
            memory_load!(stack, memory, I32, leb128::i32_load8_u, leb128::read_offset(iter));
        }
        Opcode::I32_LOAD16_S => {
            memory_load!(stack, memory, I32, leb128::i32_load16_s, leb128::read_offset(iter));
        }
        Opcode::I32_LOAD16_U => {
            memory_load!(stack, memory, I32, leb128::i32_load16_u, leb128::read_offset(iter));
        }        
        Opcode::I64_LOAD8_S => {
            memory_load!(stack, memory, I64, leb128::i64_load8_s, leb128::read_offset(iter));
        }
        Opcode::I64_LOAD8_U => {
            memory_load!(stack, memory, I64, leb128::i64_load8_u, leb128::read_offset(iter));
        }
        Opcode::I64_LOAD16_S => {
            memory_load!(stack, memory, I64, leb128::i64_load16_s, leb128::read_offset(iter));
        }
        Opcode::I64_LOAD16_U => {
            memory_load!(stack, memory, I64, leb128::i64_load16_u, leb128::read_offset(iter));
        }
        Opcode::I64_LOAD32_S => {
            memory_load!(stack, memory, I64, leb128::i64_load32_s, leb128::read_offset(iter));
        }
        Opcode::I64_LOAD32_U => {
            memory_load!(stack, memory, I64, leb128::i64_load32_u, leb128::read_offset(iter));
        }
        Opcode::I32_STORE => {
            memory_store!(stack, &mut memory[..], I32, leb128::i32_store, leb128::read_offset(iter));
        }
        Opcode::I64_STORE => {
            memory_store!(stack, &mut memory[..], I64, leb128::i64_store, leb128::read_offset(iter));
        }
        Opcode::F32_STORE => {
            memory_store!(stack, &mut memory[..], F32, leb128::f32_store, leb128::read_offset(iter));
        }
        Opcode::F64_STORE => {
            memory_store!(stack, &mut memory[..], F64, leb128::f64_store, leb128::read_offset(iter));
        }
        Opcode::I32_STORE8 => {
            memory_store!(stack, &mut memory[..], I32, leb128::i32_store8, leb128::read_offset(iter));
        }
        Opcode::I32_STORE16 => {
            memory_store!(stack, &mut memory[..], I32, leb128::i32_store16, leb128::read_offset(iter));
        }
        Opcode::I64_STORE8 => {
            memory_store!(stack, &mut memory[..], I64, leb128::i64_store8, leb128::read_offset(iter));
        }
        Opcode::I64_STORE16 => {
            memory_store!(stack, &mut memory[..], I64, leb128::i64_store16, leb128::read_offset(iter));
        }
        Opcode::I64_STORE32 => {
            memory_store!(stack, &mut memory[..], I64, leb128::i64_store32, leb128::read_offset(iter));
        }
        Opcode::MEMORY_SIZE => {
            stack.push(WasmValue::I32(memory.len() as i32));
        }
        Opcode::MEMORY_GROW => {
            let n_pages = stack.pop().expect("Stack underflow from grow")
                .as_i32().expect("Expected i32 operand") as usize;
            let new_size = memory.len() / 65535 + n_pages;
            
            if n_pages > 0 && new_size <= 1024 {
                memory.resize(new_size * 65535, 0);
                stack.push(WasmValue::I32((new_size - n_pages) as i32));
            } else {
                stack.push(WasmValue::I32(-1));
            }
        }
        Opcode::I32_CONST => {
            stack.push(WasmValue::I32(leb128::read_leb128_s(iter).expect("Failed to read i32.const value") as i32));
        }
        Opcode::I64_CONST => {
            stack.push(WasmValue::I64(leb128::read_leb128_s(iter).expect("Failed to read i64.const value")));
        }
        Opcode::F32_CONST => {
            let (buffer, remaining) = iter.split_at(4);
            *iter = remaining;
            stack.push(WasmValue::F32(f32::from_le_bytes(buffer.try_into().expect("Invalid F32 bytes"))));
        }
        Opcode::F64_CONST => {
            let (buffer, remaining) = iter.split_at(8);
            *iter = remaining;
            stack.push(WasmValue::F64(f64::from_le_bytes(buffer.try_into().expect("Invalid F64 bytes"))));
        }
        Opcode::I32_EQZ => {
            unary_fn!(stack, i32, i32, |a: i32| if a == 0 { 1 } else { 0 });
        }
        Opcode::I32_EQ => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| if a == b { 1 } else { 0 });
        }
        Opcode::I32_NE => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| if a != b { 1 } else { 0 });
        }
        Opcode::I32_LT_S => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| if a < b { 1 } else { 0 });
        }
        Opcode::I32_LT_U => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| if (a as u32) < (b as u32) { 1 } else { 0 });
        }
        Opcode::I32_GT_S => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| if a > b { 1 } else { 0 });
        }
        Opcode::I32_GT_U => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| if (a as u32) > (b as u32) { 1 } else { 0 });
        }
        Opcode::I32_LE_S => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| if a <= b { 1 } else { 0 });
        }
        Opcode::I32_LE_U => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| if (a as u32) <= (b as u32) { 1 } else { 0 });
        }
        Opcode::I32_GE_S => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| if a >= b { 1 } else { 0 });
        }
        Opcode::I32_GE_U => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| if (a as u32) >= (b as u32) { 1 } else { 0 });
        }
        Opcode::I64_EQZ => {
            unary_fn!(stack, i64, i32, |a: i64| if a == 0 { 1 } else { 0 });
        }
        Opcode::I64_EQ => {
            binary_fn!(stack, i64, i32, |a: i64, b: i64| if a == b { 1 } else { 0 });
        }
        Opcode::I64_NE => {
            binary_fn!(stack, i64, i32, |a: i64, b: i64| if a != b { 1 } else { 0 });
        }
        Opcode::I64_LT_S => {
            binary_fn!(stack, i64, i32, |a: i64, b: i64| if a < b { 1 } else { 0 });
        }
        Opcode::I64_LT_U => {
            binary_fn!(stack, i64, i32, |a: i64, b: i64| if (a as u64) < (b as u64) { 1 } else { 0 });
        }
        Opcode::I64_GT_S => {
            binary_fn!(stack, i64, i32, |a: i64, b: i64| if a > b { 1 } else { 0 });
        }
        Opcode::I64_GT_U => {
            binary_fn!(stack, i64, i32, |a: i64, b: i64| if (a as u64) > (b as u64) { 1 } else { 0 });
        }
        Opcode::I64_LE_S => {
            binary_fn!(stack, i64, i32, |a: i64, b: i64| if a <= b { 1 } else { 0 });
        }
        Opcode::I64_LE_U => {
            binary_fn!(stack, i64, i32, |a: i64, b: i64| if (a as u64) <= (b as u64) { 1 } else { 0 });
        }
        Opcode::I64_GE_S => {
            binary_fn!(stack, i64, i32, |a: i64, b: i64| if a >= b { 1 } else { 0 });
        }
        Opcode::I64_GE_U => {
            binary_fn!(stack, i64, i32, |a: i64, b: i64| if (a as u64) >= (b as u64) { 1 } else { 0 });
        }
        Opcode::F32_EQ => {
            binary_fn!(stack, f32, i32, |a: f32, b: f32| if a == b { 1 } else { 0 });
        }
        Opcode::F32_NE => {
            binary_fn!(stack, f32, i32, |a: f32, b: f32| if a != b { 1 } else { 0 });
        }
        Opcode::F32_LT => {
            binary_fn!(stack, f32, i32, |a: f32, b: f32| if a < b { 1 } else { 0 });
        }
        Opcode::F32_GT => {
            binary_fn!(stack, f32, i32, |a: f32, b: f32| if a > b { 1 } else { 0 });
        }
        Opcode::F32_LE => {
            binary_fn!(stack, f32, i32, |a: f32, b: f32| if a <= b { 1 } else { 0 });
        }
        Opcode::F32_GE => {
            binary_fn!(stack, f32, i32, |a: f32, b: f32| if a >= b { 1 } else { 0 });
        }
        Opcode::F64_EQ => {
            binary_fn!(stack, f64, i32, |a: f64, b: f64| if a == b { 1 } else { 0 });
        }
        Opcode::F64_NE => {
            binary_fn!(stack, f64, i32, |a: f64, b: f64| if a != b { 1 } else { 0 });
        }
        Opcode::F64_LT => {
            binary_fn!(stack, f64, i32, |a: f64, b: f64| if a < b { 1 } else { 0 });
        }
        Opcode::F64_GT => {
            binary_fn!(stack, f64, i32, |a: f64, b: f64| if a > b { 1 } else { 0 });
        }
        Opcode::F64_LE => {
            binary_fn!(stack, f64, i32, |a: f64, b: f64| if a <= b { 1 } else { 0 });
        }
        Opcode::F64_GE => {
            binary_fn!(stack, f64, i32, |a: f64, b: f64| if a >= b { 1 } else { 0 });
        }
        Opcode::I32_CLZ => {
            unary_fn!(stack, i32, i32, |a: i32| a.leading_zeros() as i32);
        }
        Opcode::I32_CTZ => {
            unary_fn!(stack, i32, i32, |a: i32| a.trailing_zeros() as i32);
        }
        Opcode::I32_POPCNT => {
            unary_fn!(stack, i32, i32, |a: i32| a.count_ones() as i32);
        }
        Opcode::I32_ADD => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| a.wrapping_add(b));
        }
        Opcode::I32_SUB => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| a.wrapping_sub(b));
        }
        Opcode::I32_MUL => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| a.wrapping_mul(b));
        }
        Opcode::I32_DIV_S => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| { a.checked_div(b).expect("Integer overflow or division by zero") });
        }
        Opcode::I32_DIV_U => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| { (a as u32).checked_div(b as u32).expect("Division by zero") as i32 });
        }
        Opcode::I32_REM_S => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| a.wrapping_rem(b));
        }
        Opcode::I32_REM_U => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| { (a as u32).wrapping_rem(b as u32) as i32 });
        }
        Opcode::I32_AND => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| a & b);
        }
        Opcode::I32_OR => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| a | b);
        }
        Opcode::I32_XOR => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| a ^ b);
        }
        Opcode::I32_SHL => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| a.wrapping_shl(b as u32));
        }
        Opcode::I32_SHR_S => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| a >> (b as u32));
        }
        Opcode::I32_SHR_U => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| a.wrapping_shr(b as u32));
        }
        Opcode::I32_ROTL => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| a.rotate_left(b as u32));
        }
        Opcode::I32_ROTR => {
            binary_fn!(stack, i32, i32, |a: i32, b: i32| a.rotate_right(b as u32));
        }
        Opcode::I64_CLZ => {
            unary_fn!(stack, i64, i64, |a: i64| a.leading_zeros() as i64);
        }
        Opcode::I64_CTZ => {
            unary_fn!(stack, i64, i64, |a: i64| a.trailing_zeros() as i64);
        }
        Opcode::I64_POPCNT => {
            unary_fn!(stack, i64, i64, |a: i64| a.count_ones() as i64);
        }
        Opcode::I64_ADD => {
            binary_fn!(stack, i64, i64, |a: i64, b: i64| a.wrapping_add(b));
        }
        Opcode::I64_SUB => {
            binary_fn!(stack, i64, i64, |a: i64, b: i64| a.wrapping_sub(b));
        }
        Opcode::I64_MUL => {
            binary_fn!(stack, i64, i64, |a: i64, b: i64| a.wrapping_mul(b));
        }
        Opcode::I64_DIV_S => {
            binary_fn!(stack, i64, i64, |a: i64, b: i64| { a.checked_div(b).expect("Integer overflow or division by zero") });
        }
        Opcode::I64_DIV_U => {
            binary_fn!(stack, i64, i64, |a: i64, b: i64| { (a as u64).checked_div(b as u64).expect("Division by zero") as i64 });
        }
        Opcode::I64_REM_S => {
            binary_fn!(stack, i64, i64, |a: i64, b: i64| a.wrapping_rem(b));
        }
        Opcode::I64_REM_U => {
            binary_fn!(stack, i64, i64, |a: i64, b: i64| { (a as u64).wrapping_rem(b as u64) as i64 });
        }
        Opcode::I64_AND => {
            binary_fn!(stack, i64, i64, |a: i64, b: i64| a & b);
        }
        Opcode::I64_OR => {
            binary_fn!(stack, i64, i64, |a: i64, b: i64| a | b);
        }
        Opcode::I64_XOR => {
            binary_fn!(stack, i64, i64, |a: i64, b: i64| a ^ b);
        }
        Opcode::I64_SHL => {
            binary_fn!(stack, i64, i64, |a: i64, b: i64| a.wrapping_shl(b as u32));
        }
        Opcode::I64_SHR_S => {
            binary_fn!(stack, i64, i64, |a: i64, b: i64| a >> (b as u32));
        }
        Opcode::I64_SHR_U => {
            binary_fn!(stack, i64, i64, |a: i64, b: i64| a.wrapping_shr(b as u32));
        }
        Opcode::I64_ROTL => {
            binary_fn!(stack, i64, i64, |a: i64, b: i64| a.rotate_left(b as u32));
        }
        Opcode::I64_ROTR => {
            binary_fn!(stack, i64, i64, |a: i64, b: i64| a.rotate_right(b as u32));
        }
        Opcode::F32_ABS => {
            unary_fn!(stack, f32, f32, |a: f32| a.abs());
        }
        Opcode::F32_NEG => {
            unary_fn!(stack, f32, f32, |a: f32| -a);
        }
        Opcode::F32_CEIL => {
            unary_fn!(stack, f32, f32, |a: f32| a.ceil());
        }
        Opcode::F32_FLOOR => {
            unary_fn!(stack, f32, f32, |a: f32| a.floor());
        }
        Opcode::F32_TRUNC => {
            trunc!(stack, f32, i32, i32::MIN as f32, i32::MAX as f32, |a: f32| a.trunc() as i32);
        }
        Opcode::F32_NEAREST => {
            unary_fn!(stack, f32, f32, |a: f32| a.round());
        }
        Opcode::F32_SQRT => {
            unary_fn!(stack, f32, f32, |a: f32| a.sqrt());
        }
        Opcode::F32_ADD => {
            binary_fn!(stack, f32, f32, |a: f32, b: f32| a + b);
        }
        Opcode::F32_SUB => {
            binary_fn!(stack, f32, f32, |a: f32, b: f32| a - b);
        }
        Opcode::F32_MUL => {
            binary_fn!(stack, f32, f32, |a: f32, b: f32| a * b);
        }
        Opcode::F32_DIV => {
            binary_fn!(stack, f32, f32, |a: f32, b: f32| a / b);
        }
        Opcode::F32_MIN => {
            binary_fn!(stack, f32, f32, |a: f32, b: f32| a.min(b));
        }
        Opcode::F32_MAX => {
            binary_fn!(stack, f32, f32, |a: f32, b: f32| a.max(b));
        }
        Opcode::F32_COPYSIGN => {
            binary_fn!(stack, f32, f32, |a: f32, b: f32| a.copysign(b));
        }
        Opcode::F64_ABS => {
            unary_fn!(stack, f64, f64, |a: f64| a.abs());
        }
        Opcode::F64_NEG => {
            unary_fn!(stack, f64, f64, |a: f64| -a);
        }
        Opcode::F64_CEIL => {
            unary_fn!(stack, f64, f64, |a: f64| a.ceil());
        }
        Opcode::F64_FLOOR => {
            unary_fn!(stack, f64, f64, |a: f64| a.floor());
        }
        Opcode::F64_TRUNC => {
            trunc!(stack, f64, i64, i64::MIN as f64, i64::MAX as f64, |a: f64| a.trunc() as i64);
        }
        Opcode::F64_NEAREST => {
            unary_fn!(stack, f64, f64, |a: f64| a.round());
        }
        Opcode::F64_SQRT => {
            unary_fn!(stack, f64, f64, |a: f64| a.sqrt());
        }
        Opcode::F64_ADD => {
            binary_fn!(stack, f64, f64, |a: f64, b: f64| a + b);
        }
        Opcode::F64_SUB => {
            binary_fn!(stack, f64, f64, |a: f64, b: f64| a - b);
        }
        Opcode::F64_MUL => {
            binary_fn!(stack, f64, f64, |a: f64, b: f64| a * b);
        }
        Opcode::F64_DIV => {
            binary_fn!(stack, f64, f64, |a: f64, b: f64| a / b);
        }
        Opcode::F64_MIN => {
            binary_fn!(stack, f64, f64, |a: f64, b: f64| a.min(b));
        }
        Opcode::F64_MAX => {
            binary_fn!(stack, f64, f64, |a: f64, b: f64| a.max(b));
        }
        Opcode::F64_COPYSIGN => {
            binary_fn!(stack, f64, f64, |a: f64, b: f64| a.copysign(b));
        }
        Opcode::I32_WRAP_I64 => {
            unary_fn!(stack, i64, i32, |a: i64| a as i32);
        }
        Opcode::I32_TRUNC_F32_S => {
            trunc!(stack, f32, i32, i32::MIN as f32, i32::MAX as f32, |a: f32| a.trunc() as i32);
        }
        Opcode::I32_TRUNC_F32_U => {
            trunc!(stack, f32, i32, 0.0f32, u32::MAX as f32, (|a: f32| (a.trunc() as u32) as i32));
        }
        Opcode::I32_TRUNC_F64_S => {
            trunc!(stack, f64, i32, i32::MIN as f64, i32::MAX as f64, |a: f64| a.trunc() as i32);
        }
        Opcode::I32_TRUNC_F64_U => {
            trunc!(stack, f64, i32, 0.0f64, u32::MAX as f64, (|a: f64| (a.trunc() as u32) as i32));
        }
        Opcode::I64_EXTEND_I32_S => {
            unary_fn!(stack, i32, i64, |a: i32| a as i64);
        }
        Opcode::I64_EXTEND_I32_U => {
            unary_fn!(stack, i32, i64, |a: i32| (a as u32) as i64);
        }
        Opcode::I64_TRUNC_F32_S => {
            trunc!(stack, f32, i64, i64::MIN as f32, i64::MAX as f32, |a: f32| a.trunc() as i64);
        }
        Opcode::I64_TRUNC_F32_U => {
            trunc!(stack, f32, i64, 0.0f32, u64::MAX as f32, (|a: f32| (a.trunc() as u64) as i64));
        }
        Opcode::I64_TRUNC_F64_S => {
            trunc!(stack, f64, i64, i64::MIN as f64, i64::MAX as f64, |a: f64| a.trunc() as i64);
        }
        Opcode::I64_TRUNC_F64_U => {
            trunc!(stack, f64, i64, 0.0f64, u64::MAX as f64, (|a: f64| (a.trunc() as u64) as i64));
        }
        Opcode::F32_CONVERT_I32_S => {
            unary_fn!(stack, i32, f32, |a: i32| a as f32);
        }
        Opcode::F32_CONVERT_I32_U => {
            unary_fn!(stack, i32, f32, |a: i32| (a as u32) as f32);
        }
        Opcode::F32_CONVERT_I64_S => {
            unary_fn!(stack, i64, f32, |a: i64| a as f32);
        }
        Opcode::F32_CONVERT_I64_U => {
            unary_fn!(stack, i64, f32, |a: i64| (a as u64) as f32);
        }
        Opcode::F32_DEMOTE_F64 => {
            unary_fn!(stack, f64, f32, |a: f64| a as f32);
        }
        Opcode::F64_CONVERT_I32_S => {
            unary_fn!(stack, i32, f64, |a: i32| a as f64);
        }
        Opcode::F64_CONVERT_I32_U => {
            unary_fn!(stack, i32, f64, |a: i32| (a as u32) as f64);
        }
        Opcode::F64_CONVERT_I64_S => {
            unary_fn!(stack, i64, f64, |a: i64| a as f64);
        }
        Opcode::F64_CONVERT_I64_U => {
            unary_fn!(stack, i64, f64, |a: i64| (a as u64) as f64);
        }
        Opcode::F64_PROMOTE_F32 => {
            unary_fn!(stack, f32, f64, |a: f32| a as f64);
        }
        Opcode::I32_REINTERPRET_F32 => {
            unary_fn!(stack, f32, i32, |a: f32| a.to_bits() as i32);
        }
        Opcode::I64_REINTERPRET_F64 => {
            unary_fn!(stack, f64, i64, |a: f64| a.to_bits() as i64);
        }
        Opcode::F32_REINTERPRET_I32 => {
            unary_fn!(stack, i32, f32, |a: i32| f32::from_bits(a as u32));
        }
        Opcode::F64_REINTERPRET_I64 => {
            unary_fn!(stack, i64, f64, |a: i64| f64::from_bits(a as u64));
        }
    }
}
