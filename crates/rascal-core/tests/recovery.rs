//! Task 1.7b: crash at each commit boundary yields nothing or a complete generation and repair
//! rebuilds the journal to the same identities as an uninterrupted run; cancellation before
//! and after the writer lock; a missing payload refuses the workspace until repair; orphans are
//! collected; the rebuilt journal says it was rebuilt.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use image::DynamicImage;
use rascal_core::frame::{Frame, Projection};
use rascal_core::image::{Image, Role};
use rascal_core::journal::{Event, Journal};
use rascal_core::op::{Inputs, Params};
use rascal_core::workspace::Workspace;
use rascal_testkit::fs_shim::{Event as FsEvent, RecordingFs};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-rec-{name}-{}", std::process::id()));
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

fn rect_params() -> Params {
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
    params
}

/// A workspace with the flat image imported, and a rect run on it with a recording filesystem.
/// Returns the directory, the recorded events of the rect commit and the image id.
fn rect_events() -> (PathBuf, Vec<FsEvent>, String) {
    let dir = scratch("events");
    let fs = Arc::new(RecordingFs::new());
    let ws = Workspace::open_with(&dir, fs.clone()).unwrap();
    let f = frame();
    let (image_id, _) = ws.add_canonical(canonical(&f), &f, "flat").unwrap();
    fs.reset();
    let registry = rascal_ops::registry().unwrap();
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("image".into(), ws.load(&image_id).unwrap());
    ws.invoke("find", "rect", &registry, &inputs, &rect_params())
        .unwrap();
    let events = fs.events();
    drop(ws);
    (dir, events, image_id)
}

/// The semantic content of a journal: step, op, status, inputs and outputs, no observations.
/// Step, op, status, input identities, output identities.
type Line = (
    String,
    String,
    String,
    BTreeMap<String, String>,
    BTreeMap<String, String>,
);

fn semantic(dir: &std::path::Path) -> Vec<Line> {
    Journal::read(&dir.join("journal.jsonl"))
        .unwrap()
        .into_iter()
        .filter(|l| l.event == Event::Op)
        .map(|l| {
            (
                l.step.unwrap_or_default(),
                l.op.unwrap_or_default(),
                serde_json::to_string(&l.envelope.as_ref().unwrap().status).unwrap(),
                l.inputs,
                l.outputs,
            )
        })
        .collect()
}

#[test]
fn a_crash_at_each_boundary_yields_nothing_or_a_complete_generation_and_repair_rebuilds_the_journal()
 {
    let (reference_dir, events, _) = rect_events();
    let reference = semantic(&reference_dir);
    let reference_generation = Workspace::open(&reference_dir).unwrap().snapshot().unwrap();
    let _ = std::fs::remove_dir_all(&reference_dir);

    // The four boundaries of the spec, as event indices: after the staged tree is synced
    // (before the record write under the lock), after the artifact and record renames (before
    // the directory syncs), after the generation file (before the pointer write), and after the
    // pointer rename (before the root sync).
    let first_write = events
        .iter()
        .position(|e| matches!(e, FsEvent::Write(p) if p.ends_with("record.json")))
        .unwrap();
    let ops_rename = events
        .iter()
        .position(|e| matches!(e, FsEvent::Rename(_, to) if to.to_string_lossy().contains("/ops/")))
        .unwrap();
    let generation_sync = events
        .iter()
        .position(|e| matches!(e, FsEvent::SyncFile(p) if p.ends_with("2.json")))
        .unwrap();
    let pointer_rename = events
        .iter()
        .position(|e| matches!(e, FsEvent::Rename(_, to) if to.ends_with("CURRENT")))
        .unwrap();
    let registry = rascal_ops::registry().unwrap();
    for (name, crash_at, complete) in [
        ("after staging", first_write, false),
        ("after renames", ops_rename + 1, false),
        ("after generation file", generation_sync + 1, false),
        ("after pointer rename", pointer_rename + 1, false),
        ("after root sync", pointer_rename + 2, true),
    ] {
        let dir = scratch(&format!("boundary-{crash_at}"));
        let ws = Workspace::open(&dir).unwrap();
        let f = frame();
        let (image_id, _) = ws.add_canonical(canonical(&f), &f, "flat").unwrap();
        drop(ws);
        let fs = Arc::new(RecordingFs::crashing_at(crash_at));
        let ws = Workspace::open_with(&dir, fs.clone()).unwrap();
        let mut inputs: Inputs = BTreeMap::new();
        inputs.insert("image".into(), ws.load(&image_id).unwrap());
        let result = ws.invoke("find", "rect", &registry, &inputs, &rect_params());
        assert!(fs.crashed(), "{name}: the crash point was never reached");
        drop(ws);
        // After a restart: the workspace opens, and the generation is whole either way.
        let reopened = Workspace::open(&dir).unwrap();
        let generation = reopened.snapshot().unwrap();
        if complete {
            assert!(result.is_ok(), "{name}");
            assert_eq!(generation.n, 2, "{name}");
            assert_eq!(generation.locations.len(), 1, "{name}");
        } else {
            assert!(result.is_err(), "{name}");
            assert_eq!(generation.n, 1, "{name}: nothing published");
            assert!(generation.locations.is_empty(), "{name}");
        }
        for identity in &generation.records {
            reopened.read_record(identity).unwrap();
        }
        // Repair: orphans from the failed commit go, the journal is rebuilt from the records,
        // and a complete generation's rebuilt journal has the identities of the reference run.
        let report = Workspace::repair(&dir).unwrap();
        assert!(report.missing_dropped.is_empty(), "{name}: {report:?}");
        let lines = Journal::read(&dir.join("journal.jsonl")).unwrap();
        assert_eq!(lines[0].event, Event::Note);
        assert!(
            lines[0]
                .note
                .as_deref()
                .unwrap()
                .contains("journal rebuilt by repair"),
            "{name}"
        );
        let rebuilt = semantic(&dir);
        if complete {
            assert_eq!(
                rebuilt, reference,
                "{name}: the rebuilt journal equals the uninterrupted run"
            );
            assert_eq!(
                Workspace::open(&dir).unwrap().snapshot().unwrap().locations,
                reference_generation.locations
            );
        } else {
            assert_eq!(rebuilt.len(), 1, "{name}: only the import is committed");
            assert_eq!(rebuilt[0].1, "import");
        }
        // Nothing unreachable remains.
        let report = Workspace::repair(&dir).unwrap();
        assert!(
            report.orphans_removed.is_empty() && report.abandoned_staging_removed.is_empty(),
            "{name}: a second repair finds nothing: {report:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn cancellation_before_the_lock_publishes_nothing_and_after_it_commits() {
    let dir = scratch("cancel");
    let ws = Workspace::open(&dir).unwrap();
    let f = frame();
    let (image_id, _) = ws.add_canonical(canonical(&f), &f, "flat").unwrap();
    let registry = rascal_ops::registry().unwrap();
    let mut inputs: Inputs = BTreeMap::new();
    inputs.insert("image".into(), ws.load(&image_id).unwrap());
    // Requested before the op starts: checked before the writer lock, nothing published.
    let cancel = AtomicBool::new(true);
    let invocation = ws
        .invoke_with(
            "find",
            "rect",
            &registry,
            &inputs,
            &rect_params(),
            Some(&cancel),
        )
        .unwrap();
    assert!(invocation.cancelled);
    assert!(invocation.committed.is_none());
    assert!(invocation.ids.is_empty());
    assert_eq!(ws.snapshot().unwrap().n, 1);
    assert!(
        std::fs::read_dir(dir.join("staging"))
            .unwrap()
            .next()
            .is_none(),
        "the staging directory was removed by its owner"
    );
    let lines = Journal::read(&dir.join("journal.jsonl")).unwrap();
    assert!(
        lines
            .last()
            .unwrap()
            .note
            .as_deref()
            .unwrap()
            .contains("cancelled before the writer lock")
    );
    // Not requested: the commit completes and reports it was not cancelled.
    let cancel = AtomicBool::new(false);
    let invocation = ws
        .invoke_with(
            "find",
            "rect",
            &registry,
            &inputs,
            &rect_params(),
            Some(&cancel),
        )
        .unwrap();
    assert!(!invocation.cancelled && invocation.committed.is_some());
    // Requested once the lock is held: the commit still runs to completion and says both.
    let cancel = Arc::new(AtomicBool::new(false));
    let lock_path = dir.join("lock");
    let held = std::fs::File::create(&lock_path).unwrap();
    held.lock().unwrap();
    let flag = cancel.clone();
    let setter = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        flag.store(true, Ordering::SeqCst);
    });
    let worker = {
        let dir = dir.clone();
        let cancel = cancel.clone();
        std::thread::spawn(move || {
            let ws = Workspace::open(&dir).unwrap();
            let registry = rascal_ops::registry().unwrap();
            let mut inputs: Inputs = BTreeMap::new();
            inputs.insert("image".into(), ws.load("img-1").unwrap());
            let mut params = rect_params();
            params.insert(
                "bbox".into(),
                toml::Value::Array(
                    vec![0.2, 0.2, 0.6, 0.6]
                        .into_iter()
                        .map(toml::Value::Float)
                        .collect(),
                ),
            );
            ws.invoke_with("find", "rect", &registry, &inputs, &params, Some(&cancel))
                .unwrap()
        })
    };
    // The worker is blocked on the writer lock we hold; the flag turns on while it waits; the
    // check happened before the wait, so the commit completes when we release.
    std::thread::sleep(std::time::Duration::from_millis(500));
    held.unlock().unwrap();
    setter.join().unwrap();
    let invocation = worker.join().unwrap();
    assert!(
        invocation.committed.is_some(),
        "a commit past its check runs to completion"
    );
    assert!(invocation.cancelled, "and reports the cancellation");
    assert_eq!(
        Workspace::open(&dir)
            .unwrap()
            .snapshot()
            .unwrap()
            .locations
            .len(),
        2
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_payload_refuses_the_workspace_until_repair_drops_it() {
    let dir = scratch("missing");
    let ws = Workspace::open(&dir).unwrap();
    let f = frame();
    let (image_id, _) = ws.add_canonical(canonical(&f), &f, "flat").unwrap();
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
    inputs.insert("location".into(), ws.load("loc-2").unwrap());
    let masked = ws
        .invoke("mask", "rasterize", &registry, &inputs, &Params::new())
        .unwrap();
    let mask_entry = ws
        .snapshot()
        .unwrap()
        .artifact(&masked.ids["mask"])
        .unwrap()
        .clone();
    drop(ws);
    std::fs::remove_dir_all(dir.join(&mask_entry.path)).unwrap();
    assert!(
        Workspace::open(&dir)
            .unwrap_err()
            .to_string()
            .contains("workspace repair")
    );
    let report = Workspace::repair(&dir).unwrap();
    assert_eq!(
        report.missing_dropped,
        vec![format!("{} ({})", mask_entry.id, mask_entry.identity)]
    );
    assert_eq!(report.repaired_generation, Some(4));
    let ws = Workspace::open(&dir).unwrap();
    let generation = ws.snapshot().unwrap();
    assert_eq!(generation.n, 4);
    assert!(generation.artifact(&mask_entry.id).is_none());
    assert_eq!(
        generation.records.len(),
        3,
        "the record of the lost mask stays"
    );
    assert!(ws.load(&mask_entry.id).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn orphans_and_abandoned_staging_are_collected() {
    let dir = scratch("orphans");
    let ws = Workspace::open(&dir).unwrap();
    let f = frame();
    ws.add_canonical(canonical(&f), &f, "flat").unwrap();
    drop(ws);
    std::fs::write(dir.join("artifacts/deadbeef.series.json"), "{}").unwrap();
    std::fs::create_dir_all(dir.join("artifacts/deadbeef.mask")).unwrap();
    std::fs::write(dir.join("ops/deadbeef.json"), "{}").unwrap();
    std::fs::write(dir.join("images/deadbeef.png"), "x").unwrap();
    let abandoned = dir.join("staging/gone");
    std::fs::create_dir_all(&abandoned).unwrap();
    std::fs::write(abandoned.join("lock"), "").unwrap();
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
    std::fs::File::open(&abandoned)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let report = Workspace::repair(&dir).unwrap();
    assert_eq!(report.abandoned_staging_removed, vec!["gone".to_string()]);
    let mut orphans = report.orphans_removed.clone();
    orphans.sort();
    assert_eq!(
        orphans,
        vec![
            "artifacts/deadbeef.mask",
            "artifacts/deadbeef.series.json",
            "images/deadbeef.png",
            "ops/deadbeef.json"
        ]
    );
    assert!(report.missing_dropped.is_empty());
    assert_eq!(report.journal_lines, 2, "the note and the import");
    // The image and its record survive.
    assert_eq!(std::fs::read_dir(dir.join("images")).unwrap().count(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}
