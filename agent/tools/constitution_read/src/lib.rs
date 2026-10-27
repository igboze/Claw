//! IronClaw tool `constitution_read`. Logic lives in `claw_tool_core::constitution_read`.

wit_bindgen::generate!({
    world: "sandboxed-tool",
    path: "../wit/tool.wit",
});

claw_tool_core::claw_tool!(constitution_read);
