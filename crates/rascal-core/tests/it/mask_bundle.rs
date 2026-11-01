//! Task 10.5: a Mask leaves one workspace as a bundle and enters another with the same
//! identity; a tampered bundle, a PNG or any other file is refused whole and nothing is
//! published; reads are bounded; an import of a known identity is idempotent (ADR 0022).
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use image::DynamicImage;
use rascal_core::artifact::Artifact;
use rascal_core::bundle;
use rascal_core::frame::{Frame, Projection};
use rascal_core::image::{Image, Role};
use rascal_core::op::{Inputs, Params};
use rascal_core::workspace::Workspace;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-bundle-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs one shipped op and returns its output ids by port.
fn step(
    ws: &Workspace,
    op: &str,
    inputs: &[(&str, &str)],
    params: &[(&str, toml::Value)],
) -> BTreeMap<String, String> {
    let registry = rascal_ops::registry().unwrap();
    let mut given: Inputs = BTreeMap::new();
    for (port, id) in inputs {
        given.insert((*port).to_string(), ws.load(id).unwrap());
    }
    let mut p = Params::new();
    for (k, v) in params {
        p.insert((*k).to_string(), v.clone());
    }
    let done = ws.invoke(op, op, &registry, &given, &p).unwrap();
    assert!(done.committed.is_some(), "{op}: {:?}", done.output.envelope);
    done.ids
}

/// A workspace holding the seam steps on a small panorama: a band mask with a profile and a
/// tone ramp in a `cut:` frame, and its window in a `crop:` frame of that. Returns the
/// workspace, the seam mask id and the windowed mask id.
fn seam_workspace(dir: &Path) -> (Workspace, String, String) {
    let ws = Workspace::open(dir).unwrap();
    let raw = Frame::raw(256, 128, Projection::Equirectangular);
    let image = Image::new(
        "pano",
        DynamicImage::ImageRgb8(rascal_testkit::images::step_pano(256, 128)),
        &raw,
        Role::Canonical,
        None,
    );
    let (img, _) = ws.add_canonical(image, &raw, "pano").unwrap();
    let cut = step(
        &ws,
        "cut",
        &[("image", &img)],
        &[("strip", toml::Value::Float(0.25))],
    );
    let canvas = cut["image"].clone();
    let line = step(
        &ws,
        "line",
        &[("image", &canvas)],
        &[
            ("column", toml::Value::Float(0.5)),
            ("width_px", toml::Value::Float(2.0)),
        ],
    );
    let fan = step(
        &ws,
        "fan",
        &[("image", &canvas), ("line", &line["location"])],
        &[],
    );
    let band = step(&ws, "profile", &[("band", &fan["band"])], &[]);
    let ramp = step(
        &ws,
        "ramp",
        &[("image", &canvas), ("mask", &band["mask"])],
        &[],
    );
    let window = step(
        &ws,
        "window",
        &[("image", &canvas), ("mask", &ramp["mask"])],
        &[],
    );
    (ws, ramp["mask"].clone(), window["mask"].clone())
}

fn mask_of(ws: &Workspace, id: &str) -> rascal_core::mask::Mask {
    match ws.load(id).unwrap() {
        Artifact::Mask(m) => (*m).clone(),
        other => panic!("{id} is a {}", other.artifact_type()),
    }
}

/// Exports a mask to bytes.
fn export(ws: &Workspace, id: &str) -> Vec<u8> {
    ws.export_mask(id).unwrap().bytes
}

fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

/// The import is refused, the error says `says`, and the workspace is as it was.
fn refused(ws: &Workspace, path: &Path, says: &str) {
    let generation = ws.read_current().unwrap();
    let artifacts = std::fs::read_dir(ws.root.join("artifacts"))
        .unwrap()
        .count();
    let e = ws.import_mask(path, None).unwrap_err();
    let text = e.to_string();
    assert!(text.contains(says), "{text}\nshould say: {says}");
    assert!(
        matches!(e, rascal_core::Error::Usage { .. }),
        "a refusal is a usage error: {e:?}"
    );
    assert!(text.contains("nothing was imported"), "{text}");
    assert_eq!(
        ws.read_current().unwrap(),
        generation,
        "no generation published"
    );
    assert_eq!(
        std::fs::read_dir(ws.root.join("artifacts"))
            .unwrap()
            .count(),
        artifacts,
        "no artifact left behind"
    );
}

/// The `(path, start, size)` of each member's content inside the archive.
fn members(bytes: &[u8]) -> Vec<(String, u64, u64)> {
    let mut archive = tar::Archive::new(bytes);
    archive
        .entries()
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (
                e.path().unwrap().display().to_string(),
                e.raw_file_position(),
                e.header().size().unwrap(),
            )
        })
        .collect()
}

/// A canonical archive of these members, for building hostile bundles.
fn pack(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for (path, bytes) in entries {
        let mut header = tar::Header::new_ustar();
        header.set_path(path).unwrap();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_cksum();
        builder.append(&header, *bytes).unwrap();
    }
    builder.into_inner().unwrap()
}

fn unpack(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut archive = tar::Archive::new(bytes);
    archive
        .entries()
        .unwrap()
        .map(|e| {
            let mut e = e.unwrap();
            let path = e.path().unwrap().display().to_string();
            let mut data = Vec::new();
            std::io::Read::read_to_end(&mut e, &mut data).unwrap();
            (path, data)
        })
        .collect()
}

fn repack(members: &[(String, Vec<u8>)]) -> Vec<u8> {
    let refs: Vec<(&str, &[u8])> = members
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    pack(&refs)
}

/// The manifest edited and the bundle rebuilt around it, member digests left alone.
fn with_manifest(bytes: &[u8], edit: impl FnOnce(&mut bundle::MaskBundleManifest)) -> Vec<u8> {
    let mut members = unpack(bytes);
    let mut manifest: bundle::MaskBundleManifest = serde_json::from_slice(&members[0].1).unwrap();
    edit(&mut manifest);
    let mut json = serde_json::to_vec_pretty(&manifest).unwrap();
    json.push(b'\n');
    members[0].1 = json;
    repack(&members)
}

#[test]
fn export_then_import_keeps_the_identity_the_frames_and_the_bytes() {
    let dir = scratch("roundtrip");
    let (a, seam, windowed) = seam_workspace(&dir.join("a"));
    let b = Workspace::open(&dir.join("b")).unwrap();
    for id in [&seam, &windowed] {
        let original = mask_of(&a, id);
        assert!(original.profile.is_some() || id == &windowed);
        let exported = a.export_mask(id).unwrap();
        assert_eq!(exported.identity, original.record.identity);
        assert_eq!(exported.sha256, bundle::sha256(&exported.bytes));
        let path = write(&dir, "m.mask.tar", &exported.bytes);
        let imported = b.import_mask(&path, Some(&exported.sha256)).unwrap();
        assert!(!imported.already_present);
        assert_eq!(imported.identity, original.record.identity);
        assert_eq!(imported.bundle_sha256, exported.sha256);
        let copy = mask_of(&b, &imported.id);
        assert_eq!(copy.record.identity, original.record.identity);
        assert_eq!(copy.core, original.core);
        assert_eq!(copy.blend, original.blend);
        assert_eq!(copy.protect, original.protect);
        assert_eq!(copy.profile, original.profile);
        assert_eq!(copy.prescriptions, original.prescriptions);
        // The whole chain is registered with the same identities.
        let chain = |ws: &Workspace, frame: &str| -> Vec<String> {
            ws.frames()
                .unwrap()
                .chain(frame)
                .unwrap()
                .iter()
                .map(|f| f.identity.clone())
                .collect()
        };
        assert_eq!(
            chain(&b, &copy.record.frame),
            chain(&a, &original.record.frame)
        );
        // Exporting the copy gives the same bytes.
        assert_eq!(b.export_mask(&imported.id).unwrap().bytes, exported.bytes);
    }
    assert_eq!(
        b.frames()
            .unwrap()
            .chain(&mask_of(&a, &windowed).record.frame)
            .unwrap()
            .len(),
        3,
        "raw, cut and crop"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_same_mask_gives_the_same_bundle_and_a_stable_layout() {
    let dir = scratch("determinism");
    let (a, seam, _) = seam_workspace(&dir.join("a"));
    let one = a.export_mask(&seam).unwrap();
    let two = a.export_mask(&seam).unwrap();
    assert_eq!(one.bytes, two.bytes);
    assert_eq!(one.sha256, two.sha256);
    // A second workspace that ran the same steps exports the same bytes.
    let (other, seam2, _) = seam_workspace(&dir.join("other"));
    assert_eq!(other.export_mask(&seam2).unwrap().bytes, one.bytes);
    let layout = members(&one.bytes);
    let names: Vec<&str> = layout.iter().map(|(p, _, _)| p.as_str()).collect();
    assert_eq!(names[0], "bundle.json");
    assert!(names.contains(&"mask/mask.toml") && names.contains(&"mask/core.png"));
    let rest = &names[1..];
    let mut sorted = rest.to_vec();
    sorted.sort();
    assert_eq!(rest, sorted.as_slice(), "members sorted by name");
    let mut archive = tar::Archive::new(one.bytes.as_slice());
    for e in archive.entries().unwrap() {
        let e = e.unwrap();
        let h = e.header();
        assert_eq!(h.mode().unwrap(), 0o644);
        assert_eq!(h.mtime().unwrap(), 0);
        assert_eq!(h.uid().unwrap(), 0);
        assert_eq!(h.entry_type(), tar::EntryType::Regular);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn any_flipped_byte_of_any_member_is_refused() {
    let dir = scratch("flip");
    let (a, seam, _) = seam_workspace(&dir.join("a"));
    let b = Workspace::open(&dir.join("b")).unwrap();
    let bytes = export(&a, &seam);
    for (path, start, size) in members(&bytes) {
        // Every byte of a small member; a spread of offsets in a large one.
        let step = (size / 64).max(1);
        let mut offset = 0;
        while offset < size {
            let mut tampered = bytes.clone();
            tampered[(start + offset) as usize] ^= 0x01;
            let file = write(&dir, "t.mask.tar", &tampered);
            let generation = b.read_current().unwrap();
            let e = b.import_mask(&file, None);
            assert!(e.is_err(), "{path} byte {offset} flipped was accepted");
            assert_eq!(b.read_current().unwrap(), generation);
            offset += step;
        }
    }
    // A member's flip names the member.
    let (path, start, _) = members(&bytes)
        .into_iter()
        .find(|(p, _, _)| p == "mask/core.png")
        .unwrap();
    let mut tampered = bytes.clone();
    tampered[start as usize + 40] ^= 0x01;
    refused(
        &b,
        &write(&dir, "t.mask.tar", &tampered),
        &format!("member {path}: sha256"),
    );
    // A flipped byte outside every member (a header, padding, the end) is refused too.
    let mut tampered = bytes.clone();
    let last = tampered.len() - 1;
    tampered[last] = 1;
    refused(&b, &write(&dir, "t.mask.tar", &tampered), "canonical form");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_extra_renamed_and_truncated_members_are_refused() {
    let dir = scratch("members");
    let (a, seam, _) = seam_workspace(&dir.join("a"));
    let b = Workspace::open(&dir.join("b")).unwrap();
    let bytes = export(&a, &seam);
    let all = unpack(&bytes);

    let mut missing = all.clone();
    missing.retain(|(p, _)| p != "mask/blend.png");
    refused(
        &b,
        &write(&dir, "t.tar", &repack(&missing)),
        "member mask/blend.png is in bundle.json but not in the archive",
    );

    let mut extra = all.clone();
    extra.push(("mask/zz.png".to_string(), b"hello".to_vec()));
    refused(
        &b,
        &write(&dir, "t.tar", &repack(&extra)),
        "member mask/zz.png is in the archive but not in bundle.json",
    );

    // An extra member listed in the manifest too is still not a file the record names.
    let extra_listed = with_manifest(&repack(&extra), |m| {
        m.members.push(bundle::MaskBundleMember {
            path: "mask/zz.png".to_string(),
            size: 5,
            sha256: bundle::sha256(b"hello"),
        });
    });
    refused(
        &b,
        &write(&dir, "t.tar", &extra_listed),
        "mask/zz.png is not a file mask.toml names",
    );

    let mut renamed = all.clone();
    for (p, _) in renamed.iter_mut() {
        if p == "mask/protect.png" {
            *p = "mask/protect2.png".to_string();
        }
    }
    refused(
        &b,
        &write(&dir, "t.tar", &repack(&renamed)),
        "mask/protect.png",
    );

    // A name that leaves the directory (the tar crate will not write one, so it is patched in).
    let mut escaping = all.clone();
    escaping.push(("mask/ev".to_string(), b"x".to_vec()));
    let mut escaping = repack(&escaping);
    let (_, start, _) = members(&escaping)
        .into_iter()
        .find(|(p, _, _)| p == "mask/ev")
        .unwrap();
    let at = start as usize - 512;
    let mut header = tar::Header::new_ustar();
    header
        .as_mut_bytes()
        .copy_from_slice(&escaping[at..at + 512]);
    header.as_mut_bytes()[..7].copy_from_slice(b"../evil");
    header.set_cksum();
    escaping[at..at + 512].copy_from_slice(header.as_bytes());
    refused(
        &b,
        &write(&dir, "t.tar", &escaping),
        "member ../evil is not mask/",
    );

    let truncated = &bytes[..bytes.len() / 2];
    refused(&b, &write(&dir, "t.tar", truncated), "truncated");

    let mut after = bytes.clone();
    after.extend_from_slice(&[0u8; 512]);
    after.extend_from_slice(b"trailing");
    refused(&b, &write(&dir, "t.tar", &after), "canonical form");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_wrong_frame_chain_or_identity_is_refused() {
    let dir = scratch("chain");
    let (a, _, windowed) = seam_workspace(&dir.join("a"));
    let b = Workspace::open(&dir.join("b")).unwrap();
    let bytes = export(&a, &windowed);
    let cases: Vec<(&str, Vec<u8>)> = vec![
        (
            "its fields make",
            with_manifest(&bytes, |m| {
                m.frames[1].identity = m.frames[0].identity.clone()
            }),
        ),
        (
            "its parent is",
            with_manifest(&bytes, |m| {
                m.frames[2].parent = Some("raw:9x9:flat".to_string())
            }),
        ),
        (
            "not at a raw frame",
            with_manifest(&bytes, |m| {
                m.frames.remove(0);
            }),
        ),
        (
            "but the chain ends at",
            with_manifest(&bytes, |m| {
                m.frames.pop();
            }),
        ),
        (
            "the frame chain is empty",
            with_manifest(&bytes, |m| m.frames.clear()),
        ),
        (
            "bundle.json names identity",
            with_manifest(&bytes, |m| m.identity = m.frames[0].identity.clone()),
        ),
        (
            "mask_bundle_version 2",
            with_manifest(&bytes, |m| m.mask_bundle_version = 2),
        ),
    ];
    for (says, tampered) in cases {
        refused(&b, &write(&dir, "t.tar", &tampered), says);
    }
    // A manifest that parses but is not in its canonical form.
    let mut members = unpack(&bytes);
    let value: serde_json::Value = serde_json::from_slice(&members[0].1).unwrap();
    members[0].1 = serde_json::to_vec(&value).unwrap();
    refused(
        &b,
        &write(&dir, "t.tar", &repack(&members)),
        "not in its canonical form",
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_png_or_any_file_that_is_not_a_bundle_is_never_taken_as_a_mask() {
    let dir = scratch("notabundle");
    let (a, seam, _) = seam_workspace(&dir.join("a"));
    let b = Workspace::open(&dir.join("b")).unwrap();
    let mask_dir = a
        .root
        .join(a.snapshot().unwrap().artifact(&seam).unwrap().path.clone());
    let png = mask_dir.join("core.png");
    refused(&b, &png, "it is a PNG image");
    let jpeg = write(&dir, "x.jpg", &[0xff, 0xd8, 0xff, 0xe0, 0, 0x10]);
    refused(&b, &jpeg, "it is a JPEG image");
    refused(&b, &mask_dir, "it is a directory");
    refused(&b, &write(&dir, "empty", b""), "the file is empty");
    refused(
        &b,
        &write(&dir, "x.json", b"{\"mask\": true}"),
        "not a tar archive",
    );
    let other = pack(&[("README", b"hello")]);
    refused(
        &b,
        &write(&dir, "other.tar", &other),
        "its first member is README, not bundle.json",
    );
    let gz = write(&dir, "x.tar.gz", &[0x1f, 0x8b, 8, 0]);
    refused(&b, &gz, "gzip");
    // Exporting something other than a Mask is refused naming its type.
    let image = a
        .snapshot()
        .unwrap()
        .artifacts
        .iter()
        .find(|e| e.artifact_type == rascal_core::artifact::ArtifactType::Image)
        .unwrap()
        .id
        .clone();
    let e = a.export_mask(&image).unwrap_err().to_string();
    assert!(
        e.contains("is an Image") && e.contains("only a Mask"),
        "{e}"
    );
    let location = a.snapshot().unwrap().locations[0].id.clone();
    let e = a.export_mask(&location).unwrap_err().to_string();
    assert!(e.contains("is a Location"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hostile_input_is_bounded() {
    let dir = scratch("bounded");
    let b = Workspace::open(&dir.join("b")).unwrap();
    // A file over the limit is refused from its size, before a byte is read (sparse here).
    let big = dir.join("big.tar");
    std::fs::File::create(&big)
        .unwrap()
        .set_len(bundle::MAX_BUNDLE_BYTES + 1)
        .unwrap();
    refused(&b, &big, "over the limit");
    // A header claiming more bytes than remain.
    let mut lying = pack(&[("bundle.json", b"{}")]);
    let mut header = tar::Header::new_ustar();
    header.as_mut_bytes().copy_from_slice(&lying[..512]);
    header.set_size(1 << 40);
    header.set_cksum();
    lying[..512].copy_from_slice(header.as_bytes());
    refused(
        &b,
        &write(&dir, "lying.tar", &lying),
        "claims 1099511627776 bytes",
    );
    // A manifest over its limit.
    let huge = vec![b' '; bundle::MAX_MANIFEST_BYTES as usize + 1];
    refused(
        &b,
        &write(&dir, "huge.tar", &pack(&[("bundle.json", &huge)])),
        "bundle.json is 1048577 bytes",
    );
    // Too many members.
    let names: Vec<String> = (0..bundle::MAX_MEMBERS)
        .map(|i| format!("mask/f{i:02}"))
        .collect();
    let mut entries: Vec<(&str, &[u8])> = vec![("bundle.json", b"{}")];
    for n in &names {
        entries.push((n.as_str(), b"x"));
    }
    refused(
        &b,
        &write(&dir, "many.tar", &pack(&entries)),
        "more than 16 members",
    );
    // A member name that is not plain or too long.
    let long = format!("mask/{}", "a".repeat(bundle::MAX_NAME_BYTES + 1));
    refused(
        &b,
        &write(
            &dir,
            "long.tar",
            &pack(&[("bundle.json", b"{}"), (&long, b"x")]),
        ),
        "a plain name",
    );
    refused(
        &b,
        &write(
            &dir,
            "dot.tar",
            &pack(&[("bundle.json", b"{}"), ("mask/.hidden", b"x")]),
        ),
        "a plain name",
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_known_identity_is_idempotent_and_a_taken_crop_id_moves() {
    let dir = scratch("idempotent");
    let (a, seam, windowed) = seam_workspace(&dir.join("a"));
    let path = write(&dir, "w.mask.tar", &export(&a, &windowed));
    // Into its own workspace: nothing published, the existing id.
    let before = a.read_current().unwrap();
    let again = a.import_mask(&path, None).unwrap();
    assert!(again.already_present);
    assert_eq!(again.id, windowed);
    assert_eq!(a.read_current().unwrap(), before);

    // Into a workspace whose crop:1 is another frame: the crop takes a free number.
    let original = mask_of(&a, &windowed);
    assert_eq!(original.record.frame, "crop:1");
    let c = Workspace::open(&dir.join("c")).unwrap();
    let flat = Frame::raw(40, 30, Projection::Flat);
    let image = Image::new(
        "flat",
        DynamicImage::ImageRgb8(rascal_testkit::images::flat(40, 30, 50)),
        &flat,
        Role::Canonical,
        None,
    );
    let (img, _) = c.add_canonical(image, &flat, "flat").unwrap();
    let loc = step(
        &c,
        "rect",
        &[("image", &img)],
        &[(
            "bbox",
            toml::Value::Array(
                [0.2, 0.2, 0.6, 0.6]
                    .into_iter()
                    .map(toml::Value::Float)
                    .collect(),
            ),
        )],
    );
    let mask = step(
        &c,
        "rasterize",
        &[("location", &loc["location"]), ("image", &img)],
        &[],
    );
    step(
        &c,
        "window",
        &[("image", &img), ("mask", &mask["mask"])],
        &[],
    );
    assert!(c.frames().unwrap().get("crop:1").is_some());
    let imported = c.import_mask(&path, None).unwrap();
    assert_eq!(imported.identity, original.record.identity);
    assert_eq!(imported.frame, "crop:2");
    let copy = mask_of(&c, &imported.id);
    assert_eq!(copy.record.frame, "crop:2");
    assert_eq!(copy.record.frame_identity, original.record.frame_identity);
    // The seam mask's cut frame and its parent arrive with it, under their own ids.
    let seam_path = write(&dir, "s.mask.tar", &export(&a, &seam));
    let s = c.import_mask(&seam_path, None).unwrap();
    assert!(s.already_present || s.frame.starts_with("cut:"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_digest_given_must_match() {
    let dir = scratch("digest");
    let (a, seam, _) = seam_workspace(&dir.join("a"));
    let b = Workspace::open(&dir.join("b")).unwrap();
    let exported = a.export_mask(&seam).unwrap();
    let path = write(&dir, "m.mask.tar", &exported.bytes);
    let wrong = format!("sha256:{}", "0".repeat(64));
    let generation = b.read_current().unwrap();
    let e = b.import_mask(&path, Some(&wrong)).unwrap_err().to_string();
    assert!(
        e.contains("the bundle's sha256 is") && e.contains("nothing was imported"),
        "{e}"
    );
    assert_eq!(b.read_current().unwrap(), generation);
    // The bare hex is accepted as well as the prefixed form.
    let bare = exported.sha256.trim_start_matches("sha256:").to_string();
    b.import_mask(&path, Some(&bare)).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// The chain of a mask's frame, root first, as `export_mask` hands it to the writer.
fn chain_of(ws: &Workspace, mask: &rascal_core::mask::Mask) -> Vec<Frame> {
    let mut chain: Vec<Frame> = ws
        .frames()
        .unwrap()
        .chain(&mask.record.frame)
        .unwrap()
        .into_iter()
        .cloned()
        .collect();
    chain.reverse();
    chain
}

/// The writer refuses, naming `says`, and says nothing was written.
fn not_written(result: Result<Vec<u8>, rascal_core::Error>, says: &[&str]) {
    let e = result.unwrap_err();
    assert!(
        matches!(e, rascal_core::Error::Usage { .. }),
        "a refusal is a usage error: {e:?}"
    );
    let text = e.to_string();
    for s in says {
        assert!(text.contains(s), "{text}\nshould say: {s}");
    }
    assert!(text.contains("nothing was written"), "{text}");
}

#[test]
fn export_never_writes_a_bundle_import_would_refuse() {
    let dir = scratch("writelimits");
    let (a, _, windowed) = seam_workspace(&dir.join("a"));
    let mask = mask_of(&a, &windowed);
    let chain = chain_of(&a, &mask);
    let full = bundle::write(&mask, &chain).unwrap();
    assert_eq!(
        bundle::Limits::READ,
        bundle::Limits {
            bundle_bytes: bundle::MAX_BUNDLE_BYTES,
            members: bundle::MAX_MEMBERS,
            manifest_bytes: bundle::MAX_MANIFEST_BYTES,
            name_bytes: bundle::MAX_NAME_BYTES,
            frames: bundle::MAX_FRAMES,
        },
        "the writer's limits are the reader's"
    );
    let size = full.len() as u64;
    // At the limit the bundle is written, byte for byte the same.
    let at = bundle::Limits {
        bundle_bytes: size,
        ..bundle::Limits::READ
    };
    assert_eq!(bundle::write_within(&mask, &chain, &at).unwrap(), full);
    // A byte over: refused, naming the size and the limit.
    let under = bundle::Limits {
        bundle_bytes: size - 1,
        ..bundle::Limits::READ
    };
    not_written(
        bundle::write_within(&mask, &chain, &under),
        &[
            &format!("the bundle would be {size} bytes"),
            &format!("over the limit of {} bytes", size - 1),
        ],
    );
    let layout = members(&full);
    not_written(
        bundle::write_within(
            &mask,
            &chain,
            &bundle::Limits {
                members: layout.len() - 1,
                ..bundle::Limits::READ
            },
        ),
        &[
            &format!("{} members", layout.len()),
            &format!("limit of {}", layout.len() - 1),
        ],
    );
    let manifest = layout[0].2;
    not_written(
        bundle::write_within(
            &mask,
            &chain,
            &bundle::Limits {
                manifest_bytes: manifest - 1,
                ..bundle::Limits::READ
            },
        ),
        &[
            &format!("bundle.json would be {manifest} bytes"),
            &format!("limit of {}", manifest - 1),
        ],
    );
    not_written(
        bundle::write_within(
            &mask,
            &chain,
            &bundle::Limits {
                name_bytes: 8,
                ..bundle::Limits::READ
            },
        ),
        &["member mask/", "over the limit of 8"],
    );
    not_written(
        bundle::write_within(
            &mask,
            &chain,
            &bundle::Limits {
                frames: chain.len() - 1,
                ..bundle::Limits::READ
            },
        ),
        &[
            &format!("{} frames", chain.len()),
            &format!("limit of {}", chain.len() - 1),
        ],
    );
    let _ = std::fs::remove_dir_all(&dir);
}
