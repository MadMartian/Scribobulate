# Cache Migration — Plan

*Drafted by the agent. Reviewed in place, in this file.*

## Summary

We move the session cache off the local disk and onto the shared store. The
cutover is {==a single deploy with no downtime==}{>>The cold cache is the risk,
not downtime.<<} and the old files are removed a week later.

## Steps

1. Ship the dual-write path behind a flag.
2. Backfill the existing keys overnight.
3. Flip reads to the shared store.
4. {==Delete the local cache directory==}{>>Not in this release — wait out the
   rollback window.<<}

## Risk

The shared store adds a network hop to every read. Measured against the local
disk it costs {==under a millisecond==}{>>p50 on an idle box. Give me p99.<<}, which is well inside our budget.

> Rollback is a flag flip. Nothing to undo.

{>>Only true until step 4 runs.<<}

## Open questions

- [ ] Who owns the backfill job after this ships?
- [x] Do we need per-tenant isolation? — no, the key prefix is enough
