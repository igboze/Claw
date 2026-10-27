//! IronClaw tool `market_start`. Logic lives in `claw_tool_core::market_start`.

wit_bindgen::generate!({
    world: "sandboxed-tool",
    path: "../wit/tool.wit",
});

claw_tool_core::claw_tool!(market_start);
