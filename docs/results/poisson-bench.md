# Poisson solve: the 1000×1000 benchmark

Task 7.1's benchmark, not a gate: the time `rascal_ops::poisson::solve` takes on a domain of
about a million pixels in a release build. The case is the ignored test in
`crates/rascal-ops/tests/poisson.rs`: a 1024×1024 flat frame, a 1000×1000 core two pixels of
blend ring away from the border, a base of slow ramps, a fill offset by 30 levels with a
textured red channel, `poisson-v1` (`tol 1e-4`, `max_iter 2000`), the three channels on three
threads.

```
cargo test --release -p rascal-ops --test poisson -- --ignored --nocapture bench
```

| date | machine | domain | iterations (max over channels) | solve |
|---|---|---|---|---|
| 2026-10-29 | Apple M4 Max, 64 GiB, macOS 27.0, Rust 1.99.1 | 1,008,016 px | 1421 | 5.25 s |

What the number says. Conjugate gradients on a square grid needs iterations in proportion to
the domain's side, so a seam band (narrow, long) converges in far fewer than a square of the
same area. `heal seam` on the step fixture through the fake filler (a blur of the base inside
the band, so the fill already meets the base at the edge) pastes its 21,922-pixel band in 21 ms
in a debug build, the initial guess passing the first convergence test. The ceiling of 2000
iterations leaves room above this square; a larger square, or one with a sharper mismatch
along the edge, is where `max_iter` would be reached and reported as an operational error. A
preconditioner or a coarse-to-fine start is the lab's question when the owner's corpus shows
domains that need one.
