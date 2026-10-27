//! IronClaw tool `market_board`. Logic lives in `claw_tool_core::market_board`.

wit_bindgen::generate!({
    world: "sandboxed-tool",
    path: "../wit/tool.wit",
});

claw_tool_core::claw_tool!(market_board);
