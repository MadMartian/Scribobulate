# Plan: Test the XInclude screen against the decoder it protects

## Problem

The XInclude screen (`src/imagedecode/xinclude.rs`, CVE.md ScrSEC-1) is the only thing between a
crafted SVG and the librsvg use-after-free on Linux, Windows and macOS outside the bundle. Review
has breached it repeatedly (ScrSEC-1, "Leaks found and closed"), and every breach had the same
shape: the screen read a `data:` URI or an XML layer one way and the real decoder read it another
(CWE-436, interpretation conflict). Each leak was found by a reviewer imagining an encoding the
screen had not, and each fix added a hand-written case. Nothing tests the screen against how the
decoder actually reads its input, so the next leak will also be found by whoever imagines it first.

### Root cause

The screen is a second parser of the same bytes, written by us, and its test cases come from the
same heads that wrote it. A case list can only hold the shapes someone already thought of.

## Possible approaches

### ✅ 1. Generated encodings, judged by the loaded decoder itself

Generate SVGs that contain an XInclude under layers of encoding and nesting, and require the
screen to refuse each. When the screen lets one through, ask the real decoder (the gdk-pixbuf
loader chain the app uses, which picks librsvg) whether it acted on an XInclude in that input. It
did: that is a leak, and the test fails with the smallest such input. It did not: the generator
built something inert, and the pass was correct.

The decoder's verdict comes from rendering, not from any model of librsvg: the innermost include
carries an `xi:fallback` that draws a block of a sentinel colour, and its `href` is one librsvg
cannot resolve (a relative path, with no base URL). The block appears only if librsvg processed
the include element, which is exactly the condition the screen must refuse on. The generated input
never declares an entity, so it cannot reach the use-after-free even on a vulnerable librsvg, and
the test is safe to run on every platform's runtime, including Windows' 2.62.3.

**Pros**: the oracle is the decoder each platform really loads, so the test measures the very
disagreement every leak was, on each runtime the pipeline builds against. Shrinking hands the next
fix a minimal case instead of a review finding. The decoder is consulted only when the screen
passes, so a run stays cheap. It also measures what the screen costs honest users: generated
inputs whose include the decoder would *not* act on, but which the screen refused, are a count we
can report.

**Cons**: it finds only what the generator can compose; an encoding nobody put in the grammar
stays unseen (the byte-mutation stage under Technical details narrows this, but does not close
it). It needs librsvg's gdk-pixbuf loader on every test runner, and a dev-dependency for
generation and shrinking (POLICY: justify it). The real decoder's verdict depends on its version,
so a failure on one platform may not reproduce on another; that is correct, but each failure must
name the librsvg it ran against.

### 💡 2. A reference model as the oracle

Decode the generated inputs with a Rust model of the decoder (the `data-url` crate for `data:`
URIs, an XML parser for the tree) and require the screen to refuse whenever the model finds an
XInclude.

**Pros**: pure, fast, no loader needed, runs where librsvg is absent.
**Cons**: the model is a third parser of the same bytes. Wherever it disagrees with librsvg and
libxml2, it reproduces the very CWE-436 conflict the test exists to catch, and its verdicts would
pass the screen on exactly those inputs. Worth keeping only as a fast pre-filter, never as the
judge.

### ❌ 3. Coverage-guided fuzzing (`cargo fuzz`, libFuzzer) of the screen alone

Rejected: coverage guidance steers towards new paths in the *screen*, but the leaks live in the
gap between the screen and the decoder, which the fuzzer does not see without approach 1's oracle
anyway. It also needs a nightly toolchain and a sanitizer build off the pipeline's stable
toolchain, and does not run on the Windows MSVC runner as is. Can be layered over approach 1's
oracle later if the generated grammar stops finding things.

### ❌ 4. The published CVE trigger under a memory checker as the oracle

Rejected: it detects one shape of the use-after-free, not "the decoder acted on an XInclude",
needs a memory checker on every platform, and runs a real memory-corruption payload against the
vulnerable Windows runtime on every pipeline run.

## Recommendation

Approach 1. It is the only oracle that cannot disagree with librsvg, because it is librsvg. Build
it as an ordinary test beside the screen in `src/imagedecode/` (the loader entry point is banned
outside that module, deliberately). Run it two ways: a fixed seed and a bounded case count in the
pipeline's unit-test step on every platform, sized to stay within a few tens of seconds on the
slowest runner (measure, then set), and an on-demand long run with a random seed and a large
count, for review rounds and after any change to the screen. Every leak it finds becomes a fixed
case in the screen's own tests, and a "Leaks found and closed" line in ScrSEC-1, in the fixing
commit.

## Proposed TDD Rubrics

Edit them here; completing the plan ratifies them. They move into TDD.md (beside 2.23c) as the
behaviour is built.

### Every encoding the decoder would act on is refused
- **Given** an SVG that the platform's own image decoder would act on an XInclude in, however its
  content is layered — character references, percent or base64 `data:` URIs in any header spelling,
  CSS escapes, compression, a non-UTF-8 text encoding, nesting, and any combination of these
- **When** the screen inspects it, on Linux, Windows, or macOS outside the app bundle
- **Then** it is refused before the decoder sees it

### The test judges the screen by the decoder, never by a model of it
- **Given** a generated SVG that the screen lets through
- **When** the test asks whether the screen was right
- **Then** the answer comes from the decoder the platform actually loads, and a failure names that
  decoder's version and shows the smallest input that leaks

### The test cannot trigger the flaw it guards against
- **Given** a runtime whose decoder still has the use-after-free
- **When** the test runs on it
- **Then** no generated input can reach the flaw, so the run is safe on every platform's pipeline

### A pipeline run is reproducible and bounded; a long run is on demand
- **Given** a pipeline run on any platform
- **When** the test runs
- **Then** it covers the same generated inputs as every other pipeline run and finishes within its
  bound, while a developer can ask for a longer run over fresh inputs without changing code

### What the screen costs honest SVGs is reported
- **Given** generated inputs whose include the decoder would not act on
- **When** the screen refuses some of them
- **Then** the run reports how many, and that count is not a failure

## Technical details preserved

- **Layers the grammar must compose** (each one a past leak or a rule in the screen's table):
  XML character references (decimal, hex, either case, leading zeros); percent-encoding, single
  and repeated, either hex case; `data:` headers with `;base64` spelled every way the decoder
  accepts (`%3Bbase64`, spaces, case, extra parameters); CSS escapes (`data\3a`); tabs and
  newlines the URL parser drops; carriers `<image href>`, `xlink:href`, `<use>`, CSS `url()` in a
  `style` attribute and a `<style>` element; gzip; UTF-16 both byte orders, with and without BOM;
  the XInclude namespace under any prefix or as the default namespace. Nest up to the screen's
  depth limit, and one past it (which must refuse regardless).
- **Byte mutation stage**: mutate known-hostile seeds (every past leak's fixture) one byte or one
  layer at a time, and judge each survivor by the same oracle. Cheap, and it reaches shapes the
  grammar does not name.
- **The generator must never emit a DOCTYPE or `<!ENTITY`.** Assert it in the generator, not only
  by construction: an entity redefinition inside a nested include is the use-after-free itself.
- **Oracle facts to verify against the loaded librsvg before relying on them**: that librsvg (not
  libxml2) processes `xi:include` and its `xi:fallback`; that it resolves no relative `href`
  without a base URL, so the fallback runs; and that it decodes `data:` URIs with the `data-url`
  crate, which is why approach 2's model would not be a third parser for that one layer.
- The screen's entry is `inspect` in `src/imagedecode/xinclude.rs`; the test calls it directly,
  so it runs on macOS too, where the bundle stands the screen down.
