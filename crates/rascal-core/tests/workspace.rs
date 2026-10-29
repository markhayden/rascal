//! Task 1.7a: the workspace commit protocol. The recording filesystem proves the spec's sync
//! order and that `committed` comes only after the final sync; concurrent commits produce no
//! torn generation; an op that fails after staging publishes nothing; abandoned staging
//! directories go and locked ones stay; a real sync runs on this platform.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::DynamicImage;
use rascal_core::artifact::Artifact;
use rascal_core::frame::{Frame, Projection};
use rascal_core::image::{Image, Role};
use rascal_core::location::Location;
use rascal_core::op::{Inputs, Params, Status};
use rascal_core::workspace::Workspace;
use rascal_core::workspace::sync::{Fs, RealFs};
use rascal_testkit::fs_shim::{Event, RecordingFs};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-ws-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn frame() -> Frame {
    Frame::raw(40, 30, Projection::Flat)
}

fn canonical(frame: &Frame) -> Image {
    Image::new(
        "i",
        DynamicImage::ImageRgb8(rascal_testkit::images::flat(frame.width, frame.height, 90)),
        frame,
        Role::Canonical,
        None,
    )
}

/// A workspace with the flat image imported, over a recording filesystem.
fn recorded(name: &str) -> (PathBuf, Arc<RecordingFs>, Workspace, String) {
    let dir = scratch(name);
    let fs = Arc::new(RecordingFs::new());
    let ws = Workspace::open_with(&dir, fs.clone()).unwrap();
    let f = frame();
    let (id, _) = ws.add_canonical(canonical(&f), &f, "flat").unwrap();
    fs.reset();
    (dir, fs, ws, id)
}

fn names(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .map(|e| {
            let path = e.path();
            let short = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            format!("{} {short}", e.kind())
        })
        .collect()
}

#[test]
fn the_commit_syncs_in_the_spec_order_and_commits_only_after_the_last_sync() {
    let (dir, fs, ws, image_id) = recorded("order");
    let registry = rascal_ops::registry().unwrap();
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("image".into(), ws.load(&image_id).unwrap());
    let mut params = Params::new();
    params.insert(
        "bbox".into(),
        toml::Value::Array(
            vec![0.1, 0.1, 0.5, 0.5]
                .into_iter()
                .map(toml::Value::Float)
                .collect(),
        ),
    );
    let invocation = ws
        .invoke("find", "rect", &registry, &inputs, &params)
        .unwrap();
    assert_eq!(invocation.output.envelope.status, Status::Ok);
    let committed = invocation.committed.unwrap();
    assert_eq!(committed.generation, 2);
    assert_eq!(committed.seq, 2);

    let events = fs.events();
    let n = names(&events);
    // Every write of the commit is followed by a sync of that file before any rename of it,
    // and every directory that gained an entry is synced before the generation is written.
    let index = |needle: &str| {
        n.iter()
            .position(|s| s == needle)
            .unwrap_or_else(|| panic!("{needle} not in {n:#?}"))
    };
    let record_write = index("write record.json");
    let record_sync = n
        .iter()
        .enumerate()
        .filter(|(_, s)| s.as_str() == "sync_file record.json")
        .map(|(i, _)| i)
        .max()
        .unwrap();
    let record_rename = events
        .iter()
        .position(|e| matches!(e, Event::Rename(_, to) if to.starts_with(dir.join("ops"))))
        .unwrap();
    let ops_sync = events
        .iter()
        .position(|e| matches!(e, Event::SyncDir(p) if p == &dir.join("ops")))
        .unwrap();
    let generation_write = index("write 2.json");
    let generation_sync = index("sync_file 2.json");
    let registry_sync = events
        .iter()
        .position(|e| matches!(e, Event::SyncDir(p) if p == &dir.join("registry")))
        .unwrap();
    let pointer_write = index("write CURRENT.tmp");
    let pointer_sync = index("sync_file CURRENT.tmp");
    let pointer_rename = events
        .iter()
        .position(|e| matches!(e, Event::Rename(_, to) if to == &dir.join("CURRENT")))
        .unwrap();
    let root_sync = events
        .iter()
        .rposition(|e| matches!(e, Event::SyncDir(p) if p == &dir))
        .unwrap();
    assert!(
        record_write < record_sync && record_sync < record_rename,
        "{n:#?}"
    );
    assert!(
        record_rename < ops_sync && ops_sync < generation_write,
        "{n:#?}"
    );
    assert!(
        generation_write < generation_sync && generation_sync < registry_sync,
        "{n:#?}"
    );
    assert!(
        registry_sync < pointer_write
            && pointer_write < pointer_sync
            && pointer_sync < pointer_rename,
        "{n:#?}"
    );
    assert!(pointer_rename < root_sync, "{n:#?}");
    // Nothing but the staging clean-up and the journal follows the root sync: committed is
    // reported only after the last sync.
    let after: Vec<&str> = n[root_sync + 1..].iter().map(String::as_str).collect();
    assert!(
        after
            .iter()
            .all(|s| s.starts_with("remove_dir_all") || s.starts_with("append journal")),
        "{after:?}"
    );
    // The generation is what the pointer says, and it lists the location.
    let generation = ws.snapshot().unwrap();
    assert_eq!(generation.n, 2);
    assert_eq!(generation.locations.len(), 1);
    assert_eq!(generation.records.len(), 2);
    assert_eq!(invocation.ids["location"], "loc-2");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_crash_after_staging_publishes_nothing() {
    // Find how many events a commit takes, then crash at every point before the pointer rename
    // and check that nothing is published; crash after it and check the generation is whole.
    let (dir, fs, ws, image_id) = recorded("crash-count");
    let registry = rascal_ops::registry().unwrap();
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("image".into(), ws.load(&image_id).unwrap());
    let mut params = Params::new();
    params.insert(
        "bbox".into(),
        toml::Value::Array(
            vec![0.1, 0.1, 0.5, 0.5]
                .into_iter()
                .map(toml::Value::Float)
                .collect(),
        ),
    );
    ws.invoke("find", "rect", &registry, &inputs, &params)
        .unwrap();
    let events = fs.events();
    let pointer_rename = events
        .iter()
        .position(|e| matches!(e, Event::Rename(_, to) if to == &dir.join("CURRENT")))
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);

    for crash_at in [1, 3, pointer_rename - 2, pointer_rename] {
        let dir = scratch(&format!("crash-{crash_at}"));
        let fs = Arc::new(RecordingFs::new());
        let ws = Workspace::open_with(&dir, fs.clone()).unwrap();
        let f = frame();
        let (image_id, _) = ws.add_canonical(canonical(&f), &f, "flat").unwrap();
        drop(ws);
        let before = Workspace::open(&dir).unwrap().snapshot().unwrap();
        let fs = Arc::new(RecordingFs::crashing_at(crash_at));
        let ws = Workspace::open_with(&dir, fs.clone()).unwrap();
        let mut inputs: Inputs = BTreeMap::new();
        inputs.insert("image".into(), ws.load(&image_id).unwrap());
        let result = ws.invoke("find", "rect", &registry, &inputs, &params);
        assert!(
            result.is_err(),
            "crash at {crash_at} should fail the invocation"
        );
        assert!(fs.crashed());
        drop(ws);
        // Reopen with the real filesystem, as after a restart.
        let after = Workspace::open(&dir).unwrap().snapshot().unwrap();
        assert_eq!(
            after,
            before,
            "crash at {crash_at}: the generation must be the previous one; CURRENT = {:?}; events: {:#?}",
            std::fs::read_to_string(dir.join("CURRENT")),
            names(&fs.events())
        );
        assert!(after.locations.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
    // Crashing between the pointer rename and the root sync: the rename was never made durable,
    // so the shim drops it and the previous generation is what a restart sees; the orphaned
    // files under ops/ and artifacts/ are harmless.
    let dir = scratch("crash-before-root-sync");
    let fs = Arc::new(RecordingFs::new());
    let ws = Workspace::open_with(&dir, fs.clone()).unwrap();
    let f = frame();
    let (image_id, _) = ws.add_canonical(canonical(&f), &f, "flat").unwrap();
    drop(ws);
    let before = Workspace::open(&dir).unwrap().snapshot().unwrap();
    let fs = Arc::new(RecordingFs::crashing_at(pointer_rename + 1));
    let ws = Workspace::open_with(&dir, fs.clone()).unwrap();
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("image".into(), ws.load(&image_id).unwrap());
    let result = ws.invoke("find", "rect", &registry, &inputs, &params);
    assert!(result.is_err() && fs.crashed());
    drop(ws);
    let after = Workspace::open(&dir).unwrap().snapshot().unwrap();
    assert_eq!(after, before);
    let _ = std::fs::remove_dir_all(&dir);
    // After the root sync the generation is complete and durable: a crash that only stops the
    // staging clean-up does not fail the invocation, and the record and the journal agree.
    let dir = scratch("crash-after-root-sync");
    let fs = Arc::new(RecordingFs::new());
    let ws = Workspace::open_with(&dir, fs.clone()).unwrap();
    let f = frame();
    let (image_id, _) = ws.add_canonical(canonical(&f), &f, "flat").unwrap();
    drop(ws);
    let fs = Arc::new(RecordingFs::crashing_at(pointer_rename + 2));
    let ws = Workspace::open_with(&dir, fs.clone()).unwrap();
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("image".into(), ws.load(&image_id).unwrap());
    let invocation = ws
        .invoke("find", "rect", &registry, &inputs, &params)
        .unwrap();
    assert!(fs.crashed());
    assert_eq!(invocation.committed.unwrap().generation, 2);
    drop(ws);
    let after = Workspace::open(&dir).unwrap();
    let generation = after.snapshot().unwrap();
    assert_eq!(generation.n, 2);
    assert_eq!(generation.locations.len(), 1);
    for identity in &generation.records {
        after.read_record(identity).unwrap();
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn concurrent_commits_produce_no_torn_generation() {
    let dir = scratch("concurrent");
    let ws = Workspace::open(&dir).unwrap();
    let f = frame();
    ws.add_canonical(canonical(&f), &f, "flat").unwrap();
    drop(ws);
    let threads: Vec<_> = (0..4)
        .map(|t| {
            let dir = dir.clone();
            std::thread::spawn(move || {
                let ws = Workspace::open(&dir).unwrap();
                for i in 0..25 {
                    let x0 = 0.01 * (t * 25 + i) as f64;
                    let location =
                        Location::bbox("raw:40x30:flat", [x0 * 0.5, 0.1, x0 * 0.5 + 0.2, 0.5])
                            .unwrap();
                    ws.add_location(location).unwrap();
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    let ws = Workspace::open(&dir).unwrap();
    let generation = ws.snapshot().unwrap();
    assert_eq!(generation.n, 101, "one generation per commit");
    assert_eq!(generation.locations.len(), 100);
    assert_eq!(generation.records.len(), 101);
    let mut ids: Vec<&str> = generation.locations.iter().map(|l| l.id.as_str()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 100, "every location has its own id");
    // Every generation on disk is whole and consecutive.
    for n in 1..=101 {
        let g = ws.read_generation(n).unwrap();
        assert_eq!(g.n, n);
        assert_eq!(g.records.len() as u64, n);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_abandoned_staging_directory_is_removed_and_a_locked_one_kept() {
    let dir = scratch("staging");
    {
        let ws = Workspace::open(&dir).unwrap();
        drop(ws);
    }
    let abandoned = dir.join("staging/old-token");
    std::fs::create_dir_all(&abandoned).unwrap();
    std::fs::write(abandoned.join("lock"), "").unwrap();
    std::fs::write(abandoned.join("leftover.png"), "x").unwrap();
    // The sweep leaves a directory younger than a few seconds alone (its owner may not have
    // locked it yet), so the abandoned one is aged.
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
    std::fs::File::open(&abandoned)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let held = dir.join("staging/live-token");
    std::fs::create_dir_all(&held).unwrap();
    let lock = std::fs::File::create(held.join("lock")).unwrap();
    lock.lock().unwrap();
    let _ws = Workspace::open(&dir).unwrap();
    assert!(!abandoned.exists(), "abandoned staging is removed on open");
    assert!(
        held.exists(),
        "a locked staging directory is active and left alone"
    );
    lock.unlock().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_generation_naming_a_missing_artifact_refuses_the_workspace() {
    let dir = scratch("missing");
    let ws = Workspace::open(&dir).unwrap();
    let f = frame();
    ws.add_canonical(canonical(&f), &f, "flat").unwrap();
    let entry = ws.snapshot().unwrap().artifacts[0].clone();
    drop(ws);
    std::fs::remove_file(dir.join(&entry.path)).unwrap();
    let e = Workspace::open(&dir).unwrap_err().to_string();
    assert!(
        e.contains("is missing") && e.contains("workspace repair"),
        "{e}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_real_filesystem_syncs_on_this_platform() {
    let dir = scratch("sync");
    let fs = RealFs;
    let file = dir.join("a.txt");
    fs.write(&file, b"hello").unwrap();
    fs.sync_file(&file).unwrap();
    fs.sync_dir(&dir).unwrap();
    fs.rename(&file, &dir.join("b.txt")).unwrap();
    fs.sync_dir(&dir).unwrap();
    assert_eq!(std::fs::read_to_string(dir.join("b.txt")).unwrap(), "hello");
    assert!(fs.sync_file(Path::new("/no/such/file")).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_same_artifact_committed_twice_keeps_one_copy_and_one_id() {
    let dir = scratch("dedupe");
    let ws = Workspace::open(&dir).unwrap();
    let f = frame();
    let (a, _) = ws.add_canonical(canonical(&f), &f, "flat").unwrap();
    let (b, _) = ws.add_canonical(canonical(&f), &f, "flat-again").unwrap();
    assert_eq!(a, b, "an identical identity keeps its id");
    let generation = ws.snapshot().unwrap();
    assert_eq!(generation.artifacts.len(), 1);
    assert_eq!(generation.records.len(), 2, "both imports are recorded");
    assert_eq!(
        std::fs::read_dir(dir.join("images")).unwrap().count(),
        2,
        "one PNG and its record"
    );
    let loaded = ws.load(&a).unwrap();
    assert!(matches!(loaded, Artifact::Image(_)));
    let _ = std::fs::remove_dir_all(&dir);
}
