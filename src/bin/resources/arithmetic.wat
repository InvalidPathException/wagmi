(module
  (func $add (export "add") (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.add)

  (func $sub (export "subtract") (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.sub)

  (func $mul (export "multiply") (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.mul)

  (func $div (export "divide") (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.div_s)

  (func $mod (export "modulo") (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.rem_s)
)