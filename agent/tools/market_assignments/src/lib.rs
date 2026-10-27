//! IronClaw tool `market_assignments`. Logic lives in `claw_tool_core::market_assignments`.

wit_bindgen::generate!({
    world: "sandboxed-tool",
    path: "../wit/tool.wit",
});

claw_tool_core::claw_tool!(market_assignments);
