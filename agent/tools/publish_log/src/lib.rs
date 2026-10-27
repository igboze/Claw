//! IronClaw tool `publish_log`. Logic lives in `claw_tool_core::publish_log`.

wit_bindgen::generate!({
    world: "sandboxed-tool",
    path: "../wit/tool.wit",
});

claw_tool_core::claw_tool!(publish_log);
