//! IronClaw tool `market_bid`. Logic lives in `claw_tool_core::market_bid`.

wit_bindgen::generate!({
    world: "sandboxed-tool",
    path: "../wit/tool.wit",
});

claw_tool_core::claw_tool!(market_bid);
