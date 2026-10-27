//! IronClaw tool `near_view`. Logic lives in `claw_tool_core::near_view`.

wit_bindgen::generate!({
    world: "sandboxed-tool",
    path: "../wit/tool.wit",
});

claw_tool_core::claw_tool!(near_view);
