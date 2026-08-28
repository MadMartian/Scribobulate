# AI Due Diligence

Scribobulate is built with AI, and I am not going to be coy about it: the
overwhelming majority of the code, tests, and documentation in this repository was
written by AI agents working under my direction, and 95 of its 133 commits carry a
`Co-Authored-By: Claude` trailer to say so. If you are reading this, you probably
work the same way. Here is the process I use, and the evidence that it holds up.

This document is my accounting under the *Diligence* dimension of Anthropic's AI
Fluency framework: creation diligence (what I chose to build with), transparency
diligence (who wrote what, and how you can check), and deployment diligence (what
has to pass before anything reaches you). Every mechanism described here is a rule
written down in [`sdd/POLICY.md`](sdd/POLICY.md) and enforced on every change, and
this page links into it so each rule has exactly one home.

## Creation diligence: what I chose to build with

The tooling is Claude Code driving Claude models (Opus for design and
implementation, Sonnet and Haiku for bounded review and verification work), across
three developer seats (Linux, macOS, Windows). They coordinate over
[ToasterTalk](https://github.com/MadMartian/ToasterTalk), an MCP message bus I
wrote for exactly this job: the seats, the researcher, and the review orchestrator
each hold a named identity in a room and hand work to each other directly, so a
macOS finding lands in the Linux integrator's inbox while it is still warm. It is
public and Apache-2.0, so the comms layer this project runs on is auditable
alongside the project itself.

The Linux seat is the integration authority; the platform seats hold clones and
land work as `<seat>/<feature>` branches. That topology exists because a
cross-platform GTK application has to be proven on every operating system it
claims to support, and it has its own sharp edge, written up in POLICY
(§ Cross-machine seat branches) because I measured it the hard way.

Scribobulate itself ships no AI features. It does not summarize your document, it
does not call a model, and it has no account to sign into. It is a Markdown viewer
and editor for the documents *your* agents write, and it earns its keep by staying
out of the way: native CPU rendering, 0 MiB VRAM, roughly 80 MiB RAM, so the GPU
stays free for the models you actually chose to run.

Concretely, on the privacy side:

- **One kind of outbound connection exists in the whole application**: fetching a
  remote image, on an opt-in "Show Unsafe Images" path, through the app's own
  bounded HTTP client (POLICY § Architecture rules). Off by default.
- **No telemetry, no analytics, no phone-home.** There is nothing to opt out of.
- **Crash reports contain no document text.** They land in your own state
  directory and are safe to attach to a bug report.
- **Your file is never written without an explicit save**, including through crash
  recovery and conflict handling.

Three wishes drove all of that, and the [README](README.md) tells the longer story.
I wanted Scribobulate to sit beside my own Ollama models and leave the GPU to them,
because freedom from cloud computing and cloud providers should always be an option
for a developer. I wanted it to be beautiful, which is what the theming system is
for: this is a tool for humans working with AI, and the human deserves a pleasant
place to do that work. I wanted collaboration with an agent to stop being a chore,
which is what in-file annotations are for. A reader that quietly shipped your
writing to somebody else's server would undo all three at once.

## Transparency diligence: who wrote what

- **Commit trailers.** AI-co-authored commits say so. `git log --format=%b | grep
  Co-Authored-By` is the whole audit; I did not curate it after the fact.
- **The specifications are public.** `sdd/` holds the product definition,
  architecture, test rubrics, and development policy that the agents work from.
  If you want to know why the code is shaped the way it is, the reasoning is
  sitting in the repository for you to read.
- **The hard-won findings are public too.**
  [`sdd/ANTI-PATTERNS.md`](sdd/ANTI-PATTERNS.md) has 312 entries, and each one is
  earned the same way: reproduce the failure, trace the root cause (often into the
  GTK C source), and land the fix with a test that pins it. Every entry in there
  has been measured. The transferable half of that knowledge feeds a GTK4/gtk-rs
  skill which I have deliberately kept out of this repository: it is large enough
  to deserve a standalone model of its own, or at minimum a specialized research
  agent gate-keeping what gets written into it. It remains a work-in-progress with
  no publication plans either way.
- **Two registers, two jobs.** [`sdd/ISSUES.md`](sdd/ISSUES.md) is a working
  register: ephemeral, tightly scoped, and meant for tracking within and across
  developer sessions. It is supposed to *shrink*, and it holds 20 open items as I
  write this. GitHub issues do the other job, the permanent public record, and
  each register serves its own purpose.
- **Attribution is maintained.** Apache-2.0, with every dependency's licence
  recorded in [`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md).

## Deployment diligence: what has to pass before it reaches you

This is the part I care about most, because a green test run is where the work
starts. The gates below are mandatory, and the ones an agent would most like to
skip are the ones written down hardest.

- **Behaviour is pinned before it is built.** [`sdd/TDD.md`](sdd/TDD.md) holds 476
  Given/When/Then rubrics, and a change that introduces behaviour with no rubric
  is incomplete by policy.
- **Roughly 1,350 automated tests** run under `cargo test` plus a GTK integration
  suite. A test may never be `#[cfg]`'d out per platform, because a compiled-out
  test is a deleted test that still reports green.
- **Every regression is locked down in two independent areas** (POLICY § Testing):
  an automated test *and* a `tests/MANUAL-TEST.md` check that drives the exact
  broken scenario against the running application. Both areas are mandatory. The
  unit test proves the decision; the manual run proves the pixels. A correct
  decision that never reached the widget passes the first one and still fails you.
- **The manual plan gets run.** The most recent full pass drove about 200 checks
  against the real binary under an isolated compositor and reported no hard
  failures.
- **Categories of change have completeness matrices.** [`sdd/CAM.md`](sdd/CAM.md)
  forces a new command to account for every surface (menu, toolbar, accelerator,
  overlay), and new markup to account for every context it can appear in. These
  catch the gaps that ship looking fine and surface as bug reports later.
- **Code review runs as a campaign.** Reviews go through *QA*, a skill of my own
  (closed source, at least for now), which fans out a fleet of specialized
  reviewer agents the way a building inspection sends one specialist per trade: a
  squad per application sub-system, file order alternated between them to cancel
  positional attention bias, then a nit-pick pass, a spot-check pass, and an
  orchestrator that consolidates the findings and puts every one of them through a
  verify-then-include gate before the developer agent ever sees it. It runs in
  rounds, and it turns up scores of real issues per round for the developer agent
  to fix. It is enormously expensive in tokens and wall-clock, and it earns that
  back in the human hours it would otherwise take to find the same things. It
  complements my own review of the code, and it is honest about itself: each round
  records an audit trail where the pipeline's deviations are written down.
- **A human performs the commit.** Agents prepare the work and draft the message;
  I spot-check the diff, then read the message and land it. That manual pass is
  also where *QA* came from: reading AI output myself is what taught me which code
  smells and outright programming crimes recur, and a skill can be told to hunt
  for exactly those once you can name them. The signature at the bottom of the
  release is mine, and the accountability that goes with it stays with me.

## What I am not claiming

I am one person with a fleet of agents, so here is where my confidence runs out.

- Scribobulate is **early but capable**, and the README says so before it says
  anything else. There are rough edges I know about and, statistically, some I do
  not.
- Coverage is broad and it is incomplete. The live-display verification batch
  (real compositor, real GPU, real D-Bus) has to run on a physical desktop while
  nobody is using it, so it lags the automated suite by design.
- Accessibility and screen-reader behaviour are verified by hand, which means they
  are verified less often than everything else. Proper accessibility work is
  planned and contingent on adoption: I will pour months into it once I see people
  actually using this application, accessible or otherwise.
- The gates above did not descend from heaven. I built them by reading AI output
  myself, finding what broke, and turning each lesson into a rubric, a
  deterministic evaluation harness, or a policy rule. The agents are productive
  because that scaffolding exists, and the 312 entries in the anti-pattern
  register are what it cost to put up.

## The short version

I write Markdown with agents all day, I wanted a beautiful native reader for it
that leaves my GPU alone, and I built one the same way I work: delegate the
typing, keep the judgement, and make every claim checkable. The specs, the
rubrics, and the failure register are all in this repository so you can go look
for yourself. Clone it, run the suite, open the app on your own agents' output,
and tell me what I missed.
