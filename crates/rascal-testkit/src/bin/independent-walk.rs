//! The independent client walking a `rascal serve` through the seam operations, for the
//! acceptance script (G10; task 10.6): `cut` on an image by path, then `line`, `fan`, `profile`,
//! `ramp` and `render grey`, each bound to the references the call before returned, the
//! rendering read back from its image out, and the mask's identity printed so the shell can
//! find it in the same store with the CLI. Exit 0 on success, 1 with the failure on stderr.
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
    // The image's directory is the one a caller's path may name (CONTRACT 5.2).
    let allowed = std::path::Path::new(image)
        .parent()
        .ok_or("the image has no directory")?;
    let mut command = Command::new(binary);
    command
        .args(["serve", "--workspace", workspace, "--allow-dir"])
        .arg(allowed);
    // The runs go beside the store, never into it.
    command.arg("--runs").arg(format!("{workspace}-runs"));
    let mut client = Client::start(&mut command).map_err(|e| format!("start: {e:?}"))?;
    // A completed call's result.
    let call = |client: &mut Client, name: &str, args: serde_json::Value| {
        let result = client
            .call(name, args)
            .map_err(|e| format!("{name}: {e:?}"))?;
        let structured = result
            .structured
            .ok_or(format!("{name}: no structuredContent"))?;
        if result.is_error || structured["status"] != "completed" {
            return Err(format!("{name}: {structured}"));
        }
        Ok::<_, String>(structured)
    };
    // The reference an operation made on `port`.
    let made = |result: &serde_json::Value, port: &str| -> Result<serde_json::Value, String> {
        result["artifacts"]["out"]
            .as_array()
            .and_then(|out| out.iter().find(|e| e["port"] == port))
            .map(|e| e["artifact"].clone())
            .ok_or_else(|| format!("no artifact {port} in {result}"))
    };
    let cut = call(
        &mut client,
        "cut",
        serde_json::json!({ "image": { "path": image }, "strip": 0.07 }),
    )?;
    let canvas = made(&cut, "image")?;
    let line = call(
        &mut client,
        "line",
        serde_json::json!({ "image": canvas, "column": 0.5, "width_px": 2.0 }),
    )?;
    let fan = call(
        &mut client,
        "fan",
        serde_json::json!({ "image": canvas, "line": made(&line, "location")? }),
    )?;
    let profile = call(
        &mut client,
        "profile",
        serde_json::json!({ "band": made(&fan, "band")? }),
    )?;
    let ramp = call(
        &mut client,
        "ramp",
        serde_json::json!({ "image": canvas, "mask": made(&profile, "mask")? }),
    )?;
    let mask = made(&ramp, "mask")?;
    let rendered = call(
        &mut client,
        "render",
        serde_json::json!({ "mask": mask, "for": "grey" }),
    )?;
    let rendering = made(&rendered, "image")?;
    let path = rendered["images"]["out"]
        .as_array()
        .and_then(|out| out.iter().find(|o| o["role"] == "image"))
        .and_then(|o| o["path"].as_str())
        .ok_or_else(|| format!("render returned no image by path: {rendered}"))?
        .to_string();
    let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    if !bytes.starts_with(b"\x89PNG") {
        return Err(format!("{path} is not a PNG"));
    }
    println!("mask: {}", mask["identity"].as_str().unwrap_or_default());
    println!(
        "rendering: {} ({} bytes)",
        rendering["identity"].as_str().unwrap_or_default(),
        bytes.len()
    );
    client.close().map_err(|e| e.to_string())?;
    Ok(())
}
