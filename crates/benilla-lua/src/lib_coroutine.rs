//! Lua 5.1 coroutine library, installed only by the explicit Wrath constructor.
//! Contract: https://www.lua.org/source/5.1/lbaselib.c.html (coroutine functions).

use crate::api::R;
use crate::heap::FuncObj;
use crate::value::{ThreadRef, Value, V};
use crate::vm::Vm;

pub(crate) fn open(vm: &mut Vm) {
    let table = vm.heap.new_table(0, 0);
    let functions: [(&'static str, crate::heap::NativeFn); 6] = [
        ("create", create), ("resume", resume), ("yield", yield_),
        ("status", status), ("running", running), ("wrap", wrap),
    ];
    for (name, native) in functions {
        let value = vm.new_native(name, native);
        let key = vm.str(name.as_bytes());
        vm.heap.table_mut(table).set(key, value).unwrap();
    }
    vm.set_global("coroutine", Value::Table(table));
}

fn check_thread(vm: &mut Vm, nargs: usize) -> R<ThreadRef> {
    match vm.arg(nargs, 0).unpack() {
        V::Thread(thread) => Ok(thread),
        _ => Err(vm.arg_error(1, "coroutine expected")),
    }
}

fn create(vm: &mut Vm, nargs: usize) -> R<usize> {
    let entry = vm.arg(nargs, 0);
    if !matches!(entry.unpack(), V::Func(function) if matches!(vm.heap.func(function), FuncObj::Lua(_))) {
        return Err(vm.arg_error(1, "Lua function expected"));
    }
    let thread = vm.create_thread(entry);
    vm.push(Value::Thread(thread));
    Ok(1)
}

fn resume(vm: &mut Vm, nargs: usize) -> R<usize> {
    let thread = check_thread(vm, nargs)?;
    let args = vm.args(nargs)[1..].to_vec();
    match vm.resume_thread(thread, &args) {
        Ok(values) => {
            if !vm.check_stack(values.len() + 1) {
                return Err(vm.rt_error("too many results to resume"));
            }
            vm.push(Value::Bool(true));
            for &value in &values { vm.push(value); }
            Ok(values.len() + 1)
        }
        Err(error) if vm.coroutine_aborted() => Err(error),
        Err(error) => {
            vm.push(Value::Bool(false));
            vm.push(error.value);
            Ok(2)
        }
    }
}

fn yield_(vm: &mut Vm, nargs: usize) -> R<usize> { vm.yield_thread(nargs) }

fn status(vm: &mut Vm, nargs: usize) -> R<usize> {
    let thread = check_thread(vm, nargs)?;
    let value = vm.str(vm.thread_status(thread));
    vm.push(value);
    Ok(1)
}

fn running(vm: &mut Vm, _: usize) -> R<usize> {
    let value = vm.running_thread().map(Value::Thread).unwrap_or(Value::Nil);
    vm.push(value);
    Ok(1)
}

fn wrap(vm: &mut Vm, nargs: usize) -> R<usize> {
    create(vm, nargs)?;
    let thread = vm.stack[vm.top - 1];
    let wrapper = vm.new_closure("coroutine.wrap", wrapped_resume, vec![thread]);
    vm.push(wrapper);
    Ok(1)
}

fn wrapped_resume(vm: &mut Vm, nargs: usize) -> R<usize> {
    let V::Thread(thread) = vm.upvalue(0).unpack() else { unreachable!("wrapper owns its thread") };
    let args = vm.args(nargs).to_vec();
    match vm.resume_thread(thread, &args) {
        Ok(values) => {
            if !vm.check_stack(values.len() + 1) {
                return Err(vm.rt_error("too many results to resume"));
            }
            for &value in &values { vm.push(value); }
            Ok(values.len())
        }
        Err(error) if vm.coroutine_aborted() => Err(error),
        Err(error) => {
            let value = if let Some(string) = vm.tostr(error.value) {
                let mut text = vm.where_(1);
                text.extend_from_slice(vm.heap.str(string));
                vm.str(&text)
            } else { error.value };
            Err(vm.throw(value))
        }
    }
}
