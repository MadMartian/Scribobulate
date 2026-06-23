# Annotations Viewer Test Fixture

This document exercises the annotations sidebar under **real scrolling**. It is long on
purpose: at any given scroll position most annotations are off-screen, so activating
their rows must scroll the document to them and pop the comment card, exactly as
clicking the margin chip would. Open **View ▸ Annotations** (or the toolbar's flag
button) and work down — and up — the list, watching for a row that navigates to the
wrong place, fails to open its card, or leaves the view snapped somewhere unexpected.

Expected comment-bearing rows, in order (everything else must be **absent**):

1. citation needed 2. which dataset 3. General note 4. vs which baseline (table cell)
5. rollback owner 6. staffing 7. define "engagement" 8. p-hacking 9. sample size
10. confounds 11. figure caption 12. units 13. reproducibility 14. peer review
15. data availability 16. conflict of interest 17. preregistration 18. effect size
19. limitations 20. Sign-off

## 1. Near the top

The very first annotation: the earth is {==flat==}{>>citation needed — this is false<<}
according to some. Activating this row from far down the document should jump all the way
back up here (top↔bottom navigation).

A second one close by measures a {==temperature rise==}{>>which dataset? cite the
source<<} over the last decade.

Here is a standalone point comment with no highlighted span.{>>General note: tighten this
whole opening section — it buries the lede.<<}

Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor
incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud
exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure
dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur.

Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt
mollit anim id est laborum. Sed ut perspiciatis unde omnis iste natus error sit
voluptatem accusantium doloremque laudantium, totam rem aperiam.

## 2. Things that must NOT appear in the viewer

Everything in this section renders in the document but must be **absent** from the
annotations list — the list shows only comment-bearing annotations (TDD 20.2).

- A bare highlight with no comment: the sky is {==blue==} today. (Renders highlighted;
  it is a formatting feature, not an annotation — no row, no chip.)
- An insertion: we should {++add a staging gate++} here.
- A deletion: remove {--the manual approval step--} from the flow.
- A substitution: change the cache TTL from {~~30 seconds~>5 seconds~~}.

If any of the four above show up as rows in the sidebar, that is a bug.

Neque porro quisquam est, qui dolorem ipsum quia dolor sit amet, consectetur, adipisci
velit, sed quia non numquam eius modi tempora incidunt ut labore et dolore magnam
aliquam quaerat voluptatem. Ut enim ad minima veniam, quis nostrum exercitationem ullam
corporis suscipit laboriosam, nisi ut aliquid ex ea commodi consequatur.

Quis autem vel eum iure reprehenderit qui in ea voluptate velit esse quam nihil
molestiae consequatur, vel illum qui dolorem eum fugiat quo voluptas nulla pariatur.

## 3. An annotation inside a table cell

The claim below lives inside a table cell — the hardest navigation case (TDD 20.7).
Activating its row should scroll the table into view and pop the card over the correct
cell, holding the post-scroll position (it must not snap back to the table top).

| Metric | Value | Notes |
|--------|-------|-------|
| CTR | {==up 12%==}{>>vs which baseline?<<} | measured weekly |
| Bounce | down 3% | stable |
| Latency | 180 ms | p95 |

At vero eos et accusamus et iusto odio dignissimos ducimus qui blanditiis praesentium
voluptatum deleniti atque corrupti quos dolores et quas molestias excepturi sint
occaecati cupiditate non provident, similique sunt in culpa.

## 4. Operations

Deployment is {==fully automated==}{>>who owns the rollback runbook?<<} and, the authors
claim, low-risk in every region.

The on-call rotation is {==adequately staffed==}{>>staffing: only two engineers cover
weekends — is that adequate?<<} across all three shifts.

Sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium doloremque
laudantium, totam rem aperiam, eaque ipsa quae ab illo inventore veritatis et quasi
architecto beatae vitae dicta sunt explicabo. Nemo enim ipsam voluptatem quia voluptas
sit aspernatur aut odit aut fugit.

Consequuntur magni dolores eos qui ratione voluptatem sequi nesciunt. Neque porro
quisquam est, qui dolorem ipsum quia dolor sit amet. Ut enim ad minima veniam, quis
nostrum exercitationem ullam corporis suscipit laboriosam.

## 5. Methodology

The study reports {==high engagement==}{>>define "engagement" — sessions? clicks? time?<<}
among the treatment group.

Several outcomes were tested and only two were reported.{>>p-hacking: how many outcomes
were measured in total, and were they pre-registered?<<}

The cohort was {==broadly representative==}{>>sample size? 40 respondents is not a
population<<} of the target market.

Dolores et quas molestias excepturi sint occaecati cupiditate non provident, similique
sunt in culpa qui officia deserunt mollitia animi, id est laborum et dolorum fuga. Et
harum quidem rerum facilis est et expedita distinctio.

Nam libero tempore, cum soluta nobis est eligendi optio cumque nihil impedit quo minus
id quod maxime placeat facere possimus, omnis voluptas assumenda est, omnis dolor
repellendus. Temporibus autem quibusdam et aut officiis debitis.

## 6. Analysis

The result held {==after adjustment==}{>>which confounds were adjusted for? list them<<}
for the usual demographic variables.

Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat
nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui
officia deserunt mollit anim id est laborum.

The chart on the next page shows the trend, but {==the axes are unlabeled==}{>>figure
caption: add axis labels and units to Figure 3<<} and hard to read.

Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim
veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo
consequat. Duis aute irure dolor in reprehenderit.

The reported figure of {==3.2×==}{>>units: 3.2× what? relative to which control?<<}
lacks a denominator.

## 7. Reproducibility

The code and data are {==available on request==}{>>reproducibility: "on request" is not
open; deposit in a public repository<<}, which is not the same as open.

Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor
incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud
exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.

The manuscript was {==reviewed internally==}{>>peer review: internal review is not
independent peer review — was it externally refereed?<<} before submission.

Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt
mollit anim id est laborum. Sed ut perspiciatis unde omnis iste natus error sit
voluptatem accusantium doloremque laudantium.

The dataset {==will be shared after publication==}{>>data availability: give a concrete
timeline and repository DOI<<}, the authors state.

## 8. Disclosures

The authors {==declare no conflicts==}{>>conflict of interest: one author is employed by
the vendor whose product this evaluates — disclose it<<} of interest.

Totam rem aperiam, eaque ipsa quae ab illo inventore veritatis et quasi architecto beatae
vitae dicta sunt explicabo. Nemo enim ipsam voluptatem quia voluptas sit aspernatur aut
odit aut fugit, sed quia consequuntur magni dolores.

The analysis plan was {==finalized before data collection==}{>>preregistration: link the
preregistration record if one exists<<}, according to the methods.

Neque porro quisquam est, qui dolorem ipsum quia dolor sit amet, consectetur, adipisci
velit, sed quia non numquam eius modi tempora incidunt ut labore et dolore magnam
aliquam quaerat voluptatem.

## 9. Conclusions

The authors call the effect {==large and practically significant==}{>>effect size: report
the actual effect size and its confidence interval, not just significance<<}.

Ut enim ad minima veniam, quis nostrum exercitationem ullam corporis suscipit laboriosam,
nisi ut aliquid ex ea commodi consequatur. Quis autem vel eum iure reprehenderit qui in
ea voluptate velit esse quam nihil molestiae consequatur.

They acknowledge {==few limitations==}{>>limitations: the discussion lists strengths but
almost no limitations — add a candid limitations paragraph<<} of the design.

At vero eos et accusamus et iusto odio dignissimos ducimus qui blanditiis praesentium
voluptatum deleniti atque corrupti quos dolores et quas molestias excepturi sint
occaecati cupiditate non provident.

## 10. Far at the bottom

One final point comment to close, deliberately the furthest row from the top — activating
it from the very top should scroll all the way down here.{>>Sign-off: revisit the
methodology and disclosures before publishing; do not submit as-is.<<}
