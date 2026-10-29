//! The XInclude screen judged by the decoder it protects (TDD 2.23d–2.23i; ScrSEC-1 in
//! `sdd/CVE.md`).
//!
//! Every leak review found in [`super::xinclude`] had one shape: the screen read a
//! `data:` URI or an XML layer one way and librsvg another. A case list holds only the
//! shapes someone thought of, so this test generates them ([`grammar`], and the
//! byte-mutation stage in [`seeds`]), requires the screen to refuse, and when it does
//! not, asks the decoder this platform really loads ([`oracle`]) whether it acted on an
//! include in that input. It did: a leak, reported with the smallest input that still
//! leaks and the librsvg that judged it. It did not: the input was inert, and passing
//! it was right.
//!
//! **Safe on a vulnerable decoder.** No input reaches the decoder unless [`safety`] has
//! found no `DOCTYPE` or `ENTITY` in anything it decodes to, so a run cannot trigger
//! the use-after-free even on Windows' librsvg 2.62.3.
//!
//! **Two runs.** By default a fixed seed and a bounded count, the same inputs on every
//! platform, sized for the slowest runner. With `SCRIBOBULATE_XINCLUDE_DIFFERENTIAL_CASES`
//! set, that many cases from a fresh random seed (or `…_SEED`), which also has the
//! decoder judge a sample of what the screen refused and reports how many it would not
//! have acted on: what the screen costs honest SVGs. How to run it is in POLICY.md.

mod grammar;
mod oracle;
mod safety;
mod seeds;

use super::xinclude::{self, Reason, MAX_DEPTH};
use grammar::{Carrier, Case, Core, Embed, Op, Rng};
use oracle::Verdict;

const PIPELINE_SEED: u64 = 0x5C21_B0B1_7A7E_2026;
/// Sized for the slowest runner: most cases are refused and never decoded, so a case
/// costs a build, the entity check and one screen.
const PIPELINE_CASES: usize = 1200;
const CASES_VAR: &str = "SCRIBOBULATE_XINCLUDE_DIFFERENTIAL_CASES";
const SEED_VAR: &str = "SCRIBOBULATE_XINCLUDE_DIFFERENTIAL_SEED";
/// In a long run, one refused input in this many is also judged by the decoder.
const OVER_REFUSAL_EVERY: usize = 16;
/// The most candidates one shrink tries.
const SHRINK_ATTEMPTS: usize = 400;
/// The negative control's namespace: an include in it is an unknown element.
const NOT_XINCLUDE_NS: &str = "http://example.invalid/not-xinclude";

type Screen = fn(&[u8]) -> Result<(), Reason>;

struct Config {
    seed: u64,
    cases: usize,
    long: bool,
}

impl Config {
    fn pipeline() -> Self {
        Self {
            seed: PIPELINE_SEED,
            cases: PIPELINE_CASES,
            long: false,
        }
    }

    fn from_env() -> Result<Self, String> {
        let Ok(cases) = std::env::var(CASES_VAR) else {
            return Ok(Self::pipeline());
        };
        let cases = cases
            .parse()
            .map_err(|_| format!("{CASES_VAR}={cases:?} is not a case count"))?;
        let seed = match std::env::var(SEED_VAR) {
            Ok(seed) => parse_seed(&seed).ok_or(format!("{SEED_VAR}={seed:?} is not a seed"))?,
            Err(_) => fresh_seed(),
        };
        Ok(Self {
            seed,
            cases,
            long: true,
        })
    }

    fn describe(&self) -> String {
        let kind = if self.long {
            "long run"
        } else {
            "pipeline run"
        };
        format!(
            "{kind}, seed {:#x}, {} cases; rerun it with {SEED_VAR}={:#x} {CASES_VAR}={}",
            self.seed, self.cases, self.seed, self.cases
        )
    }
}

fn parse_seed(text: &str) -> Option<u64> {
    match text.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => text.parse().ok(),
    }
}

fn fresh_seed() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    nanos ^ (u64::from(std::process::id()) << 32)
}

#[derive(Default, Debug)]
struct Stats {
    twins: usize,
    live_twins: usize,
    generated: usize,
    mutants: usize,
    dropped: usize,
    refused: usize,
    passed: usize,
    sampled_refusals: usize,
    over_refused: usize,
}

/// Where a leaking input came from: what the failure message shows.
enum Origin {
    Twin(String),
    Generated(Case),
    Mutant {
        twin: String,
        mutation: seeds::Mutation,
    },
}

/// Whether `bytes` leak: admitted, let through by `screen`, and acted on.
fn leaks(bytes: &[u8], screen: Screen) -> bool {
    screen(bytes).is_ok()
        && safety::admit(bytes).is_some_and(|input| oracle::judge(&input).0 == Verdict::Acted)
}

/// The positive control, then the negative one: the oracle must see an include the
/// decoder acts on, and must not see one it does not, before any verdict is trusted.
/// Returns the decoder that will judge.
fn control() -> Result<String, String> {
    let judge = |text: String| {
        let input = safety::admit(text.as_bytes()).ok_or("the entity check dropped a control")?;
        Ok::<_, String>(oracle::judge(&input))
    };
    if let (Verdict::Inert, error) = judge(Core::PLAIN.text())? {
        let said = error
            .map(|e| format!(" (the loader said: {e})"))
            .unwrap_or_default();
        return Err(format!(
            "the oracle cannot see an include, so it cannot judge the screen: {} did not \
             draw the control's fallback{said}. An SVG loader missing from gdk-pixbuf's \
             loader cache looks like this.",
            oracle::decoder()
        ));
    }
    if judge(Core::PLAIN.with_namespace(NOT_XINCLUDE_NS))?.0 == Verdict::Acted {
        return Err(format!(
            "{} drew the sentinel for an include in no XInclude namespace: the oracle cannot \
             tell an include processed from one merely present",
            oracle::decoder()
        ));
    }
    Ok(oracle::decoder())
}

fn run(config: &Config, screen: Screen) -> Result<Stats, String> {
    let decoder = control()?;
    let leak = |origin: Origin, bytes: &[u8]| leak_report(config, &decoder, origin, bytes, screen);
    let mut stats = Stats::default();
    let twins = seeds::twins();
    for twin in &twins {
        let input = safety::admit(&twin.bytes)
            .ok_or_else(|| format!("the entity check drops the twin {:?}", twin.name))?;
        stats.twins += 1;
        if oracle::judge(&input).0 == Verdict::Acted {
            stats.live_twins += 1;
            if screen(&twin.bytes).is_ok() {
                return Err(leak(Origin::Twin(twin.name.clone()), &twin.bytes));
            }
        }
    }
    if stats.live_twins == 0 {
        return Err(format!(
            "{decoder} acted on none of the {} twins, so mutating them would test nothing",
            stats.twins
        ));
    }
    let too_deep = (0..Case::MAX_EMBEDS).fold(Core::PLAIN.text().into_bytes(), |doc, _| {
        Embed::plain(Carrier::ImageHref).wrap(&doc)
    });
    if screen(&too_deep).is_ok() {
        return Err(format!(
            "the screen let through a document nested {} deep, one past its limit",
            Case::MAX_EMBEDS
        ));
    }
    let mut rng = Rng::new(config.seed);
    for i in 0..config.cases {
        let (origin, bytes) = if i % 2 == 0 {
            stats.generated += 1;
            let (case, bytes) = Case::random(&mut rng);
            (Origin::Generated(case), bytes)
        } else {
            stats.mutants += 1;
            let twin = &twins[rng.below(twins.len())];
            let (bytes, mutation) = seeds::mutate(&twin.bytes, &mut rng);
            let twin = twin.name.clone();
            (Origin::Mutant { twin, mutation }, bytes)
        };
        let Some(input) = safety::admit(&bytes) else {
            stats.dropped += 1;
            continue;
        };
        if screen(&bytes).is_err() {
            stats.refused += 1;
            if config.long && stats.refused % OVER_REFUSAL_EVERY == 0 {
                stats.sampled_refusals += 1;
                if oracle::judge(&input).0 == Verdict::Inert {
                    stats.over_refused += 1;
                }
            }
            continue;
        }
        stats.passed += 1;
        if oracle::judge(&input).0 == Verdict::Acted {
            return Err(leak(origin, &bytes));
        }
    }
    Ok(stats)
}

/// The failure: the decoder, how to rerun, and the smallest input that still leaks.
fn leak_report(
    config: &Config,
    decoder: &str,
    origin: Origin,
    bytes: &[u8],
    screen: Screen,
) -> String {
    let (built, minimal) = match &origin {
        Origin::Generated(case) => {
            let small = shrink(case.clone(), screen);
            (format!("generated: {small:?}"), small.build())
        }
        Origin::Twin(name) => (format!("the twin {name:?}, unmutated"), bytes.to_vec()),
        Origin::Mutant { twin, mutation } => (
            format!("the twin {twin:?}, mutated by {mutation:?}"),
            bytes.to_vec(),
        ),
    };
    format!(
        "the XInclude screen let through an input that {decoder} acted on ({}).\n\
         Smallest leaking input found, {} bytes:\n{}\nBuilt from {built}",
        config.describe(),
        minimal.len(),
        minimal.escape_ascii()
    )
}

/// `case` with ops dropped and simplified for as long as it still leaks.
fn shrink(mut case: Case, screen: Screen) -> Case {
    let mut attempts = 0;
    'improved: loop {
        let mut candidates = Vec::new();
        if case.core != Core::PLAIN {
            candidates.push(Case {
                core: Core::PLAIN,
                ops: case.ops.clone(),
            });
        }
        for i in 0..case.ops.len() {
            let mut fewer = case.clone();
            fewer.ops.remove(i);
            candidates.push(fewer);
            if let Op::Embed(embed) = &case.ops[i] {
                for simpler in embed.simpler() {
                    let mut c = case.clone();
                    c.ops[i] = Op::Embed(simpler);
                    if c != case {
                        candidates.push(c);
                    }
                }
            }
        }
        for candidate in candidates {
            attempts += 1;
            if attempts > SHRINK_ATTEMPTS {
                break 'improved;
            }
            if leaks(&candidate.build(), screen) {
                case = candidate;
                continue 'improved;
            }
        }
        break;
    }
    case
}

/// TDD 2.23d: every generated input the decoder would act on is refused, judged by the
/// decoder itself after a control proves it can see an include.
#[test]
fn the_screen_refuses_every_input_the_decoder_would_act_on() {
    let config = Config::from_env().unwrap_or_else(|e| panic!("{e}"));
    let started = std::time::Instant::now();
    let stats = run(&config, xinclude::inspect).unwrap_or_else(|failure| panic!("{failure}"));
    println!(
        "[2.23d] XInclude differential, {} ({}; {:.1?}): {} generated, {} mutants, {} dropped \
         by the entity check, {} refused, {} let through and inert; {} of {} twins live",
        oracle::decoder(),
        config.describe(),
        started.elapsed(),
        stats.generated,
        stats.mutants,
        stats.dropped,
        stats.refused,
        stats.passed,
        stats.live_twins,
        stats.twins
    );
    if config.long {
        println!(
            "[2.23i] over-refusal: of {} refused inputs the decoder judged, it would not have \
             acted on {}",
            stats.sampled_refusals, stats.over_refused
        );
    }
}

/// The gate's mutation test: a screen that refuses nothing must fail the run, with a
/// leaking input in the message.
#[test]
fn a_screen_that_refuses_nothing_fails_the_run() {
    let config = Config {
        cases: 0,
        ..Config::pipeline()
    };
    let failure = run(&config, |_| Ok(())).expect_err("a screen refusing nothing passed");
    assert!(failure.contains("let through an input that"), "{failure}");
    assert!(failure.contains("Smallest leaking input"), "{failure}");
}

/// A leaking generated case is shrunk to what it needs: under a screen refusing nothing
/// the core alone leaks, so every op and every core variation is dropped.
#[test]
fn a_leaking_case_is_shrunk_to_what_it_needs() {
    let padded = Case {
        core: Core {
            prefix: grammar::Prefix::Other,
            ns_references: 7,
            declaration: true,
        },
        ops: vec![
            Op::Embed(Embed::plain(Carrier::UseHref)),
            Op::Gzip,
            Op::Embed(Embed::plain(Carrier::ImageHref)),
        ],
    };
    let nothing: Screen = |_| Ok(());
    assert!(
        leaks(&padded.build(), nothing),
        "precondition: the padded case is live"
    );
    assert_eq!(
        shrink(padded, nothing),
        Case {
            core: Core::PLAIN,
            ops: Vec::new()
        }
    );
}

/// The pipeline run's generated half composes every layer the grammar names, nested to
/// one past the screen's depth limit, so a bounded run is not a narrow one.
#[test]
fn the_pipeline_run_composes_every_layer() {
    let mut rng = Rng::new(PIPELINE_SEED);
    let cases: Vec<Case> = (0..PIPELINE_CASES / 2)
        .map(|_| Case::random(&mut rng).0)
        .collect();
    let embeds = || {
        cases.iter().flat_map(|c| &c.ops).filter_map(|op| match op {
            Op::Embed(e) => Some(e),
            _ => None,
        })
    };
    for carrier in grammar::CARRIERS {
        assert!(
            embeds().any(|e| e.carrier == *carrier),
            "{carrier:?} never generated"
        );
    }
    assert!(cases.iter().flat_map(|c| &c.ops).any(|op| *op == Op::Gzip));
    assert!(cases
        .iter()
        .flat_map(|c| &c.ops)
        .any(|op| matches!(op, Op::Utf16 { .. })));
    assert!(cases.iter().any(|c| c.embeds() == Case::MAX_EMBEDS));
    assert_eq!(Case::MAX_EMBEDS, MAX_DEPTH + 1);
}
