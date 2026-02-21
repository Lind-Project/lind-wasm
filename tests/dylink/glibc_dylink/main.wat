(module $main.wasm
  (type (;0;) (func (param i32 i32) (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (type (;2;) (func (param i32)))
  (type (;3;) (func))
  (type (;4;) (func (param f64 f64) (result f64)))
  (type (;5;) (func (param f64) (result f64)))
  (type (;6;) (func (result i32)))
  (import "env" "memory" (memory (;0;) 1 1024 shared))
  (import "env" "__indirect_function_table" (table (;0;) 1 funcref))
  (import "env" "__stack_pointer" (global $__stack_pointer (mut i32)))
  (import "env" "__memory_base" (global $__memory_base i32))
  (import "env" "__table_base" (global $__table_base i32))
  (import "wasi_snapshot_preview1" "environ_get" (func $__imported_wasi_snapshot_preview1_environ_get (type 0)))
  (import "wasi_snapshot_preview1" "environ_sizes_get" (func $__imported_wasi_snapshot_preview1_environ_sizes_get (type 0)))
  (import "env" "malloc" (func $malloc (type 1)))
  (import "env" "calloc" (func $calloc (type 0)))
  (import "env" "free" (func $free (type 2)))
  (import "env" "_Exit" (func $_Exit (type 2)))
  (import "env" "__lind_init_addr_translation" (func $__lind_init_addr_translation (type 3)))
  (import "env" "__libc_setup_tls" (func $__libc_setup_tls (type 3)))
  (import "env" "__wasi_init_tp" (func $__wasi_init_tp (type 3)))
  (import "env" "__ctype_init" (func $__ctype_init (type 3)))
  (import "env" "printf" (func $printf (type 0)))
  (import "env" "pow" (func $pow (type 4)))
  (import "env" "sin" (func $sin (type 5)))
  (import "env" "cos" (func $cos (type 5)))
  (import "env" "log" (func $log (type 5)))
  (import "GOT.mem" "environ" (global $environ (mut i32)))
  (func $__wasm_call_ctors (type 3))
  (func $__wasm_apply_data_relocs (type 3)
    i32.const 96
    global.get $__memory_base
    i32.add
    global.get $__table_base
    i32.const 0
    i32.add
    i32.store)
  (func $__wasm_init_memory (type 3)
    (local i32 i32)
    global.get $__memory_base
    i32.const 104
    i32.add
    local.set 0
    block  ;; label = @1
      block  ;; label = @2
        block  ;; label = @3
          local.get 0
          i32.const 0
          i32.const 1
          i32.atomic.rmw.cmpxchg
          br_table 0 (;@3;) 1 (;@2;) 2 (;@1;)
        end
        i32.const 0
        global.get $__memory_base
        i32.add
        i32.const 0
        i32.const 94
        memory.init $.rodata
        i32.const 96
        global.get $__memory_base
        i32.add
        i32.const 0
        i32.const 4
        memory.init $.data
        i32.const 100
        global.get $__memory_base
        i32.add
        i32.const 0
        i32.const 4
        memory.fill
        local.get 0
        i32.const 2
        i32.atomic.store
        local.get 0
        i32.const -1
        memory.atomic.notify
        drop
        br 1 (;@1;)
      end
      local.get 0
      i32.const 1
      i64.const -1
      memory.atomic.wait32
      drop
    end
    data.drop $.rodata
    data.drop $.data)
  (func $__wasi_initialize_environ (type 3)
    (local i32 i32 i32)
    global.get $__stack_pointer
    i32.const 16
    i32.sub
    local.tee 0
    global.set $__stack_pointer
    global.get $__memory_base
    local.set 1
    local.get 0
    i32.const 12
    i32.add
    local.get 0
    i32.const 8
    i32.add
    call $__imported_wasi_snapshot_preview1_environ_sizes_get
    drop
    block  ;; label = @1
      block  ;; label = @2
        local.get 0
        i32.load offset=12
        local.tee 2
        br_if 0 (;@2;)
        local.get 1
        i32.const 100
        i32.add
        local.set 2
        br 1 (;@1;)
      end
      block  ;; label = @2
        block  ;; label = @3
          local.get 2
          i32.const 1
          i32.add
          local.tee 2
          i32.eqz
          br_if 0 (;@3;)
          local.get 0
          i32.load offset=8
          call $malloc
          local.tee 1
          i32.eqz
          br_if 0 (;@3;)
          local.get 2
          i32.const 4
          call $calloc
          local.tee 2
          br_if 1 (;@2;)
          local.get 1
          call $free
        end
        i32.const 70
        call $_Exit
        unreachable
      end
      local.get 2
      local.get 1
      call $__imported_wasi_snapshot_preview1_environ_get
      drop
    end
    global.get $environ
    local.get 2
    i32.store
    local.get 0
    i32.const 16
    i32.add
    global.set $__stack_pointer)
  (func $__wasm_call_dtors (type 3))
  (func $__unused_function_pointer (type 6) (result i32)
    i32.const 42)
  (func $_start (type 6) (result i32)
    call $__lind_init_addr_translation
    call $__libc_setup_tls
    call $__wasi_init_tp
    call $__wasi_initialize_environ
    call $__ctype_init
    call $__original_main)
  (func $__original_main (type 6) (result i32)
    (local i32 i32 i32 i32 f64 f64 f64 f64 f64 i32 i32 i32 f64 f64 f64 f64 i32 i32 i32 i32 i32 f64 f64 i32 i32 i32 i32 i32 f64 f64 i32 i32 i32 i32 i32 f64 f64 f64 i32 i32 i32 i32 i32 i32 i32 i32)
    global.get $__stack_pointer
    local.set 0
    i32.const 112
    local.set 1
    local.get 0
    local.get 1
    i32.sub
    local.set 2
    local.get 2
    global.set $__stack_pointer
    i32.const 0
    local.set 3
    local.get 2
    local.get 3
    i32.store offset=108
    f64.const 0x1.2p+3 (;=9;)
    local.set 4
    local.get 2
    local.get 4
    f64.store offset=96
    f64.const 0x1.921fb54442d18p-1 (;=0.785398;)
    local.set 5
    local.get 2
    local.get 5
    f64.store offset=88
    local.get 2
    f64.load offset=96
    local.set 6
    local.get 2
    f64.load offset=96
    local.set 7
    local.get 7
    f64.sqrt
    local.set 8
    local.get 2
    local.get 8
    f64.store offset=8
    local.get 2
    local.get 6
    f64.store
    i32.const 54
    local.set 9
    global.get $__memory_base
    local.set 10
    local.get 10
    local.get 9
    i32.add
    local.set 11
    local.get 11
    local.get 2
    call $printf
    drop
    local.get 2
    f64.load offset=96
    local.set 12
    local.get 2
    f64.load offset=96
    local.set 13
    f64.const 0x1p+1 (;=2;)
    local.set 14
    local.get 13
    local.get 14
    call $pow
    local.set 15
    local.get 2
    local.get 15
    f64.store offset=24
    local.get 2
    local.get 12
    f64.store offset=16
    i32.const 73
    local.set 16
    global.get $__memory_base
    local.set 17
    local.get 17
    local.get 16
    i32.add
    local.set 18
    i32.const 16
    local.set 19
    local.get 2
    local.get 19
    i32.add
    local.set 20
    local.get 18
    local.get 20
    call $printf
    drop
    local.get 2
    f64.load offset=88
    local.set 21
    local.get 21
    call $sin
    local.set 22
    local.get 2
    local.get 22
    f64.store offset=32
    i32.const 18
    local.set 23
    global.get $__memory_base
    local.set 24
    local.get 24
    local.get 23
    i32.add
    local.set 25
    i32.const 32
    local.set 26
    local.get 2
    local.get 26
    i32.add
    local.set 27
    local.get 25
    local.get 27
    call $printf
    drop
    local.get 2
    f64.load offset=88
    local.set 28
    local.get 28
    call $cos
    local.set 29
    local.get 2
    local.get 29
    f64.store offset=48
    i32.const 0
    local.set 30
    global.get $__memory_base
    local.set 31
    local.get 31
    local.get 30
    i32.add
    local.set 32
    i32.const 48
    local.set 33
    local.get 2
    local.get 33
    i32.add
    local.set 34
    local.get 32
    local.get 34
    call $printf
    drop
    local.get 2
    f64.load offset=96
    local.set 35
    local.get 2
    f64.load offset=96
    local.set 36
    local.get 36
    call $log
    local.set 37
    local.get 2
    local.get 37
    f64.store offset=72
    local.get 2
    local.get 35
    f64.store offset=64
    i32.const 36
    local.set 38
    global.get $__memory_base
    local.set 39
    local.get 39
    local.get 38
    i32.add
    local.set 40
    i32.const 64
    local.set 41
    local.get 2
    local.get 41
    i32.add
    local.set 42
    local.get 40
    local.get 42
    call $printf
    drop
    i32.const 0
    local.set 43
    i32.const 112
    local.set 44
    local.get 2
    local.get 44
    i32.add
    local.set 45
    local.get 45
    global.set $__stack_pointer
    local.get 43
    return)
  (global $__tls_base (mut i32) (i32.const 0))
  (export "memory" (memory 0))
  (export "__wasm_apply_data_relocs" (func $__wasm_apply_data_relocs))
  (export "_start" (func $_start))
  (start $__wasm_init_memory)
  (elem (;0;) (global.get $__table_base) func $__unused_function_pointer)
  (data $.rodata "cos(45\c2\b0) = %.4f\0a\00sin(45\c2\b0) = %.4f\0a\00log(%.2f) = %.4f\0a\00sqrt(%.2f) = %.2f\0a\00pow(%.2f, 2) = %.2f\0a\00")
  (data $.data "\00\00\00\00")
  (@custom "dylink.0" "\01\04l\02\01\00")
  (@custom ".debug_loc" "\ff\ff\ff\ff\fe\ff\ff\ff\0b\00\00\00\10\00\00\00\04\00\ed\02\00\9f\00\00\00\00\00\00\00\00\ff\ff\ff\ff\fe\ff\ff\ff\0b\00\00\00\10\00\00\00\04\00\ed\02\00\9f\00\00\00\00\00\00\00\00\ff\ff\ff\ff\fe\ff\ff\ff\0b\00\00\00\10\00\00\00\04\00\ed\02\00\9f\00\00\00\00\00\00\00\00\ff\ff\ff\ff\fe\ff\ff\ff\0b\00\00\00\10\00\00\00\04\00\ed\02\00\9f\00\00\00\00\00\00\00\00\ff\ff\ff\ff\89\00\00\00Q\00\00\00S\00\00\00\04\00\ed\02\00\9fS\00\00\00t\00\00\00\04\00\ed\00\02\9f\00\00\00\00\00\00\00\00\ff\ff\ff\ff\89\00\00\00a\00\00\00c\00\00\00\04\00\ed\02\00\9fc\00\00\00|\00\00\00\04\00\ed\00\01\9f\87\00\00\00\93\00\00\00\04\00\ed\00\01\9f\00\00\00\00\00\00\00\00\ff\ff\ff\ff\89\00\00\00p\00\00\00r\00\00\00\04\00\ed\02\00\9fr\00\00\00|\00\00\00\04\00\ed\00\02\9f\87\00\00\00\93\00\00\00\04\00\ed\00\02\9f\00\00\00\00\00\00\00\00")
  (@custom ".debug_abbrev" "\01\11\01%\0e\13\05\03\0e\10\17\1b\0e\11\01U\17\00\00\024\00\03\0eI\13?\19:\0b;\0b\02\18\00\00\03\0f\00\00\00\044\00\03\0eI\13:\0b;\0b\02\18\00\00\05\01\01I\13\00\00\06!\00I\137\0b\00\00\07\0f\00I\13\00\00\08$\00\03\0e>\0b\0b\0b\00\00\09$\00\03\0e\0b\0b>\0b\00\00\0a.\01\11\01\12\06@\18\97B\19\03\0e:\0b;\0b'\19I\13?\19\00\00\0b\05\00\02\18\03\0e:\0b;\0bI\13\00\00\0c4\00\02\17\03\0e:\0b;\0bI\13\00\00\0d\89\82\01\001\13\11\01\00\00\0e.\01\03\0e:\0b;\0b'\19I\13<\19?\19\00\00\0f\05\00I\13\00\00\10.\01\11\01\12\06@\18\97B\191\13\00\00\11\05\00\02\181\13\00\00\124\00\02\171\13\00\00\13.\01\03\0e:\0b;\0b'\19I\13?\19 \0b\00\00\14\05\00\03\0e:\0b;\0bI\13\00\00\154\00\03\0e:\0b;\0bI\13\00\00\16.\01\11\01\12\06@\18\97B\19\03\0e:\0b;\0b'\19?\19\00\00\174\00\02\18\03\0e:\0b;\0bI\13\00\00\18\0a\00\03\0e:\0b;\0b\11\01\00\00\19\0a\00\03\0e:\0b;\0b\00\00\1a\1d\001\13\11\01\12\06X\0bY\0bW\0b\00\00\1b.\01\03\0e:\0b;\05'\19I\13<\19?\19\00\00\1c\16\00I\13\03\0e:\0b;\0b\00\00\1d.\01\03\0e:\0b;\05'\19<\19?\19\00\00\1e.\01\03\0e:\0b;\05'\19<\19?\19\87\01\19\00\00\1f.\00\11\01\12\06@\18\97B\19\03\0e:\0b;\0b?\19\00\00 .\00\11\01\12\06@\18\97B\19\03\0e:\0b;\0bI\13?\19\00\00!.\01\11\01\12\06@\18\97B\19\03\0e:\0b;\0bI\13?\19\00\00\22.\00\03\0e:\0b;\0b'\19<\19?\19\00\00\00\01\11\01%\0e\13\05\03\0e\10\17\1b\0e\11\01\12\06\00\00\024\00I\13:\0b;\0b\02\18\00\00\03\01\01I\13\00\00\04!\00I\137\0b\00\00\05$\00\03\0e>\0b\0b\0b\00\00\06$\00\03\0e\0b\0b>\0b\00\00\07.\01\11\01\12\06@\18\03\0e:\0b;\0bI\13?\19\00\00\084\00\02\18\03\0e:\0b;\0bI\13\00\00\00")
  (@custom ".debug_info" "\8e\04\00\00\04\00\00\00\00\00\04\01H\03\00\00\1d\00\fc\02\00\00\00\00\00\00#\02\00\00\00\00\00\00\00\00\00\00\02\cf\02\00\00>\00\00\00\01\cb\0c\ed\03\01\00\00\00\03`\00\00\00\22\03\04\d9\01\00\00W\00\00\00\01\0c\0c\ed\03\01\00\00\00\03d\00\00\00\22\05c\00\00\00\06o\00\00\00\01\00\07h\00\00\00\08\d4\01\00\00\06\01\094\03\00\00\08\07\08@\00\00\00\05\04\08\13\00\00\00\07\02\07\89\00\00\00\07\8e\00\00\00\08\cb\01\00\00\08\01\0a\ff\ff\ff\ff\11\00\00\00\07\ed\03\00\00\00\00\9f\a7\00\00\00\01\13}\00\00\00\0b\04\ed\00\00\9fe\03\00\00\01\14;\02\00\00\0b\04\ed\00\01\9f]\03\00\00\01\15;\02\00\00\0c\00\00\00\00h\00\00\00\01\17v\00\00\00\0d\e9\00\00\00\ff\ff\ff\ff\00\0e\bd\00\00\00\01\0ev\00\00\00\0fv\00\00\00\0fv\00\00\00\00\0a\ff\ff\ff\ff\11\00\00\00\07\ed\03\00\00\00\00\9fl\00\00\00\01 }\00\00\00\0b\04\ed\00\00\9f\0e\00\00\00\01!\84\00\00\00\0b\04\ed\00\01\9f\84\02\00\00\01\22\89\00\00\00\0c\1e\00\00\00h\00\00\00\01$v\00\00\00\0dS\01\00\00\ff\ff\ff\ff\00\0e|\00\00\00\01\1bv\00\00\00\0fv\00\00\00\0fv\00\00\00\00\10\ff\ff\ff\ff\11\00\00\00\07\ed\03\00\00\00\00\9fG\02\00\00\11\04\ed\00\00\9fS\02\00\00\11\04\ed\00\01\9f^\02\00\00\12<\00\00\00i\02\00\00\0d\a5\01\00\00\ff\ff\ff\ff\00\0eN\01\00\00\01(v\00\00\00\0fv\00\00\00\0fv\00\00\00\00\10\ff\ff\ff\ff\11\00\00\00\07\ed\03\00\00\00\00\9f\0d\02\00\00\11\04\ed\00\00\9f\19\02\00\00\11\04\ed\00\01\9f$\02\00\00\12Z\00\00\00/\02\00\00\0d\f7\01\00\00\ff\ff\ff\ff\00\0e\07\01\00\00\015v\00\00\00\0fv\00\00\00\0fv\00\00\00\00\13\ee\00\00\00\01:}\00\00\00\01\14e\03\00\00\01;;\02\00\00\14]\03\00\00\01<;\02\00\00\15h\00\00\00\01>v\00\00\00\00\07@\02\00\00\08v\02\00\00\07\04\13;\01\00\00\01-}\00\00\00\01\14\f9\01\00\00\01.\84\00\00\00\14\8d\02\00\00\01/\89\00\00\00\15h\00\00\00\011v\00\00\00\00\16\89\00\00\00\ab\00\00\00\04\ed\00\00\9f\e7\01\00\00\01F\17\02\91\0c)\00\00\00\01HM\03\00\00\17\02\91\08\99\02\00\00\01IM\03\00\00\0cx\00\00\00\90\01\00\00\01TM\03\00\00\0c\a4\00\00\00\8d\02\00\00\01Zc\00\00\00\0c\de\00\00\00\83\01\00\00\01a%\04\00\00\18\aa\02\00\00\01t\06\01\00\00\19\ab\01\00\00\01r\1a\0d\02\00\00\a5\00\00\00\11\00\00\00\01J\05\1aG\02\00\00\11\01\00\00\0b\00\00\00\01j\05\0d\f7\01\00\00\b5\00\00\00\0d;\03\00\00\ea\00\00\00\0dX\03\00\00\f9\00\00\00\0do\03\00\00\05\01\00\00\0d}\03\00\00\0f\01\00\00\0d\a5\01\00\00\1b\01\00\00\00\1b\e2\02\00\00\02\a0\02>\00\00\00\0fM\03\00\00\00\1c@\02\00\00|\01\00\00\03\12\1b\e9\02\00\00\02\a3\02>\00\00\00\0fM\03\00\00\0fM\03\00\00\00\1d\c0\02\00\00\02\af\02\0f>\00\00\00\00\1eU\00\00\00\02\00\03\0fv\00\00\00\00\1f5\01\00\00\02\00\00\00\07\ed\03\00\00\00\00\9f\99\01\00\00\01x\16\ff\ff\ff\ff\02\00\00\00\07\ed\03\00\00\00\00\9fD\00\00\00\01|\14\c5\02\00\00\01|\8a\04\00\00\00\0a\ff\ff\ff\ff\15\00\00\00\07\ed\03\00\00\00\00\9f\02\00\00\00\01\8av\00\00\00\0b\04\ed\00\00\9f\f0\02\00\00\01\8av\00\00\00\0b\04\ed\00\01\9f\0e\00\00\00\01\8a%\04\00\00\0d\0a\04\00\00\ff\ff\ff\ff\00\0e\1e\02\00\00\01\81v\00\00\00\0fv\00\00\00\0f%\04\00\00\0f%\04\00\00\00\07c\00\00\00 8\01\00\00\04\00\00\00\07\ed\03\00\00\00\00\9f\b1\01\00\00\01\c7v\00\00\00!=\01\00\00&\00\00\00\07\ed\03\00\00\00\00\9f\22\00\00\00\01\cdv\00\00\00\0d|\04\00\00D\01\00\00\0du\02\00\00V\01\00\00\0d\83\04\00\00\5c\01\00\00\00\22\01\02\00\00\04 \22[\00\00\00\05\07\087\00\00\00\07\04\00\fe\00\00\00\04\00\fe\01\00\00\04\01H\03\00\00\1d\00\f5\02\00\00g\02\00\00G\02\00\00e\01\00\00\ff\01\00\00\02:\00\00\00\01\08\0c\ed\03\01\00\00\00\036\00\00\00\22\03F\00\00\00\04M\00\00\00\13\00\05\d4\01\00\00\06\01\064\03\00\00\08\07\02h\00\00\00\01\09\0c\ed\03\01\00\00\00\03I\00\00\00\22\03F\00\00\00\04M\00\00\00\15\00\02\88\00\00\00\01\0a\0c\ed\03\01\00\00\00\03\12\00\00\00\22\03F\00\00\00\04M\00\00\00\12\00\02\88\00\00\00\01\0b\0c\ed\03\01\00\00\00\03\00\00\00\00\22\02\88\00\00\00\01\0c\0c\ed\03\01\00\00\00\03$\00\00\00\22\07e\01\00\00\ff\01\00\00\04\ed\00\02\9f\1e\02\00\00\01\04\f3\00\00\00\08\03\91\e0\00\00\00\00\00\01\05\fa\00\00\00\08\03\91\d8\00\b3\02\00\00\01\06\fa\00\00\00\00\05@\00\00\00\05\04\05\b9\02\00\00\04\08\00")
  (@custom ".debug_ranges" "\fe\ff\ff\ff\fe\ff\ff\ff\fe\ff\ff\ff\fe\ff\ff\ff\fe\ff\ff\ff\fe\ff\ff\ff\fe\ff\ff\ff\fe\ff\ff\ff\89\00\00\004\01\00\005\01\00\007\01\00\00\fe\ff\ff\ff\fe\ff\ff\ff\fe\ff\ff\ff\fe\ff\ff\ff8\01\00\00<\01\00\00=\01\00\00c\01\00\00\00\00\00\00\00\00\00\00")
  (@custom ".debug_str" "x\00__main_argc_argv\00unsigned short\00_start\00environ_count\00unsigned int\00__wasi_proc_exit\00_Exit\00__ctype_init\00ret\00__wasi_args_get\00__imported_wasi_snapshot_preview1_args_get\00__wasi_args_sizes_get\00__imported_wasi_snapshot_preview1_args_sizes_get\00__wasi_environ_sizes_get\00__imported_wasi_snapshot_preview1_environ_sizes_get\00__wasi_environ_get\00__imported_wasi_snapshot_preview1_environ_get\00size_t\00environ_ptrs\00num_ptrs\00__wasm_call_dtors\00oserr\00__unused_function_pointer\00unsigned char\00empty_environ\00__wasi_initialize_environ\00__lind_init_addr_translation\00main\00/home/lind/lind-wasm/src/glibc/nptl\00/home/lind/lind-wasm/tests/dylink/glibc_dylink\00unsigned long\00argv_buf\00environ_buf\00environ_buf_size\00software\00angle\00double\00free\00exit_code\00___dummy_reference\00malloc\00calloc\00argc\00main.c\00/home/lind/lind-wasm/src/glibc/lind_syscall/crt1/crt1.c\00__ARRAY_SIZE_TYPE__\00clang version 18.1.8\00retptr1\00retptr0\00")
  (@custom ".debug_line" "c\02\00\00\04\00\10\01\00\00\01\01\01\fb\0e\0d\00\01\01\01\01\00\00\00\01\00\00\01/home/lind/lind-wasm/src/glibc\00../stdlib\00/home/lind/lind-wasm\00../lind_syscall\00../include\00\00lind_syscall/crt1/crt1.c\00\01\00\00stdlib.h\00\02\00\00clang+llvm-18.1.8-x86_64-linux-gnu-ubuntu-18.04/lib/clang/18/include/__stddef_size_t.h\00\03\00\00addr_translation.h\00\04\00\00ctype.h\00\05\00\00\00\05\16\0a\00\05\02\ff\ff\ff\ff\03\16\01\06\03i\9e\05\05\06\03\18J\02\02\00\01\01\05\16\0a\00\05\02\ff\ff\ff\ff\03#\01\06\03\5c\9e\05\05\06\03%J\02\02\00\01\01\05\16\0a\00\05\02\ff\ff\ff\ff\030\01\06\03O\9e\05\05\06\032J\02\02\00\01\01\05\16\0a\00\05\02\ff\ff\ff\ff\03=\01\06\03B\9e\05\05\06\03?J\02\02\00\01\01\00\05\02\89\00\00\00\03\c5\00\01\05\16\0a\03x\08\ac\05\09\03\10\08\12\06\90\03\b2\7fJ\05%\06\03\d4\00\08X\05\09!\05 ]\05\19\06X\05\09\06g\06\03\a5\7fX\05\1b\06\03\e1\00J\05\09gK\06\03\9d\7f\82\05\05\06\03\f5\00J\06\03\8b\7ft\05\16\06\031 \06\03O\ac\05\01\06\03\f6\00\ba\02\0c\00\01\01\05\01\0a\00\05\026\01\00\00\03\f9\00\01\02\01\00\01\01\05\01\0a\00\05\02\ff\ff\ff\ff\03\fd\00\01\02\01\00\01\01\05\1b\0a\00\05\02\ff\ff\ff\ff\03\8e\01\01\05\0a\06\c8\05\03f\02\01\00\01\01\00\05\028\01\00\00\03\c6\01\01\05\05\0a=\02\01\00\01\01\05\05\0a\00\05\02>\01\00\00\03\cd\01\01gggg\05\0cn\05\05\06f\02\01\00\01\01\86\00\00\00\04\00\1e\00\00\00\01\01\01\fb\0e\0d\00\01\01\01\01\00\00\00\01\00\00\01\00main.c\00\00\00\00\00\00\05\02e\01\00\00\15\05\0c\0a\02>\13\08!\05#\08\22\05+\06t\05&t\05\05X\05%\06\020\13\05,\06t\05(\08 \05\05\ba\05&\06\02;\13\05\22\06t\05\05\9e\05&\06\024\13\05\22\06t\05\05\9e\05\22\06\024\13\05)\06t\05%t\05\05\9e\06\02<\14\02\1c\00\01\01")
  (@custom "name" "\00\0a\09main.wasm\01\8f\03\17\00-__imported_wasi_snapshot_preview1_environ_get\013__imported_wasi_snapshot_preview1_environ_sizes_get\02\06malloc\03\06calloc\04\04free\05\05_Exit\06\1c__lind_init_addr_translation\07\10__libc_setup_tls\08\0e__wasi_init_tp\09\0c__ctype_init\0a\06printf\0b\03pow\0c\03sin\0d\03cos\0e\03log\0f\11__wasm_call_ctors\10\18__wasm_apply_data_relocs\11\12__wasm_init_memory\12\19__wasi_initialize_environ\13\11__wasm_call_dtors\14\19__unused_function_pointer\15\06_start\16\0f__original_main\07D\05\00\0f__stack_pointer\01\0d__memory_base\02\0c__table_base\03\07environ\04\0a__tls_base\09\11\02\00\07.rodata\01\05.data")
  (@custom "producers" "\02\08language\01\03C11\00\0cprocessed-by\01\05clang\0618.1.8")
  (@custom "target_features" "\04+\07atomics+\0bbulk-memory+\0fmutable-globals+\08sign-ext"))
