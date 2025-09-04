;; Factorial implementation
(module
  ;; Iterative factorial
  (func $factorial_iter (export "factorial") (param $n i32) (result i32)
    (local $result i32)
    (local $i i32)
    
    ;; Initialize result to 1
    i32.const 1
    local.set $result
    
    ;; Initialize counter to 1
    i32.const 1
    local.set $i
    
    ;; Main loop
    (block $done
      (loop $loop
        ;; Check if i > n
        local.get $i
        local.get $n
        i32.gt_s
        br_if $done
        
        ;; result = result * i
        local.get $result
        local.get $i
        i32.mul
        local.set $result
        
        ;; i = i + 1
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        
        ;; Continue loop
        br $loop))
    
    ;; Return result
    local.get $result)
  
  ;; Recursive factorial (for comparison)
  (func $factorial_rec (export "factorial_recursive") (param $n i32) (result i32)
    ;; Base case: if n <= 1, return 1
    local.get $n
    i32.const 1
    i32.le_s
    if (result i32)
      i32.const 1
    else
      ;; Recursive case: n * factorial(n-1)
      local.get $n
      local.get $n
      i32.const 1
      i32.sub
      call $factorial_rec
      i32.mul
    end)
)