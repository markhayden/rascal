//! Task 1.2: frames and identity under canonical serialization. The committed vectors in
//! `fixtures/identity/vectors.json` pin every identity the core computes for known inputs;
//! `UPDATE_VECTORS=1` rewrites them, which is a deliberate change recorded in ADR 0016.
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use image::DynamicImage;
use rascal_core::canonical;
use rascal_core::frame::{Frame, FrameKind, FrameTable, Plan, Projection, Topology};
use rascal_core::identity::Identity;
use rascal_core::image::{Image, RenderingRole, Role};
use rascal_core::location::Location;
use rascal_core::mask::{Mask, Source, UseCase};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// --- canonical serialization -----------------------------------------------------------------

#[test]
fn canonical_json_sorts_keys_everywhere_and_has_no_whitespace() {
    let value = serde_json::json!({ "b": [ { "z": 1, "a": 2 } ], "a": { "y": true, "x": null } });
    assert_eq!(
        canonical::json(&value),
        r#"{"a":{"x":null,"y":true},"b":[{"a":2,"z":1}]}"#
    );
}

#[test]
fn identities_are_kind_prefixed_and_well_formed() {
    let a = canonical::identity("frame", &serde_json::json!({ "x": 1 }));
    let b = canonical::identity("location", &serde_json::json!({ "x": 1 }));
    assert_ne!(a, b, "the kind is part of the hash");
    assert!(Identity::parse(a.as_str()).is_some());
    assert_eq!(a.hex().len(), 64);
    assert!(Identity::parse("sha256:ABCD").is_none());
    assert!(Identity::parse("md5:00").is_none());
}

#[test]
fn stored_pngs_decode_to_the_same_samples() {
    let dir = std::env::temp_dir().join(format!("rascal-canonical-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let pixels = DynamicImage::ImageRgb8(rascal_testkit::images::floor(40, 30));
    let path = dir.join("a.png");
    canonical::write_png(&path, &pixels).unwrap();
    let again = image::open(&path).unwrap();
    assert_eq!(again.as_bytes(), pixels.as_bytes());
    let gray = image::GrayImage::from_fn(40, 30, |x, y| image::Luma([((x + y) % 256) as u8]));
    canonical::write_gray(&dir.join("g.png"), &gray).unwrap();
    assert_eq!(
        image::open(dir.join("g.png")).unwrap().to_luma8().as_raw(),
        gray.as_raw()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// --- frames -----------------------------------------------------------------------------------

#[test]
fn raw_frames_are_keyed_by_size_and_projection_not_pixels() {
    let a = Frame::raw(320, 200, Projection::Flat);
    let b = Frame::raw(320, 200, Projection::Flat);
    let c = Frame::raw(320, 201, Projection::Flat);
    let d = Frame::raw(400, 200, Projection::Equirectangular);
    assert_eq!(a, b);
    assert_eq!(a.id, "raw:320x200:flat");
    assert_ne!(a.identity, c.identity);
    assert_eq!(a.topology, Topology::Bounded);
    assert_eq!(d.topology, Topology::Wrap);
    assert_eq!(d.id, "raw:400x200:equirectangular");
    assert!(a.is_raw() && a.parent().is_none());
}

#[test]
fn the_strip_width_follows_the_spec_formula() {
    for (w, f, s) in [
        (4, 0.07, 2),
        (5, 0.07, 2),
        (10, 0.07, 2),
        (10, 0.3, 4),
        (5760, 0.07, 404),
        (8, 0.5, 4),
        (7, 0.5, 2),
    ] {
        assert_eq!(Plan::strip_width(w, f), s, "W = {w}, f = {f}");
        assert!(s % 2 == 0 && s >= 2 && 2 * s <= w);
    }
}

#[test]
fn cut_and_crop_frames_carry_their_parent_and_mapping() {
    let pano = Frame::raw(4000, 2000, Projection::Equirectangular);
    let plan = Plan::new(&pano, 0.07).unwrap();
    assert_eq!(plan.strip, 280);
    assert_eq!(plan.left_span, [3720, 4000]);
    assert_eq!(plan.right_span, [0, 280]);
    let cut = Frame::cut(&pano, "img-1", &plan).unwrap();
    assert_eq!((cut.width, cut.height), (560, 2000));
    assert_eq!(cut.topology, Topology::Bounded);
    assert_eq!(cut.projection, Projection::Flat);
    assert_eq!(cut.parent(), Some(pano.id.as_str()));
    assert!(cut.id.starts_with("cut:img-1:"));
    assert_eq!(cut.kind, FrameKind::Cut);
    assert_eq!(cut.plan.as_ref(), Some(&plan));
    cut.validate().unwrap();
    // The plan and the cut frame depend on the parent's identity and the strip, not on the
    // image id, so two workspaces agree.
    let cut2 = Frame::cut(&pano, "img-9", &plan).unwrap();
    assert_eq!(cut.identity, cut2.identity);
    let plan2 = Plan::new(&pano, 0.1).unwrap();
    assert_ne!(
        Frame::cut(&pano, "img-1", &plan2).unwrap().identity,
        cut.identity
    );
    assert_ne!(plan2.identity, plan.identity);
    // A flat frame cannot be cut; a short one neither.
    assert!(Frame::cut(&Frame::raw(320, 200, Projection::Flat), "x", &plan).is_err());
    assert!(Plan::new(&Frame::raw(3, 2, Projection::Equirectangular), 0.07).is_err());
    assert!(Plan::new(&pano, 0.6).is_err());

    let flat = Frame::raw(320, 200, Projection::Flat);
    let crop = Frame::crop(&flat, 1, [64, 40, 128, 100]).unwrap();
    assert_eq!((crop.width, crop.height), (64, 60));
    assert_eq!(crop.id, "crop:1");
    assert_eq!(crop.parent(), Some(flat.id.as_str()));
    assert!(Frame::crop(&flat, 2, [64, 40, 64, 100]).is_err(), "empty");
    assert!(Frame::crop(&flat, 2, [0, 0, 321, 10]).is_err(), "outside");
    assert_ne!(
        Frame::crop(&flat, 2, [64, 40, 128, 101]).unwrap().identity,
        crop.identity
    );

    let mut table = FrameTable::default();
    table.insert(flat.clone());
    table.insert(crop.clone());
    let chain = table.chain(&crop.id).unwrap();
    assert_eq!(
        chain.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(),
        [crop.id.as_str(), flat.id.as_str()]
    );
    assert_eq!(table.next_crop(), 2);
}

// --- the dependency order and reference substitution --------------------------------------------

#[test]
fn import_computes_pixel_digest_then_frame_then_image() {
    let (image, frame, _) =
        Image::import(&repo_root().join("fixtures/images/floor.png"), None).unwrap();
    // The frame's identity does not involve the image: the same size and projection anywhere
    // gives the same frame.
    assert_eq!(
        frame.identity,
        Frame::raw(320, 200, Projection::Flat).identity
    );
    // The pixel digest depends on the samples alone.
    let digest = rascal_core::image::pixel_digest(&image.pixels);
    assert_eq!(image.record.pixel_digest, digest.as_str());
    assert_eq!(
        digest,
        canonical::pixel_digest(320, 200, 3, image.pixels.as_bytes())
    );
    // The image's identity is the digest, the frame and the roles; a different role is a
    // different image over the same pixels.
    let working = Image::new("w", image.pixels.clone(), &frame, Role::Working, None);
    assert_eq!(working.record.pixel_digest, image.record.pixel_digest);
    assert_ne!(working.record.identity, image.record.identity);
    let rendering = Image::new(
        "r",
        image.pixels.clone(),
        &frame,
        Role::Rendering,
        Some(RenderingRole::MaskInput),
    );
    assert_ne!(rendering.record.identity, image.record.identity);
}

#[test]
fn a_changed_reference_changes_the_referencing_identity() {
    let frame = Frame::raw(320, 200, Projection::Flat);
    let other = Frame::raw(320, 201, Projection::Flat);
    let location = Location::bbox(&frame.id, [0.2, 0.2, 0.4, 0.5]).unwrap();
    // The location's identity substitutes the frame's identity for its id.
    assert_ne!(
        location.identity(&frame.identity),
        location.identity(&other.identity)
    );
    assert_eq!(
        location.identity(&frame.identity),
        location
            .with_frame("any-local-alias")
            .identity(&frame.identity)
    );
    // A mask's identity references its locations by identity: change the location, keep the
    // local name, and the mask changes.
    let core = image::GrayImage::from_pixel(320, 200, image::Luma([255]));
    let source_a = Source {
        locations: vec![location.identity(&frame.identity).as_str().to_string()],
        instances: vec![],
    };
    let source_b = Source {
        locations: vec![location.identity(&other.identity).as_str().to_string()],
        instances: vec![],
    };
    let a = Mask::from_core(
        "mask",
        &frame,
        UseCase::Custom,
        core.clone(),
        source_a.clone(),
    )
    .unwrap();
    let b = Mask::from_core("mask", &frame, UseCase::Custom, core.clone(), source_b).unwrap();
    let c = Mask::from_core("another-name", &frame, UseCase::Custom, core, source_a).unwrap();
    assert_ne!(a.record.identity, b.record.identity);
    assert_eq!(
        a.record.identity, c.record.identity,
        "the local id is not part of identity"
    );
}

// --- the committed vectors -------------------------------------------------------------------

#[test]
fn identity_vectors_reproduce() {
    let (image, frame, _) =
        Image::import(&repo_root().join("fixtures/images/floor.png"), None).unwrap();
    let pano = Frame::raw(4000, 2000, Projection::Equirectangular);
    let plan = Plan::new(&pano, 0.07).unwrap();
    let cut = Frame::cut(&pano, "img-1", &plan).unwrap();
    let crop = Frame::crop(&frame, 1, [64, 40, 128, 100]).unwrap();
    let location = Location::bbox(&frame.id, [0.2, 0.2, 0.4, 0.5]).unwrap();
    let mut core = image::GrayImage::new(320, 200);
    for y in 40..100 {
        for x in 64..128 {
            core.put_pixel(x, y, image::Luma([255]));
        }
    }
    let mask = Mask::from_core(
        "mask",
        &frame,
        UseCase::Custom,
        core,
        Source {
            locations: vec![location.identity(&frame.identity).as_str().to_string()],
            instances: vec![],
        },
    )
    .unwrap();
    let grey = Image::new(
        "g",
        DynamicImage::ImageLuma8(mask.core.clone()),
        &frame,
        Role::Rendering,
        Some(RenderingRole::MaskInput),
    );
    let computed = serde_json::json!({
        "frame raw:320x200:flat": frame.identity,
        "frame raw:4000x2000:equirectangular": pano.identity,
        "plan 4000x2000 strip 0.07": plan.identity,
        "frame cut of the panorama": cut.identity,
        "frame crop:1 [64,40,128,100] of raw:320x200:flat": crop.identity,
        "pixel digest floor.png": image.record.pixel_digest,
        "image canonical floor.png": image.record.identity,
        "location bbox 0.2,0.2,0.4,0.5 in raw:320x200:flat": location.identity(&frame.identity).as_str(),
        "mask rect of that bbox": mask.record.identity,
        "image grey rendering of that mask": grey.record.identity,
    });
    let path = repo_root().join("fixtures/identity/vectors.json");
    if std::env::var_os("UPDATE_VECTORS").is_some() {
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&computed).unwrap() + "\n",
        )
        .unwrap();
    }
    let committed: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        computed, committed,
        "fixtures/identity/vectors.json differs; UPDATE_VECTORS=1 rewrites it (an ADR 0016 change)"
    );
}
