//! IronClaw tool `pay_bounty`. Logic lives in `claw_tool_core::pay_bounty`.

wit_bindgen::generate!({
    world: "sandboxed-tool",
    path: "../wit/tool.wit",
});

claw_tool_core::claw_tool!(pay_bounty);
