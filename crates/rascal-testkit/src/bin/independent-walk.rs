//! The independent client walking a `rascal serve` through the seam ops, for the acceptance
//! script (G10): import an image, `cut`, `line`, `fan`, `profile`, `ramp`, `render grey`, read
//! the rendering back, and print the mask's id so the shell can read the same workspace with
//! the CLI. Exit 0 on success, 1 with the failure on stderr.
//!
//!   independent-walk <rascal binary> <workspace> <image>

use std::process::Command;

use rascal_testkit::mcp_client::Client;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [binary, workspace, image] = args.as_slice() else {
        eprintln!("usage: independent-walk <rascal binary> <workspace> <image>");
        std::process::exit(64);
    };
    if let Err(e) = walk(binary, workspace, image) {
        eprintln!("independent-walk: {e}");
        std::process::exit(1);
    }
}

fn walk(binary: &str, workspace: &str, image: &str) -> Result<(), String> {
    let mut command = Command::new(binary);
    command.args(["serve", "--workspace", workspace]);
    let mut client = Client::start(&mut command).map_err(|e| format!("start: {e:?}"))?;
    let id =
        |result: rascal_testkit::mcp_client::CallResult, port: &str| -> Result<String, String> {
            let structured = result.structured.ok_or("no structuredContent")?;
            if result.is_error {
                return Err(format!("{port}: {structured}"));
            }
            structured["artifacts"][port]["id"]
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("no artifact {port} in {structured}"))
        };
    let call = |client: &mut Client, name: &str, args: serde_json::Value| {
        client
            .call(name, args)
            .map_err(|e| format!("{name}: {e:?}"))
    };
    let imported = call(
        &mut client,
        "workspace_import",
        serde_json::json!({ "path": image }),
    )?;
    let image_id = id(imported, "image")?;
    let cut = call(
        &mut client,
        "cut",
        serde_json::json!({ "image": image_id, "strip": 0.07 }),
    )?;
    let canvas = id(cut, "image")?;
    let line = call(
        &mut client,
        "line",
        serde_json::json!({ "image": canvas, "column": 0.5, "width_px": 2.0 }),
    )?;
    let line_id = id(line, "location")?;
    let fan = call(
        &mut client,
        "fan",
        serde_json::json!({ "image": canvas, "line": line_id }),
    )?;
    let band = id(fan, "band")?;
    let profile = call(&mut client, "profile", serde_json::json!({ "band": band }))?;
    let band_mask = id(profile, "mask")?;
    let ramp = call(
        &mut client,
        "ramp",
        serde_json::json!({ "image": canvas, "mask": band_mask }),
    )?;
    let mask = id(ramp, "mask")?;
    let rendered = call(
        &mut client,
        "render",
        serde_json::json!({ "mask": mask, "for": "grey" }),
    )?;
    let rendering = id(rendered, "image")?;
    let read = call(
        &mut client,
        "workspace_read",
        serde_json::json!({ "id": rendering }),
    )?;
    if read.images.len() != 1 || read.images[0].0 != "image/png" {
        return Err(format!(
            "workspace_read returned {} image blocks",
            read.images.len()
        ));
    }
    println!("mask: {mask}");
    println!("rendering: {rendering} ({} bytes)", read.images[0].1.len());
    client.close().map_err(|e| e.to_string())?;
    Ok(())
}
