//! Result validation before anything is committed (SPEC.md "The MCP server and client"): the
//! envelope as rascal's closed type and within the manifest's bounds, image blocks in exactly
//! the count, order and media type of `images`, each decoding to the declared size within the
//! bounds, and every region and image in a frame the call passed. A failure is an operational
//! error and nothing from the call is stored.

use std::collections::BTreeMap;

use image::DynamicImage;
use rascal_core::Error;
use rascal_core::error::Result;
use rascal_core::frame::Frame;
use rascal_core::op::{Envelope, Manifest, Status};
use rmcp::model::{CallToolResult, ContentBlock};

use crate::jsonrpc::{PNG, base64_decode};

/// A validated answer: the envelope and the decoded image of each `images` entry, in order.
#[derive(Debug)]
pub struct Validated {
    pub envelope: Envelope,
    pub images: Vec<DynamicImage>,
}

/// Validates a tool result against the tool's manifest and the frames the call passed.
pub fn validate(
    tool: &Manifest,
    result: &CallToolResult,
    frames: &BTreeMap<String, &Frame>,
) -> Result<Validated> {
    let name = &tool.name;
    let fail = |m: String| Error::op(name, format!("answer rejected: {m}"));
    let structured = result
        .structured_content
        .clone()
        .ok_or_else(|| fail("no structuredContent".into()))?;
    let bytes = serde_json::to_vec(&structured)
        .map(|v| v.len() as u64)
        .unwrap_or(u64::MAX);
    if bytes > tool.returns.max_envelope_bytes {
        return Err(fail(format!(
            "the envelope is {bytes} bytes, over max_envelope_bytes {}",
            tool.returns.max_envelope_bytes
        )));
    }
    let envelope: Envelope = serde_json::from_value(structured)
        .map_err(|e| fail(format!("the envelope does not parse: {e}")))?;
    if !envelope.artifacts.is_empty() {
        return Err(fail(
            "a pack names no artifacts; rascal publishes them".into(),
        ));
    }
    let flagged = result.is_error == Some(true);
    if flagged != (envelope.status == Status::InvalidArgs) {
        return Err(fail(format!(
            "isError is {flagged} but the status is {:?}",
            envelope.status
        )));
    }
    envelope.check_bounds(tool).map_err(fail)?;
    // Frames: every region and every image in a frame the call passed, with valid geometry.
    for region in &envelope.regions {
        let frame = frames.get(region.location.frame()).ok_or_else(|| {
            fail(format!(
                "region {} is in frame {}, which the call did not pass",
                region.id,
                region.location.frame()
            ))
        })?;
        region
            .location
            .validate(frame)
            .map_err(|e| fail(format!("region {}: {e}", region.id)))?;
        if !region.score.is_finite() {
            return Err(fail(format!("region {}: score is not finite", region.id)));
        }
    }
    for (i, listed) in envelope.images.iter().enumerate() {
        if !frames.contains_key(&listed.frame) {
            return Err(fail(format!(
                "image {i} is in frame {}, which the call did not pass",
                listed.frame
            )));
        }
        if listed.media_type != PNG {
            return Err(fail(format!(
                "image {i} is {}, not {PNG}",
                listed.media_type
            )));
        }
    }
    // Blocks: exactly one image block per `images` entry, in order, PNG, decoding to the
    // declared size within the bounds.
    let blocks: Vec<&rmcp::model::ImageContent> = result
        .content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Image(image) => Some(image),
            _ => None,
        })
        .collect();
    if blocks.len() != envelope.images.len() {
        return Err(fail(format!(
            "{} image blocks for {} images",
            blocks.len(),
            envelope.images.len()
        )));
    }
    let mut images = Vec::with_capacity(blocks.len());
    for (i, (block, listed)) in blocks.iter().zip(&envelope.images).enumerate() {
        if block.mime_type != PNG {
            return Err(fail(format!(
                "image block {i} is {}, not {PNG}",
                block.mime_type
            )));
        }
        let bytes = base64_decode(&block.data)
            .ok_or_else(|| fail(format!("image block {i}: base64 does not decode")))?;
        if bytes.len() as u64 > tool.returns.max_image_bytes {
            return Err(fail(format!(
                "image block {i} is {} bytes, over max_image_bytes {}",
                bytes.len(),
                tool.returns.max_image_bytes
            )));
        }
        let long_edge = listed.width.max(listed.height);
        if long_edge > tool.returns.max_long_edge {
            return Err(fail(format!(
                "image {i} is {}x{}, over max_long_edge {}",
                listed.width, listed.height, tool.returns.max_long_edge
            )));
        }
        let frame = frames[&listed.frame];
        if (listed.width, listed.height) != frame_size(frame) {
            return Err(fail(format!(
                "image {i} is {}x{} but frame {} is {}x{}",
                listed.width,
                listed.height,
                frame.id,
                frame_size(frame).0,
                frame_size(frame).1
            )));
        }
        let pixels = rascal_core::image::decode_bytes_within(&bytes, listed.width, listed.height)
            .map_err(|e| fail(format!("image block {i} did not decode: {e}")))?;
        if (pixels.width(), pixels.height()) != (listed.width, listed.height) {
            return Err(fail(format!(
                "image block {i} is {}x{} but images[{i}] says {}x{}",
                pixels.width(),
                pixels.height(),
                listed.width,
                listed.height
            )));
        }
        images.push(pixels);
    }
    Ok(Validated { envelope, images })
}

/// A frame's pixel size.
pub fn frame_size(frame: &Frame) -> (u32, u32) {
    (frame.width, frame.height)
}
