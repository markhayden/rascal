//! `fake-filler <behaviour> <image> [<mask>] <out>` or `fake-filler serve <behaviour> [port]`:
//! the testkit's fake fillers as a program, for tests that drive a command filler and for a
//! person trying `rascal heal` without a model.

use rascal_testkit::fake_filler::{Behaviour, run_command, serve};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = || {
        eprintln!(
            "usage: fake-filler <behaviour> <image> [<mask>] <out>\n       fake-filler serve <behaviour> [port]\nbehaviours: exact, mis-sized, re-encoding, slow, failing, non-png"
        );
        std::process::exit(64)
    };
    match args.first().map(String::as_str) {
        Some("serve") => {
            let Some(behaviour) = args.get(1).and_then(|b| Behaviour::parse(b)) else {
                usage()
            };
            let port: u16 = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(0);
            let listener = match std::net::TcpListener::bind(("127.0.0.1", port)) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("fake-filler: bind: {e}");
                    std::process::exit(3);
                }
            };
            println!("http://{}/fill", listener.local_addr().expect("local addr"));
            serve(listener, behaviour, None, Default::default());
        }
        Some(name) => {
            let Some(behaviour) = Behaviour::parse(name) else {
                usage()
            };
            let paths: Vec<&String> = args[1..].iter().collect();
            let code = match paths.as_slice() {
                [image, out] => run_command(behaviour, image.as_ref(), None, out.as_ref()),
                [image, mask, out] => {
                    run_command(behaviour, image.as_ref(), Some(mask.as_ref()), out.as_ref())
                }
                _ => usage(),
            };
            std::process::exit(code);
        }
        None => usage(),
    }
}
