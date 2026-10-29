# Security Vulnerabilities

Scribobulate's register of **security vulnerabilities that have been patched or worked around**:
what the flaw is, how it reached this application, what we did about it, every way our
workaround was later found to leak and how each leak was closed, and — where the flaw is a
dependency's — what upstream must deliver before the workaround can be removed.

**Why a register of its own.** A bug in a dependency is usually something we can live with until
upstream fixes it. A security flaw usually is not: it can corrupt memory or run someone else's
code on the reader's machine, so it warrants an interim workaround in our own code even when the
real fix is upstream's to make. Those workarounds are security-critical code that will one day be
deleted, and both facts — why the code is there, and what lets us delete it — must outlive the
session that wrote it.

**What goes where.**
- **An open vulnerability** — known, and not yet patched or worked around in a released build — is
  an entry in the known-issues register (`sdd/ISSUES.md`), triaged like any other open defect.
- **A vulnerability that is patched or worked around** gets an entry here, and stays here: when the
  upstream fix finally reaches every platform and the workaround is deleted, the entry is marked
  **Closed** with the date and kept, as the record of what the deleted code was for.
- **An ISSUES entry may point here** for the mitigation detail of a still-open upstream flaw.
  Nothing outside ISSUES.md points at an ISSUES entry.

**Disclosure rule — this file is published.** The repository is public, so everything written
here, in a commit message, in a code comment or in a test is published the moment it is pushed,
and from then on it is a race between whoever reads it to attack and whoever reads it to defend.
Therefore:
- **A flaw in a dependency is reported upstream before any detail of it is pushed.** The entry
  cites the report — the upstream ticket, advisory or security-channel reference — with its date
  and state. A flaw upstream does not yet know about goes to upstream's **private** security
  channel (their `SECURITY.md`, security tracker or security address), not a public ticket, and
  nothing that would reproduce it — inputs, routes, fixtures, a descriptive commit subject — is
  pushed until upstream has published a fix or the agreed disclosure date has passed. Until then
  the entry may exist only as a placeholder that names the component and the report reference,
  never the route.
- **A packaging channel that delivers the dependency to us counts as upstream for closing it.**
  Where the fix exists upstream but has not reached what we ship or load (a distribution package,
  a pinned build bundle), a report to that channel is required as well, and the entry cites it.
- **A flaw in our own code** has no upstream to report to; the entry says so and why ("None: our
  packaging", "None: our workaround's own leak"). The same caution still applies to an UNPATCHED
  one: patch before describing it in anything that is pushed.
- **An open vulnerability in ISSUES.md** follows the same rule: until it is reported and safe to
  describe, its entry names the component and the report, not the route.
- Filing an upstream report is an action outside this repository and is the **operator's** to
  take or approve; an agent drafts it and records it here once filed.

**Citation.** An entry is `ScrSEC-N`. Numbers are frozen: never renumbered, never reused. Cite the
upstream identifiers (CVE, GHSA, RUSTSEC, upstream issue) inside the entry, and cite this
project's changes by commit subject and date, never by hash (POLICY § SDD register writes).

**Read an entry before changing the code it names.** A workaround here was usually hardened
round after round by review; its entry lists the bypasses already closed, which are exactly the
shapes a "simplification" reopens.

## Entry format

Each entry carries, in this order:
- **Status** — `Worked around` (interim code in place, upstream fix pending somewhere),
  `Patched` (fixed in our own code, nothing upstream), or `Closed` (workaround deleted; dated).
- **Severity** — worst realistic consequence on our builds: Critical / High / Medium / Low.
- **Upstream** — advisory and tracker identifiers, the fixed upstream version, with the date the
  state was checked. `None` for a flaw in our own code.
- **Classification** — REQUIRED: every relevant CVE identifier (or "no CVE assigned"), and every
  relevant CWE (MITRE's weakness enumeration), each linked
  (`https://www.cve.org/CVERecord?id=…`, `https://cwe.mitre.org/data/definitions/N.html`) with
  the CWE's name. Classify the flaw itself and, separately, each weakness found later in our
  workaround, since those are often a different class from the flaw they work around.
- **Upstream ticket** — REQUIRED for every flaw with an upstream component, whoever filed it: the
  upstream bug report, advisory or security-channel reference, marked **filed by us** or
  **pre-existing**, with its date and state as last checked. Also one per packaging channel that
  must deliver the fix to us (a distribution package, a pinned build bundle) — a ticket there, or
  the release or commit in which that channel already delivered it. A ticket still to be found or
  filed is listed as **OWED** with the channel it must go to, and an entry with an owed ticket is
  not complete. The only exception is a flaw with no upstream component at all (our own code or
  packaging): `None: <why>`.
- **How it reaches us** — the route from untrusted input to the flaw, per platform.
- **Workaround / patch** — what we do, where it lives, on which platforms, and what it costs a
  benign user.
- **Leaks found and closed** — every later bypass of the workaround: what slipped through, and the
  rule that closed it. Dated.
- **To close for good** — what upstream (or packaging) must deliver per platform, and what we then
  delete.

## Contents

| ID | Status | Severity | CVE / CWE | Vulnerability |
|---|---|---|---|---|
| [ScrSEC-1](#scrsec-1-librsvg-use-after-free-through-an-svgs-xinclude-cve-2026-96889) | Worked around | High | CVE-2026-96889 / CWE-416; workaround leaks CWE-436, CWE-184, CWE-407, CWE-636/693 | librsvg use-after-free through an SVG's XInclude |
| [ScrSEC-2](#scrsec-2-the-macos-bundle-could-load-a-library-placed-beside-the-app) | Patched | High | no CVE / CWE-427 | The macOS bundle could load a library placed beside the `.app` |

---

## ScrSEC-1. librsvg use-after-free through an SVG's XInclude (CVE-2026-96889)

**Status**: Worked around (Linux, Windows; macOS run outside its bundle). Fixed by packaging on
macOS from the bundle.

**Severity**: High. A crafted SVG can corrupt memory. Code execution has a published route only for
a non-PIE glibc executable; ours is PIE on every platform checked, so the realistic worst case is a
crash, with code execution not ruled out.

**Classification**:
- The flaw: [CVE-2026-96889](https://www.cve.org/CVERecord?id=CVE-2026-96889);
  [CWE-416: Use After Free](https://cwe.mitre.org/data/definitions/416.html), reached through
  [CWE-827: Improper Control of Document Type Definition](https://cwe.mitre.org/data/definitions/827.html)
  (an included document redefining an entity).
- Our workaround's leaks (below):
  [CWE-436: Interpretation Conflict](https://cwe.mitre.org/data/definitions/436.html) — every
  parsing leak was the screen reading a `data:` URI or an XML layer differently from the real
  decoder — and
  [CWE-184: Incomplete List of Disallowed Inputs](https://cwe.mitre.org/data/definitions/184.html)
  for the encoding shapes it did not yet undo;
  [CWE-407: Inefficient Algorithmic Complexity](https://cwe.mitre.org/data/definitions/407.html)
  for two checks whose overlapping scans let a crafted SVG hang the app; and, for the three macOS
  stand-down leaks, which read no input at all but decided to stand the screen down on evidence
  that did not establish the fixed decoder was in use,
  [CWE-636: Not Failing Securely](https://cwe.mitre.org/data/definitions/636.html) /
  [CWE-693: Protection Mechanism Failure](https://cwe.mitre.org/data/definitions/693.html). No CVE
  assigned: our own code, patched before publication of each leak.

**Upstream**: GHSA-wq5f-xc86-pv6w / CVE-2026-96889 (2026-10-06), RUSTSEC-2026-0305; librsvg issue
https://gitlab.gnome.org/GNOME/librsvg/-/work_items/1241. Fixed in librsvg **2.63.2**. Mechanism: an
SVG whose nested `xi:include` (a `data:` URI suffices) redefines an XML entity makes librsvg free
that entity while libxml2 is still expanding it.

**Upstream ticket** (checked 2026-10-28):
- **librsvg — pre-existing**: https://gitlab.gnome.org/GNOME/librsvg/-/work_items/1241, with
  GHSA-wq5f-xc86-pv6w and RUSTSEC-2026-0305. Fixed in 2.63.2. Not filed by us: the flaw was public
  when we learned of it.
- **gvsbuild (Windows packaging) — pre-existing, delivered**: no ticket needed; gvsbuild updated its
  librsvg pin itself ("Update librsvg to version 2.63.2", 2026-10-05), and its release **2026.10.0**
  (2026-10-26) ships librsvg 2.63.2 (its `main` already pins 2.63.4). Our CI still pins gvsbuild
  2026.8.0 (librsvg 2.62.3). Moving to 2026.10.0 also moves Windows from GTK 4.22.4 to 4.24.1,
  so it is an operator decision with its own ratification, not a pin edit.
- **Ubuntu (Linux packaging) — pre-existing**: https://ubuntu.com/security/CVE-2026-96889, which
  marks jammy (22.04, librsvg 2.52.5) **Not affected** (read by the operator 2026-10-28). That is
  Ubuntu's assessment, not our measurement, and it covers one distribution release; other
  distributions and releases the app runs on are unassessed here.
- The leaks in our own screen (below) have no upstream. The leaky code itself was public (it was
  pushed before review found each leak); each leak was described in pushed text only in the commit
  that fixed it, and QA's findings live in an unpublished review file.

**How it reaches us**: every image is decoded through gdk-pixbuf's loader chain, which picks
librsvg for SVG by content, not by file name. Any SVG a document references is decoded — local
files beside the document by default, remote ones with "Show Unsafe Images" on, theme sprites, PDF
export. MEASURED on macOS: a `data:` `xi:include` renders through the loader, so the vulnerable
path runs, with no base URL needed.

| Platform | librsvg loaded (checked 2026-10-28) | Fixed upstream for us? |
|---|---|---|
| macOS, from the `.app` | 2.63.2, staged from Homebrew by the bundle build, which fails below 2.63.2 | Yes |
| macOS, binary run outside a bundle | Homebrew's, whatever is installed | Not guaranteed — screened as Linux |
| Windows | 2.62.3, from the gvsbuild GTK zip pinned in CI | No |
| Linux | the distribution's (Ubuntu 22.04: 2.52.5+dfsg-3ubuntu0.2) | Ubuntu 22.04: not affected, per Ubuntu's tracker (vendor assessment, not measured by us); other distributions unassessed |

**Workaround**: the XInclude screen, `src/imagedecode/xinclude.rs` (TDD 2.23c), runs ahead of every
hand-off of encoded image bytes to the loader chain, and a refusal is final. It refuses content
that names the XInclude namespace, declares an entity, carries an internal DTD subset or a non-W3C
DTD, or uses an encoding it cannot read — after undoing every layer a decoder would: gzip, UTF-16,
XML and CSS references, and nested `data:` documents, base64 or percent-encoded, to any depth up to
a bound. It errs towards refusing: a refused image shows the broken-image placeholder and one
status notice. On macOS the screen stands down only when the process runs from a bundle, the
librsvg dyld actually loaded is the bundle's staged, verified one, and the loader cache gdk-pixbuf
reads names no SVG loader but the staged one.

**Leaks found and closed** (QA review rounds; 2026-10-28 unless dated):
- **Round 8.** A nested `data:` URI escaped the screen: a non-base64 payload was cut at the first
  quote, and a header with no comma within a bound was treated as "not a URI". Closed: a header
  that cannot be delimited refuses; a percent payload is screened past literal quotes; base64 scan
  admits all whitespace the decoder skips. ("Fail the XInclude screen closed on data: URIs and
  newline-separated XML declarations")
- **Round 8.** A newline or tab after `<?xml` hid the encoding declaration from the encoding check.
  Closed: every `<?xml…?>` has its `encoding=` checked. (same commit)
- **Round 8 (review note SEC-V2), macOS stand-down.** The stand-down was a compile-time platform
  test, not a check of the librsvg loaded, so a binary run outside its bundle stood down too.
  Closed: stand down only from a bundle carrying the staged librsvg. ("Keep the automount table,
  screen macOS runs outside the bundle, and keep annotations out of code spans")
- **Round 9, macOS stand-down.** A bundle stood down because the staged librsvg was PRESENT, not
  because it was the one loaded. Closed: stand down only when the dyld image list shows the staged
  librsvg is what the staged loader bound. ("Gate every macOS bundle build, fail closed, and decide
  the librsvg from what dyld loaded")
- **Round 9.** Cutting the text at each literal `data:` split one real payload: the bytes up to the
  next comma were screened as nobody's "header", and a URI written literally inside an encoded one
  was decoded once where a real decoder decodes it twice. Closed: the text after the first URI is
  also read whole, layer after layer to a fixpoint; every level of a percent payload is inspected.
  ("Screen a split data: URI as the one payload it may be…", "Inspect a percent-encoded data:
  payload at every level of decoding it has")
- **Round 10.** The decode loops skipped the XML-reference layer a real decoder applies between
  percent layers (`&#37;1F` is one layer from 0x1F), and the whole-text reading never inflated
  gzip. Closed: one shared next-layer step (references, CSS escapes, then percent) in both walks,
  full inspection of every layer, and a gzip level ends a walk so an honest `.svgz` is inflated
  rather than mis-decoded. ("Take the XML reference layer between percent layers…")
- **Round 10.** A header spelled `; base64` was base64 to the decoder but not to the screen.
  Closed then by percent-decoding the header, which was itself wrong: see Round 11. ("Read a data:
  header the way the decoder does…")
- **Round 10, over-refusal (not a leak).** The whole-text readings charged the shared inspection
  budget once per round, so an honest page near the image cap with many percent-encoded icons was
  refused as too large. Closed: those readings charge a pool of their own, the same size as the
  main one, and every charge is checked, so one screen inspects at most the two pools: twice the
  old ceiling. Its tests now assert the
  namespace refusal and pair the double-encoded case with benign twins; the depth exit and both
  pools have tests that fail under mutation. ("Give the XInclude screen's whole-text readings
  their own budget pool…", 2026-10-28)
- **Round 10, macOS bundle.** The screen stood down when the staged SVG loader bound the staged
  librsvg, but gdk-pixbuf reaches that loader only through the bundle's loader cache, installed
  only if a temp-directory write succeeds. On failure gdk-pixbuf read Homebrew's compiled-in cache
  and loaded Homebrew's loader and librsvg as separate images (measured) with the screen down.
  Closed: the stand-down also requires `GDK_PIXBUF_MODULE_FILE` to name a cache whose every SVG
  module is the staged loader. ("Keep the XInclude screen up unless gdk-pixbuf reads the bundle's
  loader cache…")
- **Round 11.** The Round 10 fix percent-decoded the header, but the decoder reads it raw, so
  `%3Bbase64`, `;base64%20`, `;base%364` and a CSS form feed (`;\c base64`) were base64 to the
  screen alone; a base64 scan empties a percent body at its first `&` or `<`, and with a single
  URI nothing read it as percent. A CSS escape in a plain attribute (`;\62 ase64`) did the same
  through the screen's own normalisation. Closed: base64 is decided on the header before any
  percent-decoding, by the
  decoder's rule, any header that could spell `base64` through an encoding gets both readings,
  and a base64 verdict keeps the percent reading too. ("Read a data: header raw, as the decoder
  does…", 2026-10-29)
- **Round 11, a hang.** The DOCTYPE and `<?xml` encoding checks rescanned from every match to
  the end of the text, outside any budget, so `<!doctype ` repeated before one `>`, or `<?xml`
  repeated with no `?>`, was quadratic: at the image cap, a main thread that never returned.
  Closed: the encoding check finds each `?>` once and judges each `encoding` key once, and the
  DOCTYPE scans (which must still start at every match) are charged to the inspection budget, so
  such a text is refused as too large in seconds. ("Bound the XInclude screen's declaration
  checks…", 2026-10-29)
- **Round 11, cost.** The UTF-16 readings were never charged, so the ceiling counted bytes received
  rather than work done, and one load screened the same bytes two or three times (the size probe,
  the decode, the fallback). Closed: every reading is charged before it runs, and a verdict is kept
  per content (its length and a hash under a key chosen per process, never its address), so each
  content is screened once per load. A cap-sized page now costs about 0.1–0.5 s per load in a
  release build, down from 0.25–3 s. ("Charge every UTF-16 reading of the XInclude screen and screen
  one content once per load", 2026-10-29)
- **Round 11, macOS bundle.** The stand-down checked a fixed-name loader cache in the shared temp
  directory that any other launch of any bundle copy could rewrite between the check and
  gdk-pixbuf's read. Closed: each launch writes its own (exclusive create, named by process), and
  the check reads the path this process wrote, never the inherited environment value; the macOS
  pipeline and the bundle gates pass on it, and the built app stands the screen down, failing
  closed with an unwritable temp directory (MEASURED by the `mac` seat). ("Check the loader cache
  this process wrote…", 2026-10-29)
- **Round 12.** The screen ended a `data:` header at the first comma of its normalised text, where
  an escape can produce one (`\2c`, or `&#92;2c`, which the XML parser turns into `\2c`) that the
  decoder, ending the header at the first LITERAL comma, does not see. Under a plain `href`,
  `;a=\2c;base64` was base64 to the decoder and a percent `;base64,…` to the screen. Closed: every
  later comma whose preceding text ends in `base64` gets a base64 reading too (in Round 12 only
  those ending in the decoder's own rule; Round 13 widened it, below).
  ("Read a base64 body at every comma the decoder could end the header on…", 2026-10-29)
- **Round 12, over-refusal (not a leak).** The percent reading kept under a base64 verdict
  re-read a whole base64 raster, so an honest page over about 15 MiB carrying one cost more than
  the budget and was refused. Closed: that reading skips the body's leading base64 run, which
  percent-decoding leaves unchanged and which can hold no markup, keeping its last few bytes.
  ("Stop screening an honest base64 raster twice…", 2026-10-29)
- **Round 13.** The same moved boundary with no `;base64`: the decoder's percent body began at
  the literal comma with gzip's (or UCS-4's) first bytes, while the screen's one percent reading
  began at the escape-made comma, so the bytes between hid the signature and the compressed
  document was screened as text that spelled nothing. Closed: a later comma whose percent-decoded
  payload begins with a signature known by its first bytes gets a percent reading of its own,
  charged before it is built. ("Give a payload known by its first bytes a reading at every comma
  the decoder could end the header on", 2026-10-29)
- **Round 13, stylesheet contexts.** Three routes through text librsvg reads as CSS:
  - A context that expands no references (an `<?xml-stylesheet?>` pseudo-attribute) leaves
    `&amp;` to the decoder, where it ends in the `;` of its base64 rule, while the screen's
    normalised `&base64` has none, so the later-comma reading asked the decoder's rule and missed.
    Closed: any later comma that `base64` precedes gets a base64 reading.
  - The screen undid a CSS hex escape eating only a following space, and kept an escaped form feed,
    where CSS eats any one whitespace (form feed and CR LF included) and an escaped newline or form
    feed stands for nothing. In a stylesheet read with no XML parse (a percent-encoded
    `data:text/css`), a form feed split `base64` for the screen alone. Closed: escapes follow CSS
    Syntax 3, normalising drops form feeds too, and `base` is sought with every space and control
    byte removed.
  - The XML parser drops comments and processing instructions from a `<style>` element's text and
    unwraps CDATA, so markup between two halves of a payload joins them for the stylesheet; the
    screen read the halves apart. Closed: text where such markup has text on both sides is also
    read with all of it removed.
  Reachability of each is unmeasured; every closure only refuses more. ("Read stylesheet text as
  the decoder's CSS and XML parsers hand it over…", 2026-10-29)
- **Round 14.** That joined reading saw less than the stylesheet in three ways. It found markup in
  the normalised text, so an opener written as a reference or CSS escape (`&lt;!--`, `\3c !--`),
  or a `<!--` inside a CDATA section, all text to the parser, swallowed the real split after it
  (an unclosed one, everything to the end); and it removed no tags, where librsvg (by its source)
  keeps only a style element's own character data and drops any child element with all it holds
  (`AAAA<x/>BBBB` is one run). It also re-joined its own output, recursing once per seam the
  removal made. Closed: markup is found in the text as written, with CDATA read as the parser reads
  it, only inside style elements, child elements removed whole; an unclosed comment, instruction
  or tag is kept as text; the joined text is read once and never joined again. Reachability is
  unmeasured. ("Join a style element's text as librsvg receives it, from the markup as written",
  2026-10-29)

**The pattern behind every parsing leak** (the stand-down leaks are the other class, above): each
bypass was a place where the screen parsed a `data:` URI
or an XML layer differently from the real decoder. A differential test — generate encodings,
decode them with a reference decoder, require a refusal whenever the decoded tree contains
XInclude — is planned in [PLAN.xinclude-differential-test.md](PLAN.xinclude-differential-test.md).

**To close for good**:
- **Windows**: move the CI workflow's gvsbuild pin to 2026.10.0 or later (librsvg ≥ 2.63.2 is
  released there), ratified on the `windows` seat for the GTK 4.24 move it brings; measure the
  librsvg version in the installed tree.
- **Linux**: per distribution, either a librsvg ≥ 2.63.2 (or a vendor backport), or a vendor "not
  affected" assessment that we have confirmed by measurement — the published trigger run against
  that distribution's librsvg under a memory checker. Ubuntu 22.04 has the assessment
  (unconfirmed); the screen stays on Linux until the measurement, or until the plan's
  loaded-decoder rule can tell a fixed or unaffected decoder from the rest.
- **Then** delete the screen and everything that exists only for it (the refusal notice, the
  2.23c fixtures that expect refusals) on that platform, and mark this entry Closed. The macOS
  stand-down and its bundle gates stay as long as macOS relies on the staged librsvg.

---

## ScrSEC-2. The macOS bundle could load a library placed beside the `.app`

**Status**: Patched (2026-10-28). No upstream component.

**Severity**: High. Anything able to write a file next to the `.app` (a Downloads folder, a shared
directory) could have a library of its choice loaded into the app — and, specifically, an old
librsvg loaded while ScrSEC-1's screen stood down because the bundle's fixed librsvg was present.

**Classification**: no CVE assigned (our packaging).
[CWE-427: Uncontrolled Search Path Element](https://cwe.mitre.org/data/definitions/427.html) — a
library run path that resolved outside the bundle — and its consequence
[CWE-426: Untrusted Search Path](https://cwe.mitre.org/data/definitions/426.html).

**Upstream**: None. The bundled libraries are Homebrew's, and Homebrew's own run-path search
entries (`LC_RPATH`) are correct for Homebrew; copied into a bundle they point outside it.

**Upstream ticket**: None: our packaging. Homebrew's run paths are correct where Homebrew installs them;
the defect was copying them into a bundle unchanged. Patched before it was described in anything
pushed.

**How it reaches us**: the bundled SVG loader linked `@rpath/librsvg-2.2.dylib` and kept Homebrew's
run paths, one of which resolved to the directory containing the `.app`. dyld searches a library's
own run paths before the executable's, so a `librsvg-2.2.dylib` beside the `.app` was loaded in
place of the staged one. MEASURED by the `mac` seat: before the fix the sibling file loaded; after,
the bundle's own. The build is ad-hoc signed, with no library validation to refuse it.

**Patch**: the bundle build deletes every run path, on every staged object, that resolves outside
the bundle, and fails the build if one survives. The bundle gates now run on every bundle build:
the librsvg check (with its self-test) and the self-contained check after signing, which also
rejects any `@loader_path` or `@executable_path` reference that leaves the bundle or resolves to a
missing file. The gates fail closed when the tools they read with fail. ("Stop the bundle loading
libraries from beside the .app"; "Gate every macOS bundle build, fail closed, and decide the
librsvg from what dyld loaded") TDD and MANUAL-TEST 26.1 carry the check that a librsvg beside the
`.app` is not loaded, with its negative control.

**Leaks found and closed** (QA review rounds; 2026-10-28 unless dated):
- **Round 9.** The self-contained gate never ran on an ordinary bundle build; the gates failed
  open when `otool` or `file` crashed or answered nothing; and `@loader_path` / `@executable_path`
  references were accepted on their prefix. Closed: every build runs every gate, a tool that
  fails or answers nothing fails the gate, and each reference is resolved before it is judged.
  ("Gate every macOS bundle build, fail closed, and decide the librsvg from what dyld loaded")
- **Round 10.** `@rpath/../../../x.dylib` and `/usr/lib/../../opt/homebrew/…` were accepted on their
  lexical prefix, and the path normaliser left rpath components open to pathname globbing.
  Closed: every reference is folded against the bundle (or the OS root) before it is judged, and
  paths are split without globbing. ("Keep the XInclude screen up unless gdk-pixbuf reads the
  bundle's loader cache, and resolve every bundle reference before judging it")
- **Round 11.** The install-ID read returned success on an empty `otool -D` answer, so a tool that
  answered nothing about a library's install ID read as "no ID" and the bundle passed unread.
  Closed: it fails closed when the object's load commands carry an install ID; the librsvg gate's
  self-test holds this with a stub `otool`, and the macOS pipeline passes on it. ("Check the loader
  cache this process wrote, in a file of its own, and fail closed on a missing install ID",
  2026-10-29)

**To close for good**: nothing upstream. This entry stays Patched while the bundle copies
third-party libraries; it is the record of why those gates must not be removed.
