use crate::leb128::leb128;
use crate::specs::{op_impl, opcodes::Opcode, WasmValue};
use crate::{binary_fn, div_f, div_s, div_u, memory_load, memory_store, rem_s, rem_u, unary_fn};
use std::io::Read;

fn execute_opcode(
    opcode: Opcode,
    stack: &mut Vec<WasmValue>,
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
            // Code for BR
        }
        Opcode::BR_IF => {
            // Code for BR_IF
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
            if stack.pop().is_none() {
                panic!("Stack underflow from drop");
            }
        }
        Opcode::SELECT => {
            let cond = stack.pop().expect("Stack underflow from select");
            let val2 = stack.pop().expect("Stack underflow from select");
            let val1 = stack.pop().expect("Stack underflow from select");
            let cond_as_i32 = cond.to_i32().expect("Condition must be of type i32");

            if matches!(
                (&val1, &val2),
                (WasmValue::I32(_), WasmValue::I32(_))
                    | (WasmValue::I64(_), WasmValue::I64(_))
                    | (WasmValue::F32(_), WasmValue::F32(_))
                    | (WasmValue::F64(_), WasmValue::F64(_))
            ) == false
            {
                panic!("Type mismatch in select");
            }

            stack.push(if cond_as_i32 != 0 { val1 } else { val2 });
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
            let (align, offset) = leb128::read_memarg(iter);
            {memory_load!(stack, memory, I32, leb128::i32_load, offset);}
        }
        Opcode::I64_LOAD => {
            let (align, offset) = leb128::read_memarg(iter);
            {memory_load!(stack, memory, I64, leb128::i64_load, offset);}
        }
        Opcode::F32_LOAD => {
            let (align, offset) = leb128::read_memarg(iter);
            {memory_load!(stack, memory, F32, leb128::f32_load, offset);}
        }
        Opcode::F64_LOAD => {
            let (align, offset) = leb128::read_memarg(iter);
            {memory_load!(stack, memory, F64, leb128::f64_load, offset);}
        }
        Opcode::I32_LOAD8_S => {
            let (align, offset) = leb128::read_memarg(iter);
            {memory_load!(stack, memory, I32, leb128::i32_load8_s, offset);}
        }
        Opcode::I32_LOAD8_U => {
            let (align, offset) = leb128::read_memarg(iter);
            {memory_load!(stack, memory, I32, leb128::i32_load8_u, offset);}
        }
        Opcode::I32_LOAD16_S => {
            let (align, offset) = leb128::read_memarg(iter);
            {memory_load!(stack, memory, I32, leb128::i32_load16_s, offset);}
        }
        Opcode::I32_LOAD16_U => {
            let (align, offset) = leb128::read_memarg(iter);
            {memory_load!(stack, memory, I32, leb128::i32_load16_u, offset);}
        }        
        Opcode::I64_LOAD8_S => {
            let (align, offset) = leb128::read_memarg(iter);
            {memory_load!(stack, memory, I64, leb128::i64_load8_s, offset);}
        }
        Opcode::I64_LOAD8_U => {
            let (align, offset) = leb128::read_memarg(iter);
            {memory_load!(stack, memory, I64, leb128::i64_load8_u, offset);}
        }
        Opcode::I64_LOAD16_S => {
            let (align, offset) = leb128::read_memarg(iter);
            {memory_load!(stack, memory, I64, leb128::i64_load16_s, offset);}
        }
        Opcode::I64_LOAD16_U => {
            let (align, offset) = leb128::read_memarg(iter);
            {memory_load!(stack, memory, I64, leb128::i64_load16_u, offset);}
        }
        Opcode::I64_LOAD32_S => {
            let (align, offset) = leb128::read_memarg(iter);
            {memory_load!(stack, memory, I64, leb128::i64_load32_s, offset);}
        }
        Opcode::I64_LOAD32_U => {
            let (align, offset) = leb128::read_memarg(iter);
            {memory_load!(stack, memory, I64, leb128::i64_load32_u, offset);}
        }
        Opcode::I32_STORE => {
            let (align, offset) = leb128::read_memarg(iter);
            {memory_store!(stack, &mut memory[..], I32, leb128::i32_store, offset);}
        }
        Opcode::I64_STORE => {
            let (align, offset) = leb128::read_memarg(iter);
            {memory_store!(stack, &mut memory[..], I64, leb128::i64_store, offset);}
        }
        Opcode::F32_STORE => {
            let (align, offset) = leb128::read_memarg(iter);
            {memory_store!(stack, &mut memory[..], F32, leb128::f32_store, offset);}
        }
        Opcode::F64_STORE => {
            let (align, offset) = leb128::read_memarg(iter);
            {memory_store!(stack, &mut memory[..], F64, leb128::f64_store, offset);}
        }
        Opcode::I32_STORE8 => {
            let (align, offset) = leb128::read_memarg(iter);
            {memory_store!(stack, &mut memory[..], I32, leb128::i32_store8, offset);}
        }
        Opcode::I32_STORE16 => {
            let (align, offset) = leb128::read_memarg(iter);
            {memory_store!(stack, &mut memory[..], I32, leb128::i32_store16, offset);}
        }
        Opcode::I64_STORE8 => {
            let (align, offset) = leb128::read_memarg(iter);
            {memory_store!(stack, &mut memory[..], I64, leb128::i64_store8, offset);}
        }
        Opcode::I64_STORE16 => {
            let (align, offset) = leb128::read_memarg(iter);
            {memory_store!(stack, &mut memory[..], I64, leb128::i64_store16, offset);}
        }
        Opcode::I64_STORE32 => {
            let (align, offset) = leb128::read_memarg(iter);
            {memory_store!(stack, &mut memory[..], I64, leb128::i64_store32, offset);}
        }
        Opcode::MEMORY_SIZE => {
            stack.push(WasmValue::I32(memory.len() as i32));
        }
        Opcode::MEMORY_GROW => {
            let n_pages = stack
                .pop()
                .expect("Stack underflow from grow")
                .to_i32()
                .expect("Expected i32 operand") as usize;

            let new_size = memory.len() / 65535 + n_pages;
            if n_pages > 0 && new_size <= 1024 {
                memory.resize(new_size * 65535, 0);
                stack.push(WasmValue::I32((new_size - n_pages) as i32));
            } else {
                stack.push(WasmValue::I32(-1));
            }
        }
        Opcode::I32_CONST => {
            stack.push(WasmValue::I32(
                leb128::read_leb128_s(iter).expect("Failed to read i32.const value") as i32,
            ));
        }
        Opcode::I64_CONST => {
            stack.push(WasmValue::I64(
                leb128::read_leb128_s(iter).expect("Failed to read i64.const value"),
            ));
        }
        Opcode::F32_CONST => {
            let mut buffer = [0u8; 4];
            iter.read_exact(&mut buffer)
                .expect("Failed to read f32.const value");
            stack.push(WasmValue::F32(f32::from_le_bytes(buffer)));
        }
        Opcode::F64_CONST => {
            let mut buffer = [0u8; 8];
            iter.read_exact(&mut buffer)
                .expect("Failed to read f64.const value");
            stack.push(WasmValue::F64(f64::from_le_bytes(buffer)));
        }
        Opcode::I32_EQZ => {
            unary_fn!(stack, i32, i32, |x: i32| if x == 0 { 1 } else { 0 });
        }
        Opcode::I32_EQ => {
            binary_fn!(stack, i32, i32, op_impl::i32_eq);
        }
        Opcode::I32_NE => {
            binary_fn!(stack, i32, i32, op_impl::i32_ne);
        }
        Opcode::I32_LT_S => {
            binary_fn!(stack, i32, i32, op_impl::i32_lt_s);
        }
        Opcode::I32_LT_U => {
            binary_fn!(stack, i32, i32, op_impl::i32_lt_u);
        }
        Opcode::I32_GT_S => {
            binary_fn!(stack, i32, i32, op_impl::i32_gt_s);
        }
        Opcode::I32_GT_U => {
            binary_fn!(stack, i32, i32, op_impl::i32_gt_u);
        }
        Opcode::I32_LE_S => {
            binary_fn!(stack, i32, i32, op_impl::i32_le_s);
        }
        Opcode::I32_LE_U => {
            binary_fn!(stack, i32, i32, op_impl::i32_le_u);
        }
        Opcode::I32_GE_S => {
            binary_fn!(stack, i32, i32, op_impl::i32_ge_s);
        }
        Opcode::I32_GE_U => {
            binary_fn!(stack, i32, i32, op_impl::i32_ge_u);
        }
        Opcode::I64_EQZ => {
            unary_fn!(stack, i64, i32, |x: i64| if x == 0 { 1 } else { 0 });
        }
        Opcode::I64_EQ => {
            binary_fn!(stack, i64, i32, op_impl::i64_eq);
        }
        Opcode::I64_NE => {
            binary_fn!(stack, i64, i32, op_impl::i64_ne);
        }
        Opcode::I64_LT_S => {
            binary_fn!(stack, i64, i32, op_impl::i64_lt_s);
        }
        Opcode::I64_LT_U => {
            binary_fn!(stack, i64, i32, op_impl::i64_lt_u);
        }
        Opcode::I64_GT_S => {
            binary_fn!(stack, i64, i32, op_impl::i64_gt_s);
        }
        Opcode::I64_GT_U => {
            binary_fn!(stack, i64, i32, op_impl::i64_gt_u);
        }
        Opcode::I64_LE_S => {
            binary_fn!(stack, i64, i32, op_impl::i64_le_s);
        }
        Opcode::I64_LE_U => {
            binary_fn!(stack, i64, i32, op_impl::i64_le_u);
        }
        Opcode::I64_GE_S => {
            binary_fn!(stack, i64, i32, op_impl::i64_ge_s);
        }
        Opcode::I64_GE_U => {
            binary_fn!(stack, i64, i32, op_impl::i64_ge_u);
        }
        Opcode::F32_EQ => {
            binary_fn!(stack, f32, i32, op_impl::f32_eq);
        }
        Opcode::F32_NE => {
            binary_fn!(stack, f32, i32, op_impl::f32_ne);
        }
        Opcode::F32_LT => {
            binary_fn!(stack, f32, i32, op_impl::f32_lt);
        }
        Opcode::F32_GT => {
            binary_fn!(stack, f32, i32, op_impl::f32_gt);
        }
        Opcode::F32_LE => {
            binary_fn!(stack, f32, i32, op_impl::f32_le);
        }
        Opcode::F32_GE => {
            binary_fn!(stack, f32, i32, op_impl::f32_ge);
        }
        Opcode::F64_EQ => {
            binary_fn!(stack, f64, i32, op_impl::f64_eq);
        }
        Opcode::F64_NE => {
            binary_fn!(stack, f64, i32, op_impl::f64_ne);
        }
        Opcode::F64_LT => {
            binary_fn!(stack, f64, i32, op_impl::f64_lt);
        }
        Opcode::F64_GT => {
            binary_fn!(stack, f64, i32, op_impl::f64_gt);
        }
        Opcode::F64_LE => {
            binary_fn!(stack, f64, i32, op_impl::f64_le);
        }
        Opcode::F64_GE => {
            binary_fn!(stack, f64, i32, op_impl::f64_ge);
        }
        Opcode::I32_CLZ => {
            unary_fn!(stack, i32, i32, |x: i32| x.leading_zeros() as i32);
        }
        Opcode::I32_CTZ => {
            unary_fn!(stack, i32, i32, |x: i32| x.trailing_zeros() as i32);
        }
        Opcode::I32_POPCNT => {
            unary_fn!(stack, i32, i32, |x: i32| x.count_ones() as i32);
        }
        Opcode::I32_ADD => {
            binary_fn!(stack, i32, i32, op_impl::i32_add);
        }
        Opcode::I32_SUB => {
            binary_fn!(stack, i32, i32, op_impl::i32_sub);
        }
        Opcode::I32_MUL => {
            binary_fn!(stack, i32, i32, op_impl::i32_mul);
        }
        Opcode::I32_DIV_S => {
            div_s!(stack, i32);
        }
        Opcode::I32_DIV_U => {
            div_u!(stack, i32);
        }
        Opcode::I32_REM_S => {
            rem_s!(stack, i32);
        }
        Opcode::I32_REM_U => {
            rem_u!(stack, i32);
        }
        Opcode::I32_AND => {
            binary_fn!(stack, i32, i32, op_impl::i32_and);
        }
        Opcode::I32_OR => {
            binary_fn!(stack, i32, i32, op_impl::i32_or);
        }
        Opcode::I32_XOR => {
            binary_fn!(stack, i32, i32, op_impl::i32_xor);
        }
        Opcode::I32_SHL => {
            binary_fn!(stack, i32, i32, op_impl::i32_shl);
        }
        Opcode::I32_SHR_S => {
            binary_fn!(stack, i32, i32, op_impl::i32_shr_s);
        }
        Opcode::I32_SHR_U => {
            binary_fn!(stack, i32, i32, op_impl::i32_shr_u);
        }
        Opcode::I32_ROTL => {
            binary_fn!(stack, i32, i32, op_impl::i32_rotl);
        }
        Opcode::I32_ROTR => {
            binary_fn!(stack, i32, i32, op_impl::i32_rotr);
        }
        Opcode::I64_CLZ => {
            unary_fn!(stack, i64, i64, |x: i64| x.leading_zeros() as i64);
        }
        Opcode::I64_CTZ => {
            unary_fn!(stack, i64, i64, |x: i64| x.trailing_zeros() as i64);
        }
        Opcode::I64_POPCNT => {
            unary_fn!(stack, i64, i64, |x: i64| x.count_ones() as i64);
        }
        Opcode::I64_ADD => {
            binary_fn!(stack, i64, i64, op_impl::i64_add);
        }
        Opcode::I64_SUB => {
            binary_fn!(stack, i64, i64, op_impl::i64_sub);
        }
        Opcode::I64_MUL => {
            binary_fn!(stack, i64, i64, op_impl::i64_mul);
        }
        Opcode::I64_DIV_S => {
            div_s!(stack, i64);
        }
        Opcode::I64_DIV_U => {
            div_u!(stack, i64);
        }
        Opcode::I64_REM_S => {
            rem_s!(stack, i64);
        }
        Opcode::I64_REM_U => {
            rem_u!(stack, i64);
        }
        Opcode::I64_AND => {
            binary_fn!(stack, i64, i64, op_impl::i64_and);
        }
        Opcode::I64_OR => {
            binary_fn!(stack, i64, i64, op_impl::i64_or);
        }
        Opcode::I64_XOR => {
            binary_fn!(stack, i64, i64, op_impl::i64_xor);
        }
        Opcode::I64_SHL => {
            binary_fn!(stack, i64, i64, op_impl::i64_shl);
        }
        Opcode::I64_SHR_S => {
            binary_fn!(stack, i64, i64, op_impl::i64_shr_s);
        }
        Opcode::I64_SHR_U => {
            binary_fn!(stack, i64, i64, op_impl::i64_shr_u);
        }
        Opcode::I64_ROTL => {
            binary_fn!(stack, i64, i64, op_impl::i64_rotl);
        }
        Opcode::I64_ROTR => {
            binary_fn!(stack, i64, i64, op_impl::i64_rotr);
        }
        Opcode::F32_ABS => {
            unary_fn!(stack, f32, f32, |x: f32| x.abs());
        }
        Opcode::F32_NEG => {
            unary_fn!(stack, f32, f32, |x: f32| -x);
        }
        Opcode::F32_CEIL => {
            unary_fn!(stack, f32, f32, |x: f32| x.ceil());
        }
        Opcode::F32_FLOOR => {
            unary_fn!(stack, f32, f32, |x: f32| x.floor());
        }
        Opcode::F32_TRUNC => {
            // Code for F32_TRUNC
        }
        Opcode::F32_NEAREST => {
            unary_fn!(stack, f32, f32, |x: f32| x.round());
        }
        Opcode::F32_SQRT => {
            unary_fn!(stack, f32, f32, |x: f32| x.sqrt());
        }
        Opcode::F32_ADD => {
            binary_fn!(stack, f32, f32, op_impl::f32_add);
        }
        Opcode::F32_SUB => {
            binary_fn!(stack, f32, f32, op_impl::f32_sub);
        }
        Opcode::F32_MUL => {
            binary_fn!(stack, f32, f32, op_impl::f32_mul);
        }
        Opcode::F32_DIV => {
            div_f!(stack, f32);
        }
        Opcode::F32_MIN => {
            binary_fn!(stack, f32, f32, op_impl::f32_min);
        }
        Opcode::F32_MAX => {
            binary_fn!(stack, f32, f32, op_impl::f32_max);
        }
        Opcode::F32_COPYSIGN => {
            binary_fn!(stack, f32, f32, op_impl::f32_copysign);
        }
        Opcode::F64_ABS => {
            unary_fn!(stack, f64, f64, |x: f64| x.abs());
        }
        Opcode::F64_NEG => {
            unary_fn!(stack, f64, f64, |x: f64| -x);
        }
        Opcode::F64_CEIL => {
            unary_fn!(stack, f64, f64, |x: f64| x.ceil());
        }
        Opcode::F64_FLOOR => {
            unary_fn!(stack, f64, f64, |x: f64| x.floor());
        }
        Opcode::F64_TRUNC => {
            // Code for F64_TRUNC
        }
        Opcode::F64_NEAREST => {
            unary_fn!(stack, f64, f64, |x: f64| x.round());
        }
        Opcode::F64_SQRT => {
            unary_fn!(stack, f64, f64, |x: f64| x.sqrt());
        }
        Opcode::F64_ADD => {
            binary_fn!(stack, f64, f64, op_impl::f64_add);
        }
        Opcode::F64_SUB => {
            binary_fn!(stack, f64, f64, op_impl::f64_sub);
        }
        Opcode::F64_MUL => {
            binary_fn!(stack, f64, f64, op_impl::f64_mul);
        }
        Opcode::F64_DIV => {
            div_f!(stack, f64);
        }
        Opcode::F64_MIN => {
            binary_fn!(stack, f64, f64, op_impl::f64_min);
        }
        Opcode::F64_MAX => {
            binary_fn!(stack, f64, f64, op_impl::f64_max);
        }
        Opcode::F64_COPYSIGN => {
            binary_fn!(stack, f64, f64, op_impl::f64_copysign);
        }
        Opcode::I32_WRAP_I64 => {
            unary_fn!(stack, i64, i32, |x: i64| x as i32);
        }
        Opcode::I32_TRUNC_F32_S => {
            unary_fn!(stack, f32, i32, |x: f32| x as i32);
        }
        Opcode::I32_TRUNC_F32_U => {
            unary_fn!(stack, f32, i32, |x: f32| (x as u32) as i32);
        }
        Opcode::I32_TRUNC_F64_S => {
            unary_fn!(stack, f64, i32, |x: f64| x as i32);
        }
        Opcode::I32_TRUNC_F64_U => {
            unary_fn!(stack, f64, i32, |x: f64| (x as u32) as i32);
        }
        Opcode::I64_EXTEND_I32_S => {
            unary_fn!(stack, i32, i64, |x: i32| x as i64);
        }
        Opcode::I64_EXTEND_I32_U => {
            unary_fn!(stack, i32, i64, |x: i32| (x as u32) as i64);
        }
        Opcode::I64_TRUNC_F32_S => {
            unary_fn!(stack, f32, i64, |x: f32| x as i64);
        }
        Opcode::I64_TRUNC_F32_U => {
            unary_fn!(stack, f32, i64, |x: f32| (x as u64) as i64);
        }
        Opcode::I64_TRUNC_F64_S => {
            unary_fn!(stack, f64, i64, |x: f64| x as i64);
        }
        Opcode::I64_TRUNC_F64_U => {
            unary_fn!(stack, f64, i64, |x: f64| (x as u64) as i64);
        }
        Opcode::F32_CONVERT_I32_S => {
            unary_fn!(stack, i32, f32, |x: i32| x as f32);
        }
        Opcode::F32_CONVERT_I32_U => {
            unary_fn!(stack, i32, f32, |x: i32| (x as u32) as f32);
        }
        Opcode::F32_CONVERT_I64_S => {
            unary_fn!(stack, i64, f32, |x: i64| x as f32);
        }
        Opcode::F32_CONVERT_I64_U => {
            unary_fn!(stack, i64, f32, |x: i64| (x as u64) as f32);
        }
        Opcode::F32_DEMOTE_F64 => {
            unary_fn!(stack, f64, f32, |x: f64| x as f32);
        }
        Opcode::F64_CONVERT_I32_S => {
            unary_fn!(stack, i32, f64, |x: i32| x as f64);
        }
        Opcode::F64_CONVERT_I32_U => {
            unary_fn!(stack, i32, f64, |x: i32| (x as u32) as f64);
        }
        Opcode::F64_CONVERT_I64_S => {
            unary_fn!(stack, i64, f64, |x: i64| x as f64);
        }
        Opcode::F64_CONVERT_I64_U => {
            unary_fn!(stack, i64, f64, |x: i64| (x as u64) as f64);
        }
        Opcode::F64_PROMOTE_F32 => {
            unary_fn!(stack, f32, f64, |x: f32| x as f64);
        }
        Opcode::I32_REINTERPRET_F32 => {
            unary_fn!(stack, i32, f32, |x: i32| f32::from_bits(x as u32));
        }
        Opcode::I64_REINTERPRET_F64 => {
            unary_fn!(stack, i64, f64, |x: i64| f64::from_bits(x as u64));
        }
        Opcode::F32_REINTERPRET_I32 => {
            unary_fn!(stack, f32, i32, |x: f32| x.to_bits() as i32);
        }
        Opcode::F64_REINTERPRET_I64 => {
            unary_fn!(stack, f64, i64, |x: f64| x.to_bits() as i64);
        }
    }
}
