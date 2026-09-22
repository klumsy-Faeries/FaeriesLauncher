# Performance

Measured numbers only. Nothing here is an estimate — every figure comes
from a benchmark or an instrumented run, and the method is stated so you can
reproduce or challenge it.

## How to reproduce

```bash
cargo bench --workspace
```

Individual suites:

```bash
cargo bench -p faerie-modding --bench modding
cargo bench -p faerie-minecraft --bench version
cargo bench -p faerie-instances --bench store
```

Criterion writes an HTML report to `target/criterion/report/index.html` and
compares against the previous run, so a regression shows up as a labelled
change rather than a number you have to eyeball.

Startup timing is always on: every launch logs a stage breakdown to
`logs/launcher.log`, and the Performance page shows the same data for the
running session.

## Measured — 2026-08-31

Machine: Windows 11, 16 threads, 16 GB RAM. Release build for the
application figures; Criterion defaults for the micro-benchmarks.

### Mods

The spec's stress case is "hundreds of mods" (§43). Scanning opens every
jar, reads its descriptor, and parses it.

| Operation | Result |
|---|---|
| Scan 50 jars | 1.36 ms |
| Scan 200 jars | 4.71 ms |
| Scan 500 jars | 11.9 ms (~42,000 jars/sec) |
| Scan one jar | 45 µs |
| Compatibility check, 50 mods | 30.6 µs |
| Compatibility check, 200 mods | 141 µs |
| Compatibility check, 500 mods | 389 µs |

A 500-mod pack is scanned and fully compatibility-checked in about 12 ms,
so the Mods page opens without a perceptible pause at any realistic pack
size. No optimization was warranted here.

### Version ranges

Every dependency check runs one of these, so the constant matters at pack
scale even though each call is tiny.

| Operation | Result |
|---|---|
| Parse a compound range (`>=1.20.1 <1.21 \|\| 1.19.4`) | 422 ns |
| Match a simple range | 2.7 ns |
| Match a compound range | 3.4 ns |
| Match a Maven interval | 3.3 ns |

### Launch metadata

The path every launch walks: parse the version JSON, resolve
`inheritsFrom`, filter libraries by rule, build the argument vector.

| Operation | Result |
|---|---|
| Parse version JSON, 130 libraries (realistic 26.2) | 54 µs |
| Parse version JSON, 400 libraries | 152 µs |
| Resolve Fabric inheritance onto vanilla | 62 µs |
| Rule-filter 130 libraries | 407 ns |
| Build and substitute game arguments | 1.5 µs |

Total metadata work per launch is well under a millisecond. Launch time is
dominated by downloads and JVM startup, not by anything the launcher
computes — so optimizing this path further would be wasted effort.

### Instances

`list` runs on every page that shows instances, so it is the most
frequently executed I/O in the launcher.

| Operation | Before | After | Change |
|---|---|---|---|
| List 10 instances | 585 µs | 274 µs | −56% |
| List 50 instances | 3.21 ms | 1.36 ms | −58% |
| List 200 instances | 12.6 ms | 5.69 ms | −55% |
| Create an instance | 1.23 ms | 1.09 ms | −11% |

**What changed.** Listing made three filesystem round-trips per instance:
`is_dir()`, `is_file()` on `instance.json`, then the read. Two were
removable — `file_type()` comes free from the directory iteration, and a
missing `instance.json` is something the read itself reports. Criterion
confirms the improvement at p < 0.05.

The same measurement exposed a second problem: the Home page fetched the
instance list twice, once for the page and again inside its Instance card.
The card now receives the data instead.

### Application

| Measure | Result | Method |
|---|---|---|
| Startup (cold, empty data directory) | **2.3–2.6 ms** | instrumented, release build |
| Release binary size | 14.7 MB | on disk |
| Idle memory, native process | **49 MB** | working set |
| Idle memory, total attributable | **~422 MB** | kill-delta, confirmed by parent-process walk |

Startup breakdown: `paths` 0.8–1.0 ms · `config` 0.1 ms · `logging` 1.3 ms ·
`services` 0.1–0.2 ms. Window presentation happens after this and is
governed by WebView2, not by launcher code.

## The memory finding

**The launcher uses about 422 MB at idle, not the 90–150 MB estimated when
the stack was chosen.** The native process is 49 MB; the remaining ~373 MB
is six WebView2 processes.

This was measured two ways that agree: walking the parent-process chain to
attribute WebView2 processes to the launcher (374 MB across 6 processes),
and killing the launcher to observe the system-wide drop (421.5 MB). Note
that WebView2 processes are shared infrastructure — on the test machine
another six unrelated processes accounted for a further 309 MB, so naively
summing every `msedgewebview2.exe` overstates the launcher's cost by nearly
double.

**Attempted fix that did not work.** WebView2 accepts browser flags, so
`--disable-features=…`, `--renderer-process-limit=1`, and a reduced
`--js-flags=--max-old-space-size` were tried and measured: 730.1 MB versus
730.7 MB, and the process count was unchanged. That is noise, so the flags
were removed rather than kept for appearances.

This is inherent to rendering the interface in a WebView. It buys the
theming and layout flexibility the launcher is built around — themes as
CSS tokens, artwork as data URIs, layout as JSON — which a native toolkit
would not give as cheaply. Whether that trade is worth 370 MB is a product
decision, not a technical one, and the alternative (a native Rust UI such
as `iced`) remains open at the cost of most of the customization surface.

## Targets

Derived from what is now measured, not aspiration:

| Target | Status |
|---|---|
| Startup under 1.5 s | Met — launcher init is ~2.5 ms |
| Responsive with 500 mods | Met — 12 ms to scan and check |
| Responsive with 200 instances | Met — 5.7 ms to list |
| Near-zero idle CPU | Met by design: no polling; the only sampler is the Performance page, which stops on close |
| Low idle RAM | **Not met** — see above |
