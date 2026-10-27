//! IronClaw tool `mission_status`. Logic lives in `claw_tool_core::mission_status`.

wit_bindgen::generate!({
    world: "sandboxed-tool",
    path: "../wit/tool.wit",
});

claw_tool_core::claw_tool!(mission_status);
