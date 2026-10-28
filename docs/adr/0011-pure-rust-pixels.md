# 0011: Pure Rust pixels

Date: 2026-10-28. Status: accepted.

## Context

The pixel work is distance transforms, disc morphology, Sobel gradients, row smoothing, Lab
conversion, a gradient-domain (Poisson) solve, and PNG and JPEG codecs. OpenCV would supply
all of it and cost a system library, a build step on every platform, and a binary that cannot
be installed with one `cargo install`.

## Decision

`image`, `imageproc` and `rayon` only. The Poisson solver is written in-house to the spec's
own description (Dirichlet boundary from the base image, residual evaluated every iteration,
non-convergence is an operational error). No OpenCV, no system libraries, no GPU requirement.

## Alternatives

- **OpenCV bindings.** Faster to a first result and slower for every user and every CI runner
  after that.
- **A hand-written codec set.** Unnecessary; `image` covers PNG and JPEG.

## Consequences

- Every algorithm is specified in the spec in its own words and tested on synthetic goldens
  where the answer is known.
- Performance work is explicit (`rayon` across rows), measured in the final timing run.
