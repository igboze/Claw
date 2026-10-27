//! IronClaw tool `market_submit`. Logic lives in `claw_tool_core::market_submit`.

wit_bindgen::generate!({
    world: "sandboxed-tool",
    path: "../wit/tool.wit",
});

claw_tool_core::claw_tool!(market_submit);
