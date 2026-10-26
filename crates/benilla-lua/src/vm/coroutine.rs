//! Stackful Lua continuations. Rust/native callbacks are never suspended: yield unwinds only
//! the dispatch loop entered by resume, leaving its Lua frames and yield call in the heap.
//! The language contract follows https://www.lua.org/source/5.1/ldo.c.html; Rust unwinding
//! and shared host instruction-budget propagation are embedding-specific boundaries.

use super::*;

pub(crate) enum ResumeError {
    /// An admission failure raised in the resumer (for example, its argument-stack limit).
    Raised(LuaError),
    /// A coroutine refusal or an error from its execution: resume returns false plus this value.
    Failed(LuaError),
}

impl Vm {
    pub(crate) fn create_thread(&mut self, entry: Value) -> ThreadRef {
        let (stack_block, ci_block) = {
            let mut pools = self.heap.pools();
            (pools.alloc(STACK_SLOT_BYTES * BASIC_STACK_SIZE),
             pools.alloc(CI_BYTES * BASIC_CI_SIZE))
        };
        self.heap.new_thread(ThreadObj {
            entry,
            env: self.globals,
            status: ThreadStatus::Fresh,
            context: Some(ThreadContext {
                stack: vec![Value::Nil; BASIC_STACK_SIZE], top: 0,
                frames: Vec::new(), open: Vec::new(), globals: self.globals,
                c_depth: 0, ci_size: BASIC_CI_SIZE, ci_limit: BASIC_CI_SIZE - 1,
                clear_below: 0, in_handler: 0, stack_base: 0,
                handlers: Vec::new(), stack_block, ci_block, in_hook: false,
            }),
        })
    }

    pub(crate) fn running_thread(&self) -> Option<ThreadRef> { self.active_thread }

    pub(crate) fn thread_status(&self, thread: ThreadRef) -> &'static [u8] {
        if self.active_thread == Some(thread) { return b"running"; }
        match self.heap.thread(thread).status {
            ThreadStatus::Fresh | ThreadStatus::Suspended => b"suspended",
            ThreadStatus::Normal | ThreadStatus::Running => b"normal",
            ThreadStatus::Dead => b"dead",
        }
    }

    // Exchange ownership, without moving any stack buffer or prototype allocation. Other
    // threads' open cells address their parked stacks; only the active thread uses Open.
    fn exchange_thread_context(&mut self, context: &mut ThreadContext, parked: ThreadRef) {
        for &(index, upvalue) in &self.open {
            *self.heap.upval_mut(upvalue) = UpvalObj::Suspended { thread: parked, index: index as u32 };
        }
        macro_rules! exchange {
            ($($field:ident),* $(,)?) => { $(std::mem::swap(&mut self.$field, &mut context.$field);)* };
        }
        exchange!(stack, top, frames, open, globals, c_depth, ci_size, ci_limit,
            clear_below, in_handler, stack_base, handlers, stack_block, ci_block, in_hook);
        for &(index, upvalue) in &self.open {
            *self.heap.upval_mut(upvalue) = UpvalObj::Open(index as u32);
        }
    }

    pub(crate) fn resume_thread(&mut self, thread: ThreadRef, args: &[Value]) -> Result<Vec<Value>, ResumeError> {
        let status = self.heap.thread(thread).status;
        if !matches!(status, ThreadStatus::Fresh | ThreadStatus::Suspended) {
            let message = [b"cannot resume ".as_slice(), self.thread_status(thread), b" coroutine"].concat();
            return Err(ResumeError::Failed(LuaError { value: self.str(&message) }));
        }
        let context = self.heap.thread(thread).context.as_ref().expect("resumable context");
        let base = context.frames.last().map_or(0, |frame| frame.base());
        if args.len() > crate::api::MAX_C_STACK
            || context.top - base + args.len() > crate::api::MAX_C_STACK
        {
            return Err(ResumeError::Raised(self.rt_error("too many arguments to resume")));
        }
        // Per-thread call depths alone cannot bound a chain of native resume calls.
        let marker = 0u8;
        let here = std::ptr::addr_of!(marker) as usize;
        let root_base = if self.resume_parents.is_empty() {
            if self.stack_base == 0 { here } else { self.stack_base }
        } else { self.resume_stack_base };
        let used = if cfg!(miri) { 0 } else { root_base.abs_diff(here) };
        if self.resume_parents.len() >= MAXCCALLS as usize || used > MAX_RUST_STACK {
            return Err(ResumeError::Failed(LuaError { value: self.str(b"C stack overflow") }));
        }
        let previous = self.active_thread;
        self.resume_stack_base = root_base;
        let parent = previous.unwrap_or_else(|| {
            if let Some(main) = self.main_thread { return main; }
            let main = self.heap.new_thread(ThreadObj {
                context: None, entry: Value::Nil, env: self.globals, status: ThreadStatus::Running,
            });
            self.main_thread = Some(main);
            main
        });
        let source = self.heap.cur_source;
        let identity = self.identity();
        let mut context = self.heap.thread_mut(thread).context.take().expect("suspended thread owns its stack");
        self.exchange_thread_context(&mut context, parent);
        self.heap.thread_mut(parent).context = Some(context);
        self.heap.thread_mut(parent).status = ThreadStatus::Normal;
        self.heap.thread_mut(thread).status = ThreadStatus::Running;
        self.resume_parents.push(parent);
        self.active_thread = Some(thread);
        self.stack_base = root_base;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.run_thread(thread, status, args)
        }));
        // A host replacing the VM invalidates every saved handle, just as at call_at's boundary.
        if self.identity() != identity {
            match result {
                Err(payload) => std::panic::resume_unwind(payload),
                Ok(_) => panic!("VM replaced while resuming a coroutine"),
            }
        }
        let yielded = self.yielded.take();
        let suspended = yielded.is_some() && matches!(&result, Ok(Err(_))) && self.abort.is_none();
        let values = if let Some(count) = yielded.filter(|_| suspended) {
            self.stack[self.top - count..self.top].to_vec()
        } else { Vec::new() };
        if !suspended {
            self.close_upvals(0);
            self.frames.clear();
            self.handlers.clear();
            self.top = 0;
            self.heap.thread_mut(thread).entry = Value::Nil;
        }
        self.c_depth = 0;
        let mut context = self.heap.thread_mut(parent).context.take().expect("resumer owns its parked stack");
        self.exchange_thread_context(&mut context, thread);
        self.heap.thread_mut(thread).context = Some(context);
        self.heap.thread_mut(thread).status = if suspended { ThreadStatus::Suspended } else { ThreadStatus::Dead };
        self.heap.thread_mut(parent).status = ThreadStatus::Running;
        self.active_thread = previous;
        self.resume_parents.pop();
        self.heap.cur_source = source;
        if self.hooked { self.prof_switch(); }
        match result {
            Err(payload) => std::panic::resume_unwind(payload),
            Ok(_) if suspended => Ok(values),
            Ok(result) => result.map_err(ResumeError::Failed),
        }
    }

    fn run_thread(&mut self, thread: ThreadRef, status: ThreadStatus, args: &[Value]) -> Result<Vec<Value>, LuaError> {
        self.c_depth = 1;
        if status == ThreadStatus::Fresh {
            let entry = self.heap.thread(thread).entry;
            self.push(entry);
            for &arg in args { self.push(arg); }
            match self.precall(0, args.len(), -1)? {
                Called::Lua => self.frames.last_mut().unwrap().boundary = true,
                Called::Done => return Ok(self.stack[..self.top].to_vec()),
            }
        } else {
            let frame = self.frames.pop().expect("yield retains its native frame");
            debug_assert!(frame.proto.is_null());
            let first = self.top;
            for &arg in args { self.push(arg); }
            self.poscall(frame.func_idx(), first, args.len(), frame.nresults);
            if frame.nresults >= 0 {
                let caller = self.frames.last().expect("yield has a Lua caller");
                self.top = caller.base() + caller.maxstack as usize;
            }
        }
        self.execute()?;
        Ok(self.stack[..self.top].to_vec())
    }

    pub(crate) fn yield_thread(&mut self, nargs: usize) -> Result<usize, LuaError> {
        if self.active_thread.is_none() {
            return Err(self.rt_error("attempt to yield from outside a coroutine"));
        }
        if self.c_depth != 1 || self.in_hook || self.in_handler != 0 || !self.handlers.is_empty() {
            return Err(self.rt_error("attempt to yield across metamethod/C-call boundary"));
        }
        self.yielded = Some(nargs);
        // This is internal control flow, not a thrown Lua error: no error handler may run.
        Err(LuaError { value: Value::Nil })
    }

    pub(crate) fn coroutine_aborted(&self) -> bool { self.abort.is_some() }
}
