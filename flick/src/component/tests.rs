use wasmtime::Store;
use wasmtime::component::{Component, Linker};

use super::{MemoryLimits, engine, memory_limits, prepare, why};

struct State {
    limits: MemoryLimits,
}

fn store(output: usize) -> Store<State> {
    let mut store = Store::new(
        engine().expect("engine"),
        State {
            limits: memory_limits(64 << 10),
        },
    );
    store.limiter(|state| &mut state.limits);
    prepare(&mut store, 100_000, output).expect("budgets");
    store
}

/// Thirty-two list entries alias the same 4 KiB buffer. The component has
/// only 64 KiB of linear memory but the owned reply would contain 128 KiB.
fn aliased_reply() -> Component {
    let mut entries = String::new();
    for _ in 0..32 {
        entries.push_str("\\00\\10\\00\\00\\00\\10\\00\\00");
    }
    let bytes = wat::parse_str(format!(
        r#"(component
      (core module $m
        (memory (export "memory") 1)
        (data (i32.const 0) "\10\00\00\00\20\00\00\00")
        (data (i32.const 16) "{entries}")
        (func (export "reply") (result i32) i32.const 0))
      (core instance $i (instantiate $m))
      (type $bytes (list u8))
      (type $reply (list $bytes))
      (func (export "reply") (result $reply)
        (canon lift (core func $i "reply") (memory $i "memory"))))"#
    ))
    .expect("component WAT");
    Component::new(engine().expect("engine"), bytes).expect("component")
}

#[test]
fn a_large_enough_output_budget_reads_the_aliased_binary_reply() {
    let mut store = store(256 << 10);
    let component = aliased_reply();
    let instance = Linker::new(engine().expect("engine"))
        .instantiate(&mut store, &component)
        .expect("instance");
    let reply = instance
        .get_typed_func::<(), (Vec<Vec<u8>>,)>(&mut store, "reply")
        .expect("function");
    let (bytes,) = reply.call(&mut store, ()).expect("reply");
    assert_eq!(bytes.len(), 32);
    assert_eq!(bytes.iter().map(Vec::len).sum::<usize>(), 128 << 10);
    assert!(bytes.iter().flatten().all(|&b| b == 0));
}

#[test]
fn output_budget_stops_alias_amplification_while_lifting() {
    let mut store = store(8 << 10);
    let component = aliased_reply();
    let instance = Linker::new(engine().expect("engine"))
        .instantiate(&mut store, &component)
        .expect("instance");
    let reply = instance
        .get_typed_func::<(), (Vec<Vec<u8>>,)>(&mut store, "reply")
        .expect("function");
    let error = reply
        .call(&mut store, ())
        .expect_err("128 KiB cannot pass an 8 KiB output budget");
    assert_eq!(why(&error), "exceeded its output limit");
}

#[test]
fn multiple_memories_share_the_store_memory_budget() {
    let bytes = wat::parse_str(
        r#"(component
      (core module $m (memory 1) (func (export "size") (result i32) memory.size))
      (core instance $a (instantiate $m))
      (core instance $b (instantiate $m))
      (func (export "first-size") (result u32) (canon lift (core func $a "size")))
      (func (export "second-size") (result u32) (canon lift (core func $b "size"))))"#,
    )
    .expect("component WAT");
    let component = Component::new(engine().expect("engine"), bytes).expect("component");
    let mut store = store(256 << 10);
    let result = Linker::new(engine().expect("engine")).instantiate(&mut store, &component);
    assert!(
        result.is_err(),
        "two 64 KiB memories cannot pass a 64 KiB store budget"
    );
}

#[test]
fn growing_memory_obeys_the_remaining_store_budget() {
    let bytes = wat::parse_str(
        r#"(component
      (core module $m
        (memory 0)
        (func (export "grow") (result i32) i32.const 1 memory.grow))
      (core instance $i (instantiate $m))
      (func (export "grow") (result u32) (canon lift (core func $i "grow"))))"#,
    )
    .expect("component WAT");
    let component = Component::new(engine().expect("engine"), bytes).expect("component");
    let mut store = store(256 << 10);
    let instance = Linker::new(engine().expect("engine"))
        .instantiate(&mut store, &component)
        .expect("instance");
    let grow = instance
        .get_typed_func::<(), (u32,)>(&mut store, "grow")
        .expect("function");
    assert_eq!(grow.call(&mut store, ()).expect("one page fits"), (0,));
    let error = grow
        .call(&mut store, ())
        .expect_err("a second page exceeds the budget");
    assert_eq!(why(&error), "ran out of its memory");
}

#[test]
fn multiple_tables_share_the_store_memory_budget() {
    let entries = (64 << 10) / size_of::<usize>();
    let bytes = wat::parse_str(format!(
        r#"(component
      (core module $m
        (table {entries} funcref)
        (func (export "size") (result i32) table.size))
      (core instance $a (instantiate $m))
      (core instance $b (instantiate $m))
      (func (export "first-size") (result u32) (canon lift (core func $a "size")))
      (func (export "second-size") (result u32) (canon lift (core func $b "size"))))"#
    ))
    .expect("component WAT");
    let component = Component::new(engine().expect("engine"), bytes).expect("component");
    let mut store = store(256 << 10);
    assert!(
        Linker::new(engine().expect("engine"))
            .instantiate(&mut store, &component)
            .is_err(),
        "two full-budget tables cannot pass one store budget"
    );
}
