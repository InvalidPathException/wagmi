;; Control flow examples
(module
  ;; Fibonacci sequence
  (func $fibonacci (export "fibonacci") (param $n i32) (result i32)
    (local $a i32)
    (local $b i32)
    (local $temp i32)
    (local $i i32)
    
    ;; Handle base cases
    local.get $n
    i32.const 2
    i32.lt_s
    if (result i32)
      local.get $n
    else
      ;; Initialize
      i32.const 0
      local.set $a
      i32.const 1
      local.set $b
      i32.const 2
      local.set $i
      
      ;; Loop
      (block $done
        (loop $loop
          local.get $i
          local.get $n
          i32.gt_s
          br_if $done
          
          ;; temp = a + b
          local.get $a
          local.get $b
          i32.add
          local.set $temp
          
          ;; a = b, b = temp
          local.get $b
          local.set $a
          local.get $temp
          local.set $b
          
          ;; i++
          local.get $i
          i32.const 1
          i32.add
          local.set $i
          
          br $loop))
      
      local.get $b
    end)
  
  ;; Maximum of two numbers
  (func $max (export "max") (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.gt_s
    if (result i32)
      local.get $a
    else
      local.get $b
    end)
  
  ;; Minimum of two numbers
  (func $min (export "min") (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.lt_s
    if (result i32)
      local.get $a
    else
      local.get $b
    end)
  
  ;; Absolute value
  (func $abs (export "abs") (param $n i32) (result i32)
    local.get $n
    i32.const 0
    i32.lt_s
    if (result i32)
      i32.const 0
      local.get $n
      i32.sub
    else
      local.get $n
    end)
  
  ;; Sign function (-1, 0, or 1)
  (func $sign (export "sign") (param $n i32) (result i32)
    local.get $n
    i32.const 0
    i32.eq
    if (result i32)
      i32.const 0
    else
      local.get $n
      i32.const 0
      i32.gt_s
      if (result i32)
        i32.const 1
      else
        i32.const -1
      end
    end)
)