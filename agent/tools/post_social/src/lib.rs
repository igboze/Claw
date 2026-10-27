//! IronClaw tool `post_social`. Logic lives in `claw_tool_core::post_social`.

wit_bindgen::generate!({
    world: "sandboxed-tool",
    path: "../wit/tool.wit",
});

claw_tool_core::claw_tool!(post_social);
