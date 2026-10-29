//! Task 2.7: the filler port driven directly with each fake, as a command and over HTTP.
//! Each behaviour produces its documented outcome; the request-field matrix for every profile
//! and `send_mask`; a non-PNG body is an operational error; the key never appears in any file
//! the call wrote. Needs the `testkit` feature for the fake command inside the binary.
#![cfg(feature = "testkit")]
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::time::Instant;

use image::{DynamicImage, GrayImage, Luma};
use rascal_core::config::{Config, FillerConfig};
use rascal_core::frame::{Frame, Projection};
use rascal_core::image::{Image, RenderingRole, Role};
use rascal_core::rendering::Profile;
use rascal_mask::filler::http::HttpFiller;
use rascal_mask::filler::{self, FillRequest, Filler, ImageSource, MaskSource, request_fields};
use rascal_testkit::fake_filler::{Behaviour, spawn};
use std::time::Duration;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-fillers-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A request: the floor as the working image and a centred grey block as the mask rendering.
fn sample_request() -> (FillRequest, Image, GrayImage) {
    let f = Frame::raw(64, 40, Projection::Flat);
    let working = Image::new(
        "w",
        DynamicImage::ImageRgb8(rascal_testkit::images::floor(64, 40)),
        &f,
        Role::Working,
        None,
    );
    let mut grey = GrayImage::new(64, 40);
    for y in 10..30 {
        for x in 20..44 {
            grey.put_pixel(x, y, Luma([255]));
        }
    }
    let rendering = Image::new(
        "r",
        DynamicImage::ImageLuma8(grey.clone()),
        &f,
        Role::Rendering,
        Some(RenderingRole::MaskInput),
    );
    let request = FillRequest::build(
        RenderingRole::MaskInput,
        false,
        &working,
        &rendering,
        None,
        None,
    )
    .unwrap();
    (request, working, grey)
}

fn command_config(behaviour: Behaviour, with_mask: bool, timeout_s: u64) -> FillerConfig {
    let mut command = vec![
        env!("CARGO_BIN_EXE_rascal").to_string(),
        "fake-filler".to_string(),
        behaviour.name().to_string(),
        "{image}".to_string(),
    ];
    if with_mask {
        command.push("{mask}".to_string());
    }
    command.push("{out}".to_string());
    FillerConfig {
        command: Some(command),
        url: None,
        profile: "grey".to_string(),
        paste: "feather".to_string(),
        timeout_s,
        send_mask: false,
        key_env: None,
        max_side: None,
        revision: Some("fake".to_string()),
    }
}

fn http_config(url: &str, key_env: Option<&str>, timeout_s: u64) -> FillerConfig {
    FillerConfig {
        command: None,
        url: Some(url.to_string()),
        profile: "grey".to_string(),
        paste: "feather".to_string(),
        timeout_s,
        send_mask: false,
        key_env: key_env.map(str::to_string),
        max_side: None,
        revision: None,
    }
}

#[test]
fn the_request_field_matrix_for_every_profile_and_send_mask() {
    for profile in Profile::ALL {
        let Some(role) = profile.role() else {
            continue; // the inspection profile fills nothing
        };
        for send_mask in [false, true] {
            let fields = request_fields(role, send_mask);
            match role {
                RenderingRole::MaskInput => {
                    assert_eq!(fields.image, ImageSource::Working, "{profile:?}");
                    assert_eq!(fields.mask, MaskSource::Rendering, "{profile:?}");
                }
                RenderingRole::ImageInput => {
                    assert_eq!(fields.image, ImageSource::Rendering, "{profile:?}");
                    assert_eq!(
                        fields.mask,
                        if send_mask {
                            MaskSource::Grey
                        } else {
                            MaskSource::None
                        },
                        "{profile:?} send_mask {send_mask}"
                    );
                }
            }
        }
    }
    // Building an image-input request with send_mask needs the grey rendering.
    let (_, working, grey) = sample_request();
    let f = Frame::raw(64, 40, Projection::Flat);
    let tinted = Image::new(
        "t",
        DynamicImage::ImageRgb8(working.pixels.to_rgb8()),
        &f,
        Role::Rendering,
        Some(RenderingRole::ImageInput),
    );
    let grey_image = Image::new(
        "g",
        DynamicImage::ImageLuma8(grey),
        &f,
        Role::Rendering,
        Some(RenderingRole::MaskInput),
    );
    let without = FillRequest::build(
        RenderingRole::ImageInput,
        false,
        &working,
        &tinted,
        None,
        Some("a prompt"),
    )
    .unwrap();
    assert!(without.mask_png.is_none());
    assert_eq!(without.prompt.as_deref(), Some("a prompt"));
    assert!(
        FillRequest::build(
            RenderingRole::ImageInput,
            true,
            &working,
            &tinted,
            None,
            None
        )
        .is_err()
    );
    let with = FillRequest::build(
        RenderingRole::ImageInput,
        true,
        &working,
        &tinted,
        Some(&grey_image),
        None,
    )
    .unwrap();
    assert!(with.mask_png.is_some());
    assert_eq!((with.width, with.height), (64, 40));
}

#[test]
fn each_fake_behaviour_produces_its_outcome_as_a_command() {
    let dir = scratch("command");
    let (request, working, grey) = sample_request();
    let before = working.pixels.to_rgb8();
    for behaviour in Behaviour::ALL {
        let config = command_config(behaviour, true, 1);
        let filler = filler::from_config("fake", &config).unwrap();
        let started = Instant::now();
        let result = filler.call(&request, &dir);
        match behaviour {
            Behaviour::Exact => {
                let (fill, call) = result.unwrap();
                assert_eq!((fill.width(), fill.height()), (64, 40));
                let after = fill.to_rgb8();
                let mut changed_inside = 0;
                for (x, y, p) in after.enumerate_pixels() {
                    if grey.get_pixel(x, y)[0] == 0 {
                        assert_eq!(
                            p,
                            before.get_pixel(x, y),
                            "{x},{y}: untouched outside the mask"
                        );
                    } else if p != before.get_pixel(x, y) {
                        changed_inside += 1;
                    }
                }
                assert!(
                    changed_inside > 100,
                    "the blur changed {changed_inside} pixels"
                );
                assert_eq!(call.outcome, "ok");
                assert_eq!(call.status, 0);
                assert_eq!((call.request_width, call.request_height), (64, 40));
                assert!(call.request_bytes > 0 && call.response_bytes > 0);
                assert!(
                    call.target.contains("{image}") && call.target.contains("{out}"),
                    "{}",
                    call.target
                );
            }
            Behaviour::MisSized => {
                let err = result.unwrap_err().to_string();
                assert!(
                    err.contains("65x40") && err.contains("64x40") && err.contains("no resize"),
                    "{err}"
                );
            }
            Behaviour::Reencoding => {
                let (fill, _) = result.unwrap();
                let after = fill.to_rgb8();
                for (x, y, p) in after.enumerate_pixels() {
                    assert_ne!(p, before.get_pixel(x, y), "{x},{y}: every pixel changed");
                }
            }
            Behaviour::Slow => {
                let err = result.unwrap_err().to_string();
                assert!(err.contains("no answer within 1 s"), "{err}");
                assert!(
                    started.elapsed().as_secs() < 3,
                    "the process was killed at the timeout"
                );
            }
            Behaviour::Failing => {
                let err = result.unwrap_err().to_string();
                assert!(
                    err.contains("exited with 2") && err.contains("failing as asked"),
                    "{err}"
                );
            }
            Behaviour::NonPng => {
                let err = result.unwrap_err().to_string();
                assert!(err.contains("not a PNG"), "{err}");
            }
        }
    }
    // A command naming {mask} for a profile that sends none is a usage error before anything runs.
    let (no_mask, _, _) = {
        let (_, working, grey) = sample_request();
        let f = Frame::raw(64, 40, Projection::Flat);
        let tinted = Image::new(
            "t",
            DynamicImage::ImageRgb8(working.pixels.to_rgb8()),
            &f,
            Role::Rendering,
            Some(RenderingRole::ImageInput),
        );
        (
            FillRequest::build(
                RenderingRole::ImageInput,
                false,
                &working,
                &tinted,
                None,
                None,
            )
            .unwrap(),
            working,
            grey,
        )
    };
    let filler = filler::from_config("fake", &command_config(Behaviour::Exact, true, 5)).unwrap();
    let err = filler.call(&no_mask, &dir).unwrap_err().to_string();
    assert!(
        err.contains("{mask}") && err.contains("sends no mask"),
        "{err}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn each_fake_behaviour_produces_its_outcome_over_http_and_the_key_stays_out_of_every_file() {
    let dir = scratch("http");
    let (request, working, _) = sample_request();
    let before = working.pixels.to_rgb8();
    for behaviour in Behaviour::ALL {
        let (url, seen) = spawn(behaviour, None);
        let filler = filler::from_config("hosted", &http_config(&url, None, 1)).unwrap();
        let started = Instant::now();
        let result = filler.call(&request, &dir);
        match behaviour {
            Behaviour::Exact => {
                let (fill, call) = result.unwrap();
                assert_eq!((fill.width(), fill.height()), (64, 40));
                assert_eq!(call.status, 200);
                assert_eq!(call.target, url);
                let log = seen.lock().unwrap();
                assert_eq!(log.len(), 1);
                assert!(
                    log[0].has_image && log[0].has_mask,
                    "a mask-input profile sends both"
                );
                assert!(log[0].authorization.is_none());
                assert_eq!(log[0].image_bytes, request.image_png.len());
            }
            Behaviour::MisSized => {
                assert!(result.unwrap_err().to_string().contains("no resize"));
            }
            Behaviour::Reencoding => {
                let after = result.unwrap().0.to_rgb8();
                assert!(
                    after
                        .enumerate_pixels()
                        .all(|(x, y, p)| p != before.get_pixel(x, y))
                );
            }
            Behaviour::Slow => {
                let err = result.unwrap_err().to_string();
                assert!(err.contains("no answer within 1 s"), "{err}");
                assert!(started.elapsed().as_secs() < 3);
            }
            Behaviour::Failing => {
                let err = result.unwrap_err().to_string();
                assert!(
                    err.contains("HTTP 500") && err.contains("failing as asked"),
                    "{err}"
                );
            }
            Behaviour::NonPng => {
                let err = result.unwrap_err().to_string();
                assert!(
                    err.contains("not a PNG") && err.contains("text/plain"),
                    "{err}"
                );
            }
        }
    }
    // The bearer key: read from key_env at call time, sent in the header, never written. The
    // key source is a closure here (a test may not set the process environment); the
    // environment path is the "not set" case below.
    let key = "sk-test-0123456789-never-on-disk";
    let (url, seen) = spawn(Behaviour::Exact, Some(key.to_string()));
    let var = "RASCAL_TEST_FILLER_KEY";
    let filler = HttpFiller::new(
        "hosted",
        &url,
        Some(var.to_string()),
        Duration::from_secs(5),
    )
    .unwrap()
    .with_key_source(move |name| (name == var).then(|| key.to_string()));
    let (_, call) = filler.call(&request, &dir).unwrap();
    assert_eq!(
        seen.lock().unwrap()[0].authorization.as_deref(),
        Some(&*format!("Bearer {key}"))
    );
    let note = call.note();
    assert!(note.starts_with("filler call: {"));
    assert!(!note.contains(key), "{note}");
    std::fs::write(dir.join("call.json"), &note).unwrap();
    for entry in walk(&dir) {
        let bytes = std::fs::read(&entry).unwrap();
        assert!(
            !bytes.windows(key.len()).any(|w| w == key.as_bytes()),
            "{} holds the key",
            entry.display()
        );
    }
    // A wrong key is refused by the server and reported as HTTP 401 without the key.
    let wrong = HttpFiller::new(
        "hosted",
        &url,
        Some(var.to_string()),
        Duration::from_secs(5),
    )
    .unwrap()
    .with_key_source(|_| Some("not-the-key".to_string()));
    let err = wrong.call(&request, &dir).unwrap_err().to_string();
    assert!(
        err.contains("HTTP 401") && !err.contains("not-the-key"),
        "{err}"
    );
    // An unset key_env (the real environment, where the variable does not exist) is a usage
    // error naming the variable, before any call.
    let unset = filler::from_config("hosted", &http_config(&url, Some(var), 5)).unwrap();
    let err = unset.call(&request, &dir).unwrap_err().to_string();
    assert!(err.contains(var) && err.contains("not set"), "{err}");
    assert_eq!(seen.lock().unwrap().len(), 2, "the unset key made no call");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_configuration_checks_its_filler_entries() {
    let good = r#"
config_version = 1
[fillers.local]
command = ["fill", "{image}", "{mask}", "{out}"]
profile = "grey"
[fillers.hosted]
url = "https://example.invalid/fill"
profile = "tinted"
paste = "hard"
key_env = "HOSTED_KEY"
timeout_s = 30
max_side = 2048
send_mask = true
revision = "r1"
"#;
    let config = Config::parse(good, "test").unwrap();
    assert_eq!(config.fillers.len(), 2);
    assert_eq!(
        config.fillers["local"].paste, "feather",
        "the default paste"
    );
    assert_eq!(
        config.fillers["local"].timeout_s, 120,
        "the default timeout"
    );
    assert!(config.fillers["local"].mask_input() && !config.fillers["hosted"].mask_input());
    for (bad, message) in [
        ("[fillers.x]\nprofile = \"grey\"\n", "give command or url"),
        (
            "[fillers.x]\ncommand = [\"a\"]\nurl = \"http://h\"\nprofile = \"grey\"\n",
            "not both",
        ),
        (
            "[fillers.x]\ncommand = [\"a\"]\nprofile = \"overlay\"\n",
            "profile overlay is not one of",
        ),
        (
            "[fillers.x]\ncommand = [\"a\"]\nprofile = \"grey\"\npaste = \"glue\"\n",
            "paste glue is not one of",
        ),
        (
            "[fillers.x]\ncommand = [\"a\"]\nprofile = \"grey\"\ntimeout_s = 0\n",
            "timeout_s",
        ),
        (
            "[fillers.x]\ncommand = [\"a\"]\nprofile = \"grey\"\nkey_env = \"K\"\n",
            "key_env goes with url",
        ),
        (
            "[fillers.x]\ncommand = [\"a\"]\nprofile = \"grey\"\nbogus = 1\n",
            "unknown field",
        ),
    ] {
        let err = Config::parse(&format!("config_version = 1\n{bad}"), "test")
            .unwrap_err()
            .to_string();
        assert!(err.contains(message), "{bad}: {err}");
    }
}

fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}
