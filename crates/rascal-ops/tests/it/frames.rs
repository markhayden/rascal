//! Task 10.6 (CONTRACT 4.7, `wrong_frame`): the frames each op's inputs may be drawn in
//! together, checked before a served call runs. By default every input in one frame; `put`,
//! `unwindow`, `window`, `invert` and `measure` state their own relations.
#![allow(clippy::unwrap_used)]

use rascal_core::artifact::ArtifactType;
use rascal_core::frame::{Frame, Plan, Projection};
use rascal_core::op::{FrameMismatch, Framed};

fn framed<'a>(port: &'a str, artifact_type: ArtifactType, frame: &'a Frame) -> Framed<'a> {
    Framed {
        port,
        index: None,
        artifact_type,
        frame,
    }
}

/// The positions of the inputs `op` refuses.
fn refused(op: &str, inputs: &[Framed<'_>]) -> Vec<usize> {
    let registry = rascal_ops::registry().unwrap();
    registry
        .get(op)
        .unwrap()
        .check_frames(inputs)
        .into_iter()
        .map(|FrameMismatch { at, .. }| at)
        .collect()
}

#[test]
fn every_op_states_the_frames_its_inputs_share() {
    let raw = Frame::raw(800, 400, Projection::Equirectangular);
    let other = Frame::raw(320, 200, Projection::Flat);
    let crop = Frame::crop(&raw, 1, [10, 10, 90, 90]).unwrap();
    let plan = Plan::new(&raw, 0.25).unwrap();
    let cut = Frame::cut(&raw, "0123456789ab", &plan).unwrap();
    use ArtifactType::{Image, Location, Mask, Plan as PlanType};

    // The default: one frame, the first input's; each other input elsewhere is refused.
    assert!(
        refused(
            "paste",
            &[
                framed("base", Image, &raw),
                framed("fill", Image, &raw),
                framed("mask", Mask, &raw)
            ]
        )
        .is_empty()
    );
    assert_eq!(
        refused(
            "paste",
            &[
                framed("base", Image, &raw),
                framed("fill", Image, &raw),
                framed("mask", Mask, &crop)
            ]
        ),
        [2]
    );
    assert_eq!(
        refused(
            "rasterize",
            &[
                framed("location", Location, &other),
                framed("image", Image, &raw)
            ]
        ),
        [1]
    );
    // put: the canvas in the cut frame of the image's frame, the plan made for it.
    assert!(
        refused(
            "put",
            &[
                framed("image", Image, &raw),
                framed("canvas", Image, &cut),
                framed("plan", PlanType, &raw)
            ]
        )
        .is_empty()
    );
    assert_eq!(
        refused(
            "put",
            &[
                framed("image", Image, &raw),
                framed("canvas", Image, &crop),
                framed("plan", PlanType, &other)
            ]
        ),
        [1, 2]
    );
    // unwindow: the crop in a crop: frame of the image's frame.
    assert!(
        refused(
            "unwindow",
            &[framed("image", Image, &raw), framed("crop", Image, &crop)]
        )
        .is_empty()
    );
    assert_eq!(
        refused(
            "unwindow",
            &[framed("image", Image, &other), framed("crop", Image, &crop)]
        ),
        [1]
    );
    assert_eq!(
        refused(
            "unwindow",
            &[framed("image", Image, &raw), framed("crop", Image, &cut)]
        ),
        [1]
    );
    // window: the image and the mask in one frame (the crop is what it makes).
    assert_eq!(
        refused(
            "window",
            &[framed("image", Image, &raw), framed("mask", Mask, &crop)]
        ),
        [1]
    );
    // invert: one input, any frame.
    assert!(refused("invert", &[framed("mask", Mask, &crop)]).is_empty());
    // measure: the images and the mask in one frame; the line, a column fraction, in any.
    assert!(
        refused(
            "measure",
            &[
                framed("after", Image, &raw),
                framed("mask", Mask, &raw),
                framed("line", Location, &cut)
            ]
        )
        .is_empty()
    );
    assert_eq!(
        refused(
            "measure",
            &[
                framed("after", Image, &raw),
                framed("before", Image, &crop),
                framed("mask", Mask, &raw),
                framed("line", Location, &cut)
            ]
        ),
        [1]
    );
}
