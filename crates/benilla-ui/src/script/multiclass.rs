//! Optional WM native extension. Transport, admission and lifetime belong to the app.

use std::sync::Arc;

use mlua::{MultiValue, Value};

use super::{Model, UiScript, UiTarget};

/// The installed WM addon ABI; arguments are op, serial, guid/talent, class, token low/high.
/// A true admission is not a successful server mutation. Poll carries the server's receipt.
pub trait MulticlassHost: Send + Sync {
    fn request(&self, vm: u64, arguments: [u32; 6]) -> bool;
    fn poll(&self, vm: u64) -> Option<String>;
}

impl UiScript {
    /// Install only for an explicitly enabled Wrath host. Read the live model's identity on
    /// each call: a fork must not inherit the original VM's authority through its closures.
    pub fn set_multiclass_host(&self, host: Arc<dyn MulticlassHost>) -> mlua::Result<()> {
        if self.target() != UiTarget::Wrath335 {
            return Ok(());
        }
        let requests = Arc::clone(&host);
        self.lua.globals().set("MulticlassRequest", self.lua.create_function(
            move |lua, values: MultiValue| {
                let mut arguments = [0; 6];
                for (index, argument) in arguments.iter_mut().enumerate() {
                    // lua_tonumber in the WM DLL accepts numeric strings and maps missing or
                    // nonnumeric values to zero. Preserve it, then enforce the uint32 domain.
                    let value = values.get(index).cloned().unwrap_or(Value::Nil);
                    let number = lua.coerce_number(value)?.unwrap_or(0.0);
                    if !number.is_finite() || number < 0.0 || number > u32::MAX as f64
                        || number.fract() != 0.0
                    {
                        return Ok(0u32);
                    }
                    *argument = number as u32;
                }
                let vm = lua.app_data_ref::<Model>().unwrap().session;
                Ok(u32::from(requests.request(vm, arguments)))
            },
        )?)?;
        self.lua.globals().set("MulticlassPoll", self.lua.create_function(
            move |lua, _: MultiValue| {
                let vm = lua.app_data_ref::<Model>().unwrap().session;
                match host.poll(vm) {
                    Some(reply) => Ok(MultiValue::from_vec(vec![Value::String(
                        lua.create_string(reply)?,
                    )])),
                    None => Ok(MultiValue::new()),
                }
            },
        )?)
    }
}
