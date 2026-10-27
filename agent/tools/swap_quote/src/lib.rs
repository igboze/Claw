//! IronClaw tool `swap_quote`. Logic lives in `claw_tool_core::swap_quote`.

wit_bindgen::generate!({
    world: "sandboxed-tool",
    path: "../wit/tool.wit",
});

claw_tool_core::claw_tool!(swap_quote);
