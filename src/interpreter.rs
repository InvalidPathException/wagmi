use crate::{binary_fn, unary_fn};
use crate::specs::opcodes::Opcode;
use crate::specs::{op_impl, WasmValue};

fn execute_opcode(opcode: Opcode, stack: &mut Vec<WasmValue>) {
    match opcode {
        Opcode::UNREACHABLE => {
            // Code for UNREACHABLE
        }
        Opcode::NOP => {
            // Code for NOP
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
            // Code for DROP
        }
        Opcode::SELECT => {
            // Code for SELECT
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
            // Code for I32_LOAD
        }
        Opcode::I64_LOAD => {
            // Code for I64_LOAD
        }
        Opcode::F32_LOAD => {
            // Code for F32_LOAD
        }
        Opcode::F64_LOAD => {
            // Code for F64_LOAD
        }
        Opcode::I32_LOAD8_S => {
            // Code for I32_LOAD8_S
        }
        Opcode::I32_LOAD8_U => {
            // Code for I32_LOAD8_U
        }
        Opcode::I32_LOAD16_S => {
            // Code for I32_LOAD16_S
        }
        Opcode::I32_LOAD16_U => {
            // Code for I32_LOAD16_U
        }
        Opcode::I64_LOAD8_S => {
            // Code for I64_LOAD8_S
        }
        Opcode::I64_LOAD8_U => {
            // Code for I64_LOAD8_U
        }
        Opcode::I64_LOAD16_S => {
            // Code for I64_LOAD16_S
        }
        Opcode::I64_LOAD16_U => {
            // Code for I64_LOAD16_U
        }
        Opcode::I64_LOAD32_S => {
            // Code for I64_LOAD32_S
        }
        Opcode::I64_LOAD32_U => {
            // Code for I64_LOAD32_U
        }
        Opcode::I32_STORE => {
            // Code for I32_STORE
        }
        Opcode::I64_STORE => {
            // Code for I64_STORE
        }
        Opcode::F32_STORE => {
            // Code for F32_STORE
        }
        Opcode::F64_STORE => {
            // Code for F64_STORE
        }
        Opcode::I32_STORE8 => {
            // Code for I32_STORE8
        }
        Opcode::I32_STORE16 => {
            // Code for I32_STORE16
        }
        Opcode::I64_STORE8 => {
            // Code for I64_STORE8
        }
        Opcode::I64_STORE16 => {
            // Code for I64_STORE16
        }
        Opcode::I64_STORE32 => {
            // Code for I64_STORE32
        }
        Opcode::MEMORY_SIZE => {
            // Code for MEMORY_SIZE
        }
        Opcode::MEMORY_GROW => {
            // Code for MEMORY_GROW
        }
        Opcode::I32_CONST => {
            // Code for I32_CONST
        }
        Opcode::I64_CONST => {
            // Code for I64_CONST
        }
        Opcode::F32_CONST => {
            // Code for F32_CONST
        }
        Opcode::F64_CONST => {
            // Code for F64_CONST
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
            // Code for I32_CLZ
        }
        Opcode::I32_CTZ => {
            // Code for I32_CTZ
        }
        Opcode::I32_POPCNT => {
            // Code for I32_POPCNT
        }
        Opcode::I32_ADD => {
            // Code for I32_ADD
        }
        Opcode::I32_SUB => {
            binary_fn!(stack, i32, i32, +);
        }
        Opcode::I32_MUL => {
            // Code for I32_MUL
        }
        Opcode::I32_DIV_S => {
            // Code for I32_DIV_S
        }
        Opcode::I32_DIV_U => {
            // Code for I32_DIV_U
        }
        Opcode::I32_REM_S => {
            // Code for I32_REM_S
        }
        Opcode::I32_REM_U => {
            // Code for I32_REM_U
        }
        Opcode::I32_AND => {
            // Code for I32_AND
        }
        Opcode::I32_OR => {
            // Code for I32_OR
        }
        Opcode::I32_XOR => {
            // Code for I32_XOR
        }
        Opcode::I32_SHL => {
            // Code for I32_SHL
        }
        Opcode::I32_SHR_S => {
            // Code for I32_SHR_S
        }
        Opcode::I32_SHR_U => {
            // Code for I32_SHR_U
        }
        Opcode::I32_ROTL => {
            // Code for I32_ROTL
        }
        Opcode::I32_ROTR => {
            // Code for I32_ROTR
        }
        Opcode::I64_CLZ => {
            // Code for I64_CLZ
        }
        Opcode::I64_CTZ => {
            // Code for I64_CTZ
        }
        Opcode::I64_POPCNT => {
            // Code for I64_POPCNT
        }
        Opcode::I64_ADD => {
            // Code for I64_ADD
        }
        Opcode::I64_SUB => {
            // Code for I64_SUB
        }
        Opcode::I64_MUL => {
            // Code for I64_MUL
        }
        Opcode::I64_DIV_S => {
            // Code for I64_DIV_S
        }
        Opcode::I64_DIV_U => {
            // Code for I64_DIV_U
        }
        Opcode::I64_REM_S => {
            // Code for I64_REM_S
        }
        Opcode::I64_REM_U => {
            // Code for I64_REM_U
        }
        Opcode::I64_AND => {
            // Code for I64_AND
        }
        Opcode::I64_OR => {
            // Code for I64_OR
        }
        Opcode::I64_XOR => {
            // Code for I64_XOR
        }
        Opcode::I64_SHL => {
            // Code for I64_SHL
        }
        Opcode::I64_SHR_S => {
            // Code for I64_SHR_S
        }
        Opcode::I64_SHR_U => {
            // Code for I64_SHR_U
        }
        Opcode::I64_ROTL => {
            // Code for I64_ROTL
        }
        Opcode::I64_ROTR => {
            // Code for I64_ROTR
        }
        Opcode::F32_ABS => {
            // Code for F32_ABS
        }
        Opcode::F32_NEG => {
            // Code for F32_NEG
        }
        Opcode::F32_CEIL => {
            // Code for F32_CEIL
        }
        Opcode::F32_FLOOR => {
            // Code for F32_FLOOR
        }
        Opcode::F32_TRUNC => {
            // Code for F32_TRUNC
        }
        Opcode::F32_NEAREST => {
            // Code for F32_NEAREST
        }
        Opcode::F32_SQRT => {
            // Code for F32_SQRT
        }
        Opcode::F32_ADD => {
            // Code for F32_ADD
        }
        Opcode::F32_SUB => {
            // Code for F32_SUB
        }
        Opcode::F32_MUL => {
            // Code for F32_MUL
        }
        Opcode::F32_DIV => {
            // Code for F32_DIV
        }
        Opcode::F32_MIN => {
            // Code for F32_MIN
        }
        Opcode::F32_MAX => {
            // Code for F32_MAX
        }
        Opcode::F32_COPYSIGN => {
            // Code for F32_COPYSIGN
        }
        Opcode::F64_ABS => {
            // Code for F64_ABS
        }
        Opcode::F64_NEG => {
            // Code for F64_NEG
        }
        Opcode::F64_CEIL => {
            // Code for F64_CEIL
        }
        Opcode::F64_FLOOR => {
            // Code for F64_FLOOR
        }
        Opcode::F64_TRUNC => {
            // Code for F64_TRUNC
        }
        Opcode::F64_NEAREST => {
            // Code for F64_NEAREST
        }
        Opcode::F64_SQRT => {
            // Code for F64_SQRT
        }
        Opcode::F64_ADD => {
            // Code for F64_ADD
        }
        Opcode::F64_SUB => {
            // Code for F64_SUB
        }
        Opcode::F64_MUL => {
            // Code for F64_MUL
        }
        Opcode::F64_DIV => {
            // Code for F64_DIV
        }
        Opcode::F64_MIN => {
            // Code for F64_MIN
        }
        Opcode::F64_MAX => {
            // Code for F64_MAX
        }
        Opcode::F64_COPYSIGN => {
            // Code for F64_COPYSIGN
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
