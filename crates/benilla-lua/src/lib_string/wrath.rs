//! Post-1.12 string globals used by the target SecureTemplates/FrameXML. Installed only in
//! Wrath mode. Behavioral reference: Elune's lstrlib.c str_split/trim/join/concat; this is an
//! independent compatibility implementation, not evidence of exact 12340 binary behavior.
//! https://github.com/wowless/wowless/blob/main/elune/liblua/lstrlib.c

use crate::api::R;
use crate::value::StrRef;
use crate::vm::Vm;

pub(super) fn open(vm: &mut Vm) {
    let table = vm
        .get_global("string")
        .as_table()
        .expect("string library installed");
    for (name, function) in [
        ("split", split as crate::heap::NativeFn),
        ("trim", trim),
        ("join", join),
    ] {
        let value = vm.new_native(name, function);
        let key = vm.str(name.as_bytes());
        vm.heap
            .table_mut(table)
            .set(key, value)
            .expect("library key");
    }
    vm.register("strsplit", split);
    vm.register("strtrim", trim);
    vm.register("strjoin", join);
    vm.register("strconcat", concat);
}

fn c_len(bytes: &[u8]) -> usize {
    bytes.iter().position(|&byte| byte == 0).unwrap_or(bytes.len())
}

fn charset(bytes: &[u8]) -> [bool; 256] {
    let mut set = [false; 256];
    for &byte in &bytes[..c_len(bytes)] { set[usize::from(byte)] = true; }
    set
}

fn push_slice(vm: &mut Vm, string: StrRef, first: usize, end: usize, bytes: &mut Vec<u8>) -> R<()> {
    bytes.clear();
    vm.reserve(bytes, end - first)?;
    bytes.extend_from_slice(&vm.heap.str(string)[first..end]);
    let result = vm.str(bytes);
    if vm.heap.oom { return Err(vm.memory_error()); }
    vm.push(result);
    Ok(())
}

fn split(vm: &mut Vm, nargs: usize) -> R<usize> {
    let delimiter = vm.check_str(nargs, 1)?;
    let string = vm.check_str(nargs, 2)?;
    let limit = vm.opt_int(nargs, 3, 0)?;
    let delimiters = charset(vm.heap.str(delimiter));
    let length = c_len(vm.heap.str(string));
    // Discard optional/surplus arguments before reserving results. Empty fields, including
    // the final one, count toward the limit; zero is unlimited and <= 1 keeps the whole input.
    vm.top = vm.native_base() + 2;
    let mut first = 0;
    let mut count = 0;
    let mut bytes = Vec::new();
    loop {
        if !vm.check_stack(1) {
            return Err(vm.error_at(1, "stack overflow (strsplit())"));
        }
        let remainder = limit != 0 && (limit <= 1 || count >= (limit - 1) as usize);
        let end = if remainder { length } else {
            vm.heap.str(string)[first..length].iter()
                .position(|&byte| delimiters[usize::from(byte)])
                .map_or(length, |offset| first + offset)
        };
        push_slice(vm, string, first, end, &mut bytes)?;
        count += 1;
        if end == length { return Ok(count); }
        first = end + 1;
    }
}

fn trim(vm: &mut Vm, nargs: usize) -> R<usize> {
    let string = vm.check_str(nargs, 1)?;
    let delimiter = vm.opt_str(nargs, 2, b" \r\n\t")?;
    let mut delimiters = charset(vm.heap.str(delimiter));
    // strchr also matches a C string's terminating NUL. Keep interior bytes, including NULs.
    delimiters[0] = true;
    let bytes = vm.heap.str(string);
    let mut first = 0;
    let mut end = bytes.len();
    while first < end && delimiters[usize::from(bytes[first])] { first += 1; }
    while end > first && delimiters[usize::from(bytes[end - 1])] { end -= 1; }
    push_slice(vm, string, first, end, &mut Vec::new())?;
    Ok(1)
}

/// lua_concat's zero/one cases matter: one value is returned untouched, while two or more
/// use the existing right-associative __concat and numeric formatting owner.
fn concat(vm: &mut Vm, nargs: usize) -> R<usize> {
    if nargs == 0 { return Ok(vm.ret_str(b"")); }
    let first = vm.top - nargs;
    if nargs > 1 { vm.concat(first, vm.top - 1)?; }
    vm.top = first + 1;
    Ok(1)
}

fn join(vm: &mut Vm, nargs: usize) -> R<usize> {
    let separator = vm.check_str(nargs, 1)?;
    let separator_len = c_len(vm.heap.str(separator));
    let pieces = nargs - 1;
    if pieces <= 1 || separator_len == 0 {
        return concat(vm, pieces);
    }
    let mut result = Vec::new();
    for index in 0..pieces {
        let piece = vm.check_str(nargs, index + 2)?;
        let length = c_len(vm.heap.str(piece));
        vm.reserve(&mut result, length)?;
        result.extend_from_slice(&vm.heap.str(piece)[..length]);
        if index + 1 < pieces {
            vm.reserve(&mut result, separator_len)?;
            result.extend_from_slice(&vm.heap.str(separator)[..separator_len]);
        }
    }
    Ok(vm.ret_str(&result))
}
